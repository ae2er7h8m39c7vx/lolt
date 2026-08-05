//! C ABI for the macOS widget.
//!
//! One entry point: hand it a league and a team, get JSON back. The Swift side
//! never sees a Rust type, only a NUL-terminated UTF-8 string it must hand back
//! to [`lolt_string_free`].
//!
//! Every function is panic-safe — a panic unwinding across the FFI boundary is
//! undefined behaviour, so all of them are caught and reported as an error
//! payload instead.

use std::ffi::{c_char, c_ulonglong, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde::Serialize;

use crate::api::{Api, DEFAULT_API_KEY};
use crate::snapshot::{self, Snapshot};

/// What the widget decodes: `ok` tells it which of the other two to read.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    snapshot: Option<Snapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl Response {
    fn failed(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            snapshot: None,
            error: Some(message.into()),
        }
    }
}

/// Fetch a snapshot and return it as JSON.
///
/// `league` and `team` are NUL-terminated UTF-8; either may be NULL to accept
/// the default (`lck` / `t1`). `timeout_secs` of 0 means "use the default".
///
/// Returns an owned C string that is never NULL, unless allocation itself
/// failed. The caller owns it and must release it with [`lolt_string_free`].
///
/// # Safety
///
/// `league` and `team` must each be NULL or a valid NUL-terminated string that
/// stays alive for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn lolt_snapshot_json(
    league: *const c_char,
    team: *const c_char,
    timeout_secs: c_ulonglong,
) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| {
        let league = match borrow_str(league, "lck") {
            Ok(v) => v,
            Err(e) => return Response::failed(e),
        };
        let team = match borrow_str(team, "t1") {
            Ok(v) => v,
            Err(e) => return Response::failed(e),
        };
        let timeout = if timeout_secs == 0 { 20 } else { timeout_secs };

        match Api::new(DEFAULT_API_KEY, "en-US", timeout)
            .and_then(|api| snapshot::build(&api, &league, &team, 3, 5))
        {
            Ok(snapshot) => Response {
                ok: true,
                snapshot: Some(snapshot),
                error: None,
            },
            // `{:#}` unrolls the anyhow context chain into one sentence.
            Err(err) => Response::failed(format!("{err:#}")),
        }
    }))
    .unwrap_or_else(|_| Response::failed("the tracker panicked while building the snapshot"));

    into_c_string(&response)
}

/// Release a string produced by this library. NULL is ignored.
///
/// # Safety
///
/// `ptr` must be NULL or a pointer previously returned by this library, and
/// must not be freed twice.
#[no_mangle]
pub unsafe extern "C" fn lolt_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

/// Version of the core, as a static string. Never NULL, never freed.
#[no_mangle]
pub extern "C" fn lolt_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Borrow a C string, falling back to `default` when it is NULL or empty.
unsafe fn borrow_str(ptr: *const c_char, default: &str) -> Result<String, String> {
    if ptr.is_null() {
        return Ok(default.to_string());
    }
    match CStr::from_ptr(ptr).to_str() {
        Ok(s) if s.trim().is_empty() => Ok(default.to_string()),
        Ok(s) => Ok(s.trim().to_string()),
        Err(_) => Err("arguments must be valid UTF-8".to_string()),
    }
}

fn into_c_string(response: &Response) -> *mut c_char {
    // Serialising a Snapshot cannot fail; the fallback keeps the contract that
    // callers always receive decodable JSON.
    let json = serde_json::to_string(response).unwrap_or_else(|_| {
        r#"{"ok":false,"error":"could not serialise the snapshot"}"#.to_string()
    });
    match CString::new(json) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips a pointer through the free function the widget calls.
    #[test]
    fn strings_survive_the_boundary_and_free_cleanly() {
        let response = Response::failed("boom");
        let ptr = into_c_string(&response);
        assert!(!ptr.is_null());

        let json = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string();
        assert_eq!(json, r#"{"ok":false,"error":"boom"}"#);

        unsafe { lolt_string_free(ptr) };
        unsafe { lolt_string_free(std::ptr::null_mut()) }; // NULL is a no-op
    }

    #[test]
    fn missing_arguments_fall_back_to_the_defaults() {
        let default = unsafe { borrow_str(std::ptr::null(), "lck") }.unwrap();
        assert_eq!(default, "lck");

        let blank = CString::new("   ").unwrap();
        assert_eq!(unsafe { borrow_str(blank.as_ptr(), "t1") }.unwrap(), "t1");

        let given = CString::new(" g2-esports ").unwrap();
        assert_eq!(
            unsafe { borrow_str(given.as_ptr(), "t1") }.unwrap(),
            "g2-esports"
        );
    }

    #[test]
    fn version_is_readable_from_c() {
        let version = unsafe { CStr::from_ptr(lolt_version()) }.to_str().unwrap();
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }
}
