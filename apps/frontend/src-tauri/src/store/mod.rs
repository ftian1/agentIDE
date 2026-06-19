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

/// Get the database file path.
fn db_path() -> Result<PathBuf, String> {
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    let app_dir = data_dir.join("remote-ai-ide");
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("Failed to create data directory: {}", e))?;
    Ok(app_dir.join("app.db"))
}
