//! SQLite-backed persistence for connections, sessions, and settings.
//!
//! Uses [`rusqlite`] with bundled SQLite. Stores saved connections,
//! session history, and user preferences.

use rusqlite::{Connection, params};
use std::path::PathBuf;
use std::sync::Mutex;

/// Application database.
pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// Open (or create) the application database at the default location.
    pub fn open() -> Result<Self, String> {
        let db_path = db_path()?;
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| format!("Failed to set WAL mode: {}", e))?;

        let db = Self {
            conn: Mutex::new(conn),
        };

        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS connections (
                id TEXT PRIMARY KEY,
                label TEXT NOT NULL,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                user TEXT NOT NULL,
                auth_method TEXT NOT NULL DEFAULT 'key',
                identity_file TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                connection_id TEXT NOT NULL,
                tool TEXT NOT NULL,
                cwd TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                ended_at TEXT,
                exit_code INTEGER,
                FOREIGN KEY (connection_id) REFERENCES connections(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tap_exchanges (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                connection_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                exchange_id TEXT NOT NULL UNIQUE,
                seq INTEGER NOT NULL DEFAULT 0,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                host TEXT NOT NULL,
                status INTEGER NOT NULL DEFAULT 0,
                req_headers TEXT NOT NULL DEFAULT '{}',
                req_body BLOB,
                resp_headers TEXT NOT NULL DEFAULT '{}',
                resp_body BLOB,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                mode TEXT NOT NULL DEFAULT 'reverse',
                truncated INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_tap_conn ON tap_exchanges(connection_id);
            CREATE INDEX IF NOT EXISTS idx_tap_session ON tap_exchanges(session_id);
            CREATE INDEX IF NOT EXISTS idx_tap_created ON tap_exchanges(created_at);
            ",
        )
        .map_err(|e| format!("Migration error: {}", e))?;

        Ok(())
    }

    /// Save a connection configuration.
    pub fn save_connection(
        &self,
        id: &str,
        label: &str,
        host: &str,
        port: u16,
        user: &str,
        auth_method: &str,
        identity_file: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO connections (id, label, host, port, user, auth_method, identity_file, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
            params![id, label, host, port, user, auth_method, identity_file],
        )
        .map_err(|e| format!("Save error: {}", e))?;
        Ok(())
    }

    /// Load all saved connections.
    pub fn load_connections(&self) -> Result<Vec<ConnectionRecord>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, label, host, port, user, auth_method, identity_file FROM connections ORDER BY updated_at DESC",
            )
            .map_err(|e| format!("Query error: {}", e))?;

        let records = stmt
            .query_map([], |row| {
                Ok(ConnectionRecord {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    host: row.get(2)?,
                    port: row.get(3)?,
                    user: row.get(4)?,
                    auth_method: row.get(5)?,
                    identity_file: row.get(6)?,
                })
            })
            .map_err(|e| format!("Query error: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Delete a connection by ID.
    pub fn delete_connection(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute("DELETE FROM connections WHERE id = ?1", params![id])
            .map_err(|e| format!("Delete error: {}", e))?;
        Ok(())
    }

    /// Get a setting value.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let result = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .ok();
        Ok(result)
    }

    /// Set a setting value.
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| format!("Save error: {}", e))?;
        Ok(())
    }

    // ── Tap exchange persistence ──────────────────────────────────

    /// Insert a captured HTTP exchange into the DB (skip if duplicate).
    pub fn insert_tap_exchange(&self, rec: &TapExchangeRecord) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let req_headers = serde_json::to_string(&rec.req_headers).unwrap_or_default();
        let resp_headers = serde_json::to_string(&rec.resp_headers).unwrap_or_default();
        conn.execute(
            "INSERT OR IGNORE INTO tap_exchanges
             (connection_id, session_id, exchange_id, seq, method, url, host, status,
              req_headers, req_body, resp_headers, resp_body, duration_ms, mode, truncated)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                rec.connection_id, rec.session_id, rec.exchange_id, rec.seq,
                rec.method, rec.url, rec.host, rec.status,
                req_headers, rec.req_body, resp_headers, rec.resp_body,
                rec.duration_ms, rec.mode, rec.truncated,
            ],
        ).map_err(|e| format!("Insert tap exchange: {e}"))?;
        Ok(())
    }

    /// Load exchanges for a connection, newest first, limited.
    pub fn load_tap_exchanges(
        &self,
        connection_id: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<TapExchangeRecord>, String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        let mut stmt = conn
            .prepare(
                "SELECT connection_id, session_id, exchange_id, seq, method, url, host,
                        status, req_headers, req_body, resp_headers, resp_body,
                        duration_ms, mode, truncated
                 FROM tap_exchanges WHERE connection_id = ?1
                 ORDER BY id DESC LIMIT ?2 OFFSET ?3",
            )
            .map_err(|e| format!("Query error: {e}"))?;
        let records = stmt
            .query_map(params![connection_id, limit, offset], |row| {
                let req_headers_str: String = row.get(8)?;
                let resp_headers_str: String = row.get(10)?;
                Ok(TapExchangeRecord {
                    connection_id: row.get(0)?,
                    session_id: row.get(1)?,
                    exchange_id: row.get(2)?,
                    seq: row.get(3)?,
                    method: row.get(4)?,
                    url: row.get(5)?,
                    host: row.get(6)?,
                    status: row.get(7)?,
                    req_headers: serde_json::from_str(&req_headers_str).unwrap_or_default(),
                    req_body: row.get(9)?,
                    resp_headers: serde_json::from_str(&resp_headers_str).unwrap_or_default(),
                    resp_body: row.get(11)?,
                    duration_ms: row.get(12)?,
                    mode: row.get(13)?,
                    truncated: row.get(14)?,
                })
            })
            .map_err(|e| format!("Query error: {e}"))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(records)
    }

    /// Delete all exchanges for a connection.
    pub fn clear_tap_exchanges(&self, connection_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| format!("Lock error: {}", e))?;
        conn.execute(
            "DELETE FROM tap_exchanges WHERE connection_id = ?1",
            params![connection_id],
        ).map_err(|e| format!("Clear error: {e}"))?;
        Ok(())
    }
}

/// A saved connection record from the database.
#[derive(Debug, Clone)]
pub struct ConnectionRecord {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_method: String,
    pub identity_file: Option<String>,
}

/// A captured HTTP exchange stored in the DB.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TapExchangeRecord {
    pub connection_id: String,
    pub session_id: String,
    pub exchange_id: String,
    pub seq: i64,
    pub method: String,
    pub url: String,
    pub host: String,
    pub status: i64,
    pub req_headers: serde_json::Value,
    pub req_body: Vec<u8>,
    pub resp_headers: serde_json::Value,
    pub resp_body: Vec<u8>,
    pub duration_ms: i64,
    pub mode: String,
    pub truncated: bool,
}

/// Get the database file path.
fn db_path() -> Result<PathBuf, String> {
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let app_dir = data_dir.join("remote-ai-ide");
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create data directory: {}", e))?;
    Ok(app_dir.join("app.db"))
}
