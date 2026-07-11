//! FFI round-trip: drive the C ABI from Rust, parse the JSON, free the string.
//! Run with: cargo test -p dvorakj-parser --features ffi

#![cfg(feature = "ffi")]

use dvorakj_parser::ffi::{
    dvorakj_abi_version, dvorakj_parse_str_json, dvorakj_string_free, DjStatus,
};
use std::ffi::{c_char, CStr, CString};
use std::ptr;

fn parse(text: &str, source_id: &str, strict: i32) -> (DjStatus, Option<String>) {
    let sid = CString::new(source_id).unwrap();
    let mut out: *mut c_char = ptr::null_mut();
    let st = unsafe {
        dvorakj_parse_str_json(
            text.as_ptr(),
            text.len(),
            sid.as_ptr(),
            strict,
            &mut out as *mut *mut c_char,
        )
    };
    let json = if out.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        unsafe { dvorakj_string_free(out) };
        Some(s)
    };
    (st, json)
}

fn is_ok(st: DjStatus) -> bool {
    matches!(st, DjStatus::Ok)
}

#[test]
fn parse_str_json_roundtrip() {
    let text = "名前\n[\nq|w|e\n]\n";
    let (st, json) = parse(text, "toy.jp.txt", 0);
    assert!(is_ok(st), "status must be Ok");
    let json = json.expect("Ok must produce JSON");
    assert!(json.contains("\"ok\":true"), "json: {json}");
    assert!(json.contains("\"schema_version\":1"));
    assert!(json.contains("\"name\":\"名前\""));
}

#[test]
fn null_pointer_is_reported() {
    let mut out: *mut c_char = ptr::null_mut();
    let st = unsafe {
        dvorakj_parse_str_json(ptr::null(), 0, ptr::null(), 0, &mut out as *mut *mut c_char)
    };
    assert!(matches!(st, DjStatus::NullPointer));
}

#[test]
fn invalid_utf8_is_reported() {
    let bytes = [0xff, 0xfe, 0x00];
    let mut out: *mut c_char = ptr::null_mut();
    let st = unsafe {
        dvorakj_parse_str_json(
            bytes.as_ptr(),
            bytes.len(),
            ptr::null(),
            0,
            &mut out as *mut *mut c_char,
        )
    };
    assert!(matches!(st, DjStatus::InvalidUtf8));
}

#[test]
fn abi_version_is_stable() {
    assert_eq!(dvorakj_abi_version(), 1);
}
