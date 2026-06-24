//! Minimal WinHTTP client — Windows only.
//!
//! Uses the system HTTP stack so proxy auto-detection and SSPI authentication
//! (NTLM / Kerberos / Negotiate) are handled transparently, matching the
//! behaviour of Edge / curl(windows) / PowerShell.
//!
//! This module is **synchronous** — callers should wrap it in
//! `tokio::task::spawn_blocking`.

use std::ffi::c_void;
use std::mem;
use std::ptr;

use windows_sys::Win32::Networking::WinHttp::*;

const USER_AGENT: &str = "remote-ai-ide/1.0";

/// Encode a Rust `&str` as a null-terminated UTF-16 wide string.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// RAII wrappers — each calls WinHttpCloseHandle on drop.
// ---------------------------------------------------------------------------

struct HInternet(*mut c_void);

impl HInternet {
    fn check(p: *mut c_void, label: &str) -> Result<Self, String> {
        if p.is_null() {
            Err(format!("{label} failed: GLE={}", unsafe {
                windows_sys::Win32::Foundation::GetLastError()
            }))
        } else {
            Ok(Self(p))
        }
    }
}

impl Drop for HInternet {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse `https://host[:port]/path` into its components.
fn split_url(url: &str) -> Result<(&str, u16, &str, bool), String> {
    let (rest, tls) = if let Some(r) = url.strip_prefix("https://") {
        (r, true)
    } else if let Some(r) = url.strip_prefix("http://") {
        (r, false)
    } else {
        return Err(format!("bad url (no scheme): {url}"));
    };

    let default_port = if tls { 443 } else { 80 };

    let (host_port, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let (host, port) = if let Some(i) = host_port.find(':') {
        let p: u16 = host_port[i + 1..]
            .parse()
            .map_err(|_| format!("bad port: {}", &host_port[i + 1..]))?;
        (&host_port[..i], p)
    } else {
        (host_port, default_port)
    };

    if host.is_empty() {
        return Err(format!("empty host in {url}"));
    }
    Ok((host, port, path, tls))
}

fn last_error() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Perform a synchronous GET request via WinHTTP.
///
/// Returns `(http_status, response_body_bytes)`.
pub fn get(url: &str, headers: &[(&str, &str)]) -> Result<(u16, Vec<u8>), String> {
    let (host, port, path, tls) = split_url(url)?;

    // 1. Session
    let ua = wide(USER_AGENT);
    let session = HInternet::check(
        unsafe {
            WinHttpOpen(
                ua.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                ptr::null(),
                ptr::null(),
                0, // synchronous
            )
        },
        "WinHttpOpen",
    )?;

    // 2. Connect
    let host_w = wide(host);
    let conn = HInternet::check(
        unsafe { WinHttpConnect(session.0, host_w.as_ptr(), port, 0) },
        &format!("WinHttpConnect({host}:{port})"),
    )?;

    // 3. Open request
    let method_w = wide("GET");
    let path_w = wide(path);
    let flags: u32 = if tls { WINHTTP_FLAG_SECURE } else { 0 };
    let req = HInternet::check(
        unsafe {
            WinHttpOpenRequest(
                conn.0,
                method_w.as_ptr(),
                path_w.as_ptr(),
                ptr::null(), // HTTP/1.1
                ptr::null(),
                ptr::null(),
                flags,
            )
        },
        "WinHttpOpenRequest",
    )?;

    // 4. Allow bad certs (TLS-inspecting corporate proxies)
    let mut sec_flags: u32 =
        SECURITY_FLAG_IGNORE_UNKNOWN_CA
        | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID
        | SECURITY_FLAG_IGNORE_CERT_CN_INVALID;
    let ok = unsafe {
        WinHttpSetOption(
            req.0,
            WINHTTP_OPTION_SECURITY_FLAGS,
            &mut sec_flags as *mut _ as *mut c_void,
            mem::size_of::<u32>() as u32,
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpSetOption(SECURITY_FLAGS): GLE={}", last_error()));
    }

    // 5. Add headers
    for (name, value) in headers {
        let hdr = format!("{name}: {value}");
        let hdr_w = wide(&hdr);
        let ok = unsafe {
            WinHttpAddRequestHeaders(
                req.0,
                hdr_w.as_ptr(),
                (hdr_w.len() - 1) as u32, // exclude NUL
                WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
            )
        };
        if ok == 0 {
            return Err(format!("WinHttpAddRequestHeaders({name}): GLE={}", last_error()));
        }
    }

    // 6. Send
    let ok = unsafe {
        WinHttpSendRequest(
            req.0,
            ptr::null(),
            0,
            ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpSendRequest: GLE={}", last_error()));
    }

    // 7. Receive response
    let ok = unsafe { WinHttpReceiveResponse(req.0, ptr::null_mut()) };
    if ok == 0 {
        return Err(format!("WinHttpReceiveResponse: GLE={}", last_error()));
    }

    // 8. Status code
    let mut status: u32 = 0;
    let mut size = mem::size_of::<u32>() as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut status as *mut _ as *mut c_void,
            &mut size,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpQueryHeaders(STATUS): GLE={}", last_error()));
    }

    // 9. Read body
    let mut body = Vec::new();
    loop {
        let mut avail: u32 = 0;
        let ok = unsafe { WinHttpQueryDataAvailable(req.0, &mut avail) };
        if ok == 0 {
            return Err(format!("WinHttpQueryDataAvailable: GLE={}", last_error()));
        }
        if avail == 0 {
            break;
        }
        let mut buf = vec![0u8; avail as usize];
        let mut nread: u32 = 0;
        let ok = unsafe {
            WinHttpReadData(req.0, buf.as_mut_ptr() as *mut c_void, avail, &mut nread)
        };
        if ok == 0 {
            return Err(format!("WinHttpReadData: GLE={}", last_error()));
        }
        body.extend_from_slice(&buf[..nread as usize]);
    }

    Ok((status as u16, body))
}

/// Perform a synchronous POST request via WinHTTP.
///
/// Returns `(http_status, response_body_bytes)`.
pub fn post(
    url: &str,
    headers: &[(&str, &str)],
    post_body: &[u8],
) -> Result<(u16, Vec<u8>), String> {
    let (host, port, path, tls) = split_url(url)?;

    // 1. Session
    let ua = wide(USER_AGENT);
    let session = HInternet::check(
        unsafe {
            WinHttpOpen(
                ua.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                ptr::null(),
                ptr::null(),
                0,
            )
        },
        "WinHttpOpen",
    )?;

    // 2. Connect
    let host_w = wide(host);
    let conn = HInternet::check(
        unsafe { WinHttpConnect(session.0, host_w.as_ptr(), port, 0) },
        &format!("WinHttpConnect({host}:{port})"),
    )?;

    // 3. Open request
    let method_w = wide("POST");
    let path_w = wide(path);
    let flags: u32 = if tls { WINHTTP_FLAG_SECURE } else { 0 };
    let req = HInternet::check(
        unsafe {
            WinHttpOpenRequest(
                conn.0,
                method_w.as_ptr(),
                path_w.as_ptr(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
                flags,
            )
        },
        "WinHttpOpenRequest",
    )?;

    // 4. Allow bad certs
    let mut sec_flags: u32 =
        SECURITY_FLAG_IGNORE_UNKNOWN_CA
        | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID
        | SECURITY_FLAG_IGNORE_CERT_CN_INVALID;
    let ok = unsafe {
        WinHttpSetOption(
            req.0,
            WINHTTP_OPTION_SECURITY_FLAGS,
            &mut sec_flags as *mut _ as *mut c_void,
            mem::size_of::<u32>() as u32,
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpSetOption(SECURITY_FLAGS): GLE={}", last_error()));
    }

    // 5. Add headers
    for (name, value) in headers {
        let hdr = format!("{name}: {value}");
        let hdr_w = wide(&hdr);
        let ok = unsafe {
            WinHttpAddRequestHeaders(
                req.0,
                hdr_w.as_ptr(),
                (hdr_w.len() - 1) as u32,
                WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
            )
        };
        if ok == 0 {
            return Err(format!("WinHttpAddRequestHeaders({name}): GLE={}", last_error()));
        }
    }

    // 6. Send
    let (pbody, len) = if post_body.is_empty() {
        (ptr::null(), 0u32)
    } else {
        (post_body.as_ptr() as *const c_void, post_body.len() as u32)
    };
    let ok = unsafe {
        WinHttpSendRequest(
            req.0,
            ptr::null(),
            0,
            pbody as *mut c_void,
            len,
            len,
            0,
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpSendRequest: GLE={}", last_error()));
    }

    // 7. Receive response
    let ok = unsafe { WinHttpReceiveResponse(req.0, ptr::null_mut()) };
    if ok == 0 {
        return Err(format!("WinHttpReceiveResponse: GLE={}", last_error()));
    }

    // 8. Status code
    let mut status: u32 = 0;
    let mut size = mem::size_of::<u32>() as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            req.0,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(),
            &mut status as *mut _ as *mut c_void,
            &mut size,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpQueryHeaders(STATUS): GLE={}", last_error()));
    }

    // 9. Read body
    let mut body = Vec::new();
    loop {
        let mut avail: u32 = 0;
        let ok = unsafe { WinHttpQueryDataAvailable(req.0, &mut avail) };
        if ok == 0 {
            return Err(format!("WinHttpQueryDataAvailable: GLE={}", last_error()));
        }
        if avail == 0 {
            break;
        }
        let mut buf = vec![0u8; avail as usize];
        let mut nread: u32 = 0;
        let ok = unsafe {
            WinHttpReadData(req.0, buf.as_mut_ptr() as *mut c_void, avail, &mut nread)
        };
        if ok == 0 {
            return Err(format!("WinHttpReadData: GLE={}", last_error()));
        }
        body.extend_from_slice(&buf[..nread as usize]);
    }

    Ok((status as u16, body))
}
