//! Minimal WinHTTP GET client for the OTA loader — Windows only.
//!
//! Uses the system HTTP stack so proxy auto-detection and SSPI
//! authentication work transparently, same as Edge / PowerShell.
//! This module is synchronous (blocking) which matches the loader's style.

use std::ffi::c_void;
use std::mem;
use std::ptr;
use windows_sys::Win32::Networking::WinHttp::*;

const USER_AGENT: &str = "ota-loader/0.1";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

struct HInternet(*mut c_void);

impl HInternet {
    fn check(p: *mut c_void, label: &str) -> Result<Self, String> {
        if p.is_null() {
            Err(format!("{label}: GLE={}", unsafe { win32_last_error() }))
        } else {
            Ok(Self(p))
        }
    }
}

impl Drop for HInternet {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { WinHttpCloseHandle(self.0); }
        }
    }
}

fn win32_last_error() -> u32 {
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

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
        let p: u16 = host_port[i + 1..].parse().map_err(|_| format!("bad port"))?;
        (&host_port[..i], p)
    } else {
        (host_port, default_port)
    };
    if host.is_empty() {
        return Err(format!("empty host in {url}"));
    }
    Ok((host, port, path, tls))
}

/// Perform a synchronous GET request via WinHTTP.
/// Returns the response body bytes on success (status 2xx).
pub fn get(url: &str) -> Result<Vec<u8>, String> {
    let (host, port, path, tls) = split_url(url)?;

    // 1. Session — automatic proxy detection
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
        "WinHttpConnect",
    )?;

    // 3. Open request
    let method_w = wide("GET");
    let path_w = wide(path);
    let flags: u32 = if tls { WINHTTP_FLAG_SECURE } else { 0 };
    let req = HInternet::check(
        unsafe {
            WinHttpOpenRequest(
                conn.0, method_w.as_ptr(), path_w.as_ptr(),
                ptr::null(), ptr::null(), ptr::null(), flags,
            )
        },
        "WinHttpOpenRequest",
    )?;

    // 4. Allow bad certs (TLS-intercepting proxies)
    let mut sec_flags: u32 =
        SECURITY_FLAG_IGNORE_UNKNOWN_CA
        | SECURITY_FLAG_IGNORE_CERT_DATE_INVALID
        | SECURITY_FLAG_IGNORE_CERT_CN_INVALID;
    let ok = unsafe {
        WinHttpSetOption(
            req.0, WINHTTP_OPTION_SECURITY_FLAGS,
            &mut sec_flags as *mut _ as *mut c_void,
            mem::size_of::<u32>() as u32,
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpSetOption(SECURITY): GLE={}", win32_last_error()));
    }

    // 5. Send
    let ok = unsafe { WinHttpSendRequest(req.0, ptr::null(), 0, ptr::null_mut(), 0, 0, 0) };
    if ok == 0 {
        return Err(format!("WinHttpSendRequest: GLE={}", win32_last_error()));
    }

    // 6. Receive response
    let ok = unsafe { WinHttpReceiveResponse(req.0, ptr::null_mut()) };
    if ok == 0 {
        return Err(format!("WinHttpReceiveResponse: GLE={}", win32_last_error()));
    }

    // 7. Status code
    let mut status: u32 = 0;
    let mut size = mem::size_of::<u32>() as u32;
    let ok = unsafe {
        WinHttpQueryHeaders(
            req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            ptr::null(), &mut status as *mut _ as *mut c_void, &mut size, ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!("WinHttpQueryHeaders(STATUS): GLE={}", win32_last_error()));
    }
    let status = status as u16;

    // 8. Read body
    let mut body = Vec::new();
    loop {
        let mut avail: u32 = 0;
        let ok = unsafe { WinHttpQueryDataAvailable(req.0, &mut avail) };
        if ok == 0 {
            return Err(format!("WinHttpQueryDataAvailable: GLE={}", win32_last_error()));
        }
        if avail == 0 { break; }
        let mut buf = vec![0u8; avail as usize];
        let mut nread: u32 = 0;
        let ok = unsafe {
            WinHttpReadData(req.0, buf.as_mut_ptr() as *mut c_void, avail, &mut nread)
        };
        if ok == 0 {
            return Err(format!("WinHttpReadData: GLE={}", win32_last_error()));
        }
        body.extend_from_slice(&buf[..nread as usize]);
    }

    if status < 200 || status >= 300 {
        return Err(format!("HTTP {status}"));
    }
    Ok(body)
}
