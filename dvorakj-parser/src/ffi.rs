//! C ABI layer for single-file DLL/SO use (`ffi` feature).
//!
//! Only primitives (pointer + length + integer status) and UTF-8 JSON strings
//! cross the boundary. Every `extern "C"` entry catches panics and converts
//! them to [`DjStatus::Panic`]. Returned strings must be freed with
//! [`dvorakj_string_free`].

use std::ffi::{c_char, c_int, CStr, CString};

use crate::json::{error_to_json, report_to_json};
use crate::model::{ParseOptions, ParseReport, ParseResult};

/// C ABI status code. Kept `#[repr(C)]` and stable.
#[repr(C)]
pub enum DjStatus {
    Ok = 0,
    NullPointer = 1,
    InvalidUtf8 = 2,
    DecodeError = 3,
    ParseError = 4,
    Panic = 5,
    SerializeError = 6,
}

/// ABI version for structural / JSON-schema compatibility checks.
const ABI_VERSION: u32 = 1;

/// SAFETY: `source_id` must be NUL-terminated or null. Returns `None` for null.
unsafe fn c_str_to_string(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    Some(CStr::from_ptr(p).to_string_lossy().into_owned())
}

/// String → owned C string. Returns `None` if `s` contains an interior NUL
/// (so the caller can surface `SerializeError` rather than a silent empty).
fn string_into_c(s: String) -> Option<*mut c_char> {
    CString::new(s).ok().map(|c| c.into_raw())
}

/// Write the report/error JSON to `out_json` and return the matching status.
fn emit_result(result: ParseResult<ParseReport>, out_json: *mut *mut c_char) -> DjStatus {
    match result {
        Ok(report) => match report_to_json(&report) {
            Ok(s) => match string_into_c(s) {
                Some(ptr) => {
                    unsafe { *out_json = ptr };
                    DjStatus::Ok
                }
                None => DjStatus::SerializeError,
            },
            Err(_) => DjStatus::SerializeError,
        },
        Err(parse_err) => {
            if let Ok(s) = error_to_json(&parse_err) {
                if let Some(ptr) = string_into_c(s) {
                    unsafe { *out_json = ptr };
                }
            }
            DjStatus::ParseError
        }
    }
}

/// Byte buffer (`.txt` contents) → ParseReport JSON. Requires `encoding`
/// (internal Shift-JIS/UTF-8/BOM detection), so it is double-gated below.
///
/// # Safety
/// `bytes`/`len` must describe a valid readable buffer (or `bytes` null).
/// `source_id` must be NUL-terminated or null. `out_json` must be a valid
/// writable `*mut *mut c_char`. The written pointer must be freed with
/// [`dvorakj_string_free`].
#[cfg(feature = "encoding")]
#[no_mangle]
pub unsafe extern "C" fn dvorakj_parse_json(
    bytes: *const u8,
    len: usize,
    source_id: *const c_char,
    strict: c_int,
    out_json: *mut *mut c_char,
) -> DjStatus {
    if bytes.is_null() || out_json.is_null() {
        return DjStatus::NullPointer;
    }
    let result = std::panic::catch_unwind(|| {
        let buf = unsafe { std::slice::from_raw_parts(bytes, len) };
        let sid = unsafe { c_str_to_string(source_id) }.unwrap_or_default();
        let opts = ParseOptions {
            source_id: Some(sid.clone()),
            keyboard: crate::model::KeyboardLayout::from_source_id(&sid),
            strict: strict != 0,
        };
        crate::decode::parse_bytes(buf, &sid, opts)
    });
    match result {
        Ok(r) => emit_result(r, out_json),
        Err(_) => DjStatus::Panic,
    }
}

/// Already-decoded UTF-8 text → ParseReport JSON (no `encoding` needed).
///
/// # Safety
/// Same contract as [`dvorakj_parse_json`] (`text`/`len` buffer, `source_id`
/// NUL-terminated or null, `out_json` writable; free with [`dvorakj_string_free`]).
#[no_mangle]
pub unsafe extern "C" fn dvorakj_parse_str_json(
    text: *const u8,
    len: usize,
    source_id: *const c_char,
    strict: c_int,
    out_json: *mut *mut c_char,
) -> DjStatus {
    if text.is_null() || out_json.is_null() {
        return DjStatus::NullPointer;
    }
    let decoded = std::panic::catch_unwind(|| {
        let buf = unsafe { std::slice::from_raw_parts(text, len) };
        std::str::from_utf8(buf).map(|s| s.to_string())
    });
    let text = match decoded {
        Ok(Ok(s)) => s,
        Ok(Err(_)) => return DjStatus::InvalidUtf8,
        Err(_) => return DjStatus::Panic,
    };
    let result = std::panic::catch_unwind(|| {
        let sid = unsafe { c_str_to_string(source_id) };
        let opts = ParseOptions {
            keyboard: sid
                .as_deref()
                .map(crate::model::KeyboardLayout::from_source_id)
                .unwrap_or_default(),
            source_id: sid,
            strict: strict != 0,
        };
        crate::parse_str(&text, opts)
    });
    match result {
        Ok(r) => emit_result(r, out_json),
        Err(_) => DjStatus::Panic,
    }
}

/// Free a string returned by `dvorakj_parse_*`.
///
/// # Safety
/// `s` must be a pointer previously written by `dvorakj_parse_*` via `out_json`,
/// or null. Must not be called twice on the same pointer.
#[no_mangle]
pub unsafe extern "C" fn dvorakj_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe { drop(CString::from_raw(s)) };
    }
}

/// Library version string (static, must NOT be freed).
#[no_mangle]
pub extern "C" fn dvorakj_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// ABI version (for structural / JSON-schema back-compat checks).
#[no_mangle]
pub extern "C" fn dvorakj_abi_version() -> u32 {
    ABI_VERSION
}
