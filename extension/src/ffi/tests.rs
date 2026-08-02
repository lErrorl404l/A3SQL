// ── Tests ──────────────────────────────────────────────────────────────────
// The FFI surface is exercised directly with real C pointers (the same shape
// the Arma engine uses). These run under the normal test harness; miri scope
// deliberately excludes the FFI surface.

use crate::ffi::{
    CALLBACK, RVExtension, RVExtensionArgs, RVExtensionRegisterCallback, RVExtensionVersion, write_output,
};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Serialise tests against the crate-global `DB` and `CALLBACK` statics
/// and reset to a clean database.
fn setup() -> std::sync::MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    crate::dispatch::dispatch("reset", &[]);
    g
}

/// Read a NUL-terminated C string. Only safe for the bounded buffers the
/// tests build (zero-initialised, larger than `output_size`).
fn read_cstr(ptr: *const c_char) -> String {
    // SAFETY: callers pass pointers into zero-initialised buffers of
    // known size, and the functions under test always write a terminator.
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

// ── write_output: UTF-8 boundary back-off ──────────────────────────

#[test]
fn write_output_fits_buffer_unchanged() {
    let mut buf = [0i8; 16];
    // SAFETY: test buffer is valid for output_size bytes.
    unsafe { write_output(buf.as_mut_ptr(), 16, "hello") };
    assert_eq!(read_cstr(buf.as_ptr()), "hello");
    assert_eq!(buf[5], 0, "null terminator");
    assert_eq!(buf[6], 0, "no trailing writes");
}

#[test]
fn write_output_exact_fit() {
    let mut buf = [0i8; 16];
    // SAFETY: test buffer is valid for output_size bytes.
    unsafe { write_output(buf.as_mut_ptr(), 6, "hello") };
    assert_eq!(read_cstr(buf.as_ptr()), "hello");
    assert_eq!(buf[5], 0, "null terminator");
}

#[test]
fn write_output_backs_off_2byte_codepoint() {
    // "é" = 2 bytes. output_size 5 allows 4 bytes: lands mid-é, must
    // back off to "ok:" (3 bytes) + null.
    let mut buf = [0i8; 16];
    // SAFETY: test buffer is valid for output_size bytes.
    unsafe { write_output(buf.as_mut_ptr(), 5, "ok:é") };
    assert_eq!(read_cstr(buf.as_ptr()), "ok:");
    assert_eq!(buf[3], 0, "null terminator after back-off");
    assert_eq!(buf[4], 0, "trailing byte untouched");
}

#[test]
fn write_output_backs_off_3byte_codepoint() {
    // "a€b": a(1) + €(3 bytes) + b(1) = 5 bytes. output_size 4 allows 3
    // bytes: lands inside €, must back off to "a" (1 byte) + null.
    let mut buf = [0i8; 16];
    // SAFETY: test buffer is valid for output_size bytes.
    unsafe { write_output(buf.as_mut_ptr(), 4, "a€b") };
    assert_eq!(read_cstr(buf.as_ptr()), "a");
    assert_eq!(buf[1], 0, "null terminator after back-off");
}

#[test]
fn write_output_backs_off_4byte_codepoint() {
    // "𝄞" = 4 bytes. output_size 4 allows 3 bytes: lands inside 𝄞,
    // must back off to "x" (1 byte) + null.
    let mut buf = [0i8; 16];
    // SAFETY: test buffer is valid for output_size bytes.
    unsafe { write_output(buf.as_mut_ptr(), 4, "x𝄞") };
    assert_eq!(read_cstr(buf.as_ptr()), "x");
    assert_eq!(buf[1], 0, "null terminator after back-off");
}

#[test]
fn write_output_truncates_at_boundary_not_inside() {
    // output_size 7 allows 6 bytes of "ab€c" (a b € c = 5 bytes): fits
    // whole, no back-off needed.
    let mut buf = [0i8; 16];
    // SAFETY: test buffer is valid for output_size bytes.
    unsafe { write_output(buf.as_mut_ptr(), 7, "ab€c") };
    assert_eq!(read_cstr(buf.as_ptr()), "ab€c");
    assert_eq!(buf[6], 0, "null terminator");
}

#[test]
fn write_output_size_zero_is_noop() {
    let mut buf = [0x41; 8]; // sentinel, must be untouched
    // SAFETY: null output is always allowed; size 0 means nothing written.
    unsafe { write_output(buf.as_mut_ptr(), 0, "hello") };
    assert_eq!(buf, [0x41; 8], "buffer untouched");
}

#[test]
fn write_output_size_one_is_noop() {
    let mut buf = [0x41; 8];
    // SAFETY: test buffer valid; max len is 0.
    unsafe { write_output(buf.as_mut_ptr(), 1, "hello") };
    assert_eq!(buf, [0x41; 8], "buffer untouched");
}

#[test]
fn write_output_empty_response_is_noop() {
    let mut buf = [0x41; 8];
    // SAFETY: null output is always allowed.
    unsafe { write_output(buf.as_mut_ptr(), 8, "") };
    assert_eq!(buf, [0x41; 8], "empty response leaves buffer untouched");
}

#[test]
fn write_output_null_buffer_is_noop() {
    // SAFETY: write_output tolerates a null output pointer (no-op).
    unsafe { write_output(std::ptr::null_mut(), 8, "hello") };
}

// ── RVExtensionVersion ─────────────────────────────────────────────

#[test]
fn version_written_null_terminated() {
    let mut buf = [0i8; 64];
    // SAFETY: test buffer valid for output_size bytes.
    unsafe { RVExtensionVersion(buf.as_mut_ptr(), 64) };
    let s = read_cstr(buf.as_ptr());
    assert!(s.starts_with("a3sql "), "version: {s}");
}

#[test]
fn version_small_buffer_still_terminated() {
    // output_size 8 < version length: must still null-terminate within
    // the claimed size (previously wrote 8 bytes with no terminator).
    let mut buf = [0i8; 64];
    // SAFETY: test buffer is larger than output_size, so the read stays
    // in-bounds even if the write were buggy.
    unsafe { RVExtensionVersion(buf.as_mut_ptr(), 8) };
    assert_eq!(buf[7], 0, "null terminator inside claimed size");
    let s = read_cstr(buf.as_ptr());
    assert!(s.starts_with("a3sql "), "version prefix: {s}");
    assert!(s.len() <= 7, "no bytes beyond output_size - 1: {s}");
}

#[test]
fn version_size_zero_writes_nothing() {
    let mut buf = [0x41; 8];
    // SAFETY: size 0 means nothing is written.
    unsafe { RVExtensionVersion(buf.as_mut_ptr(), 0) };
    assert_eq!(buf, [0x41; 8], "buffer untouched");
}

// ── RVExtensionArgs pointer-contract handling ──────────────────────

#[test]
fn args_null_output_returns_minus_one() {
    let fn_c = CString::new("ping").unwrap();
    // SAFETY: deliberately violating the contract — null output.
    let rc = unsafe { RVExtensionArgs(std::ptr::null_mut(), 64, fn_c.as_ptr(), std::ptr::null(), 0) };
    assert_eq!(rc, -1);
}

#[test]
fn args_null_function_returns_minus_one() {
    let mut buf = [0i8; 64];
    // SAFETY: deliberately violating the contract — null function.
    let rc = unsafe { RVExtensionArgs(buf.as_mut_ptr(), 64, std::ptr::null(), std::ptr::null(), 0) };
    assert_eq!(rc, -1);
}

#[test]
fn args_sql_null_argv_returns_minus_one() {
    // A null argv on the arma-rs "sql" path used to panic (arma-rs
    // unwraps the argv pointer when argc matches the handler arity).
    let fn_c = CString::new("sql").unwrap();
    let mut buf = [0i8; 256];
    // SAFETY: deliberately violating the contract — null argv.
    let rc = unsafe { RVExtensionArgs(buf.as_mut_ptr(), 256, fn_c.as_ptr(), std::ptr::null(), 1) };
    assert_eq!(rc, -1, "null argv must not panic");
}

// ── RVExtension string path ─────────────────────────────────────────

#[test]
fn string_path_ping() {
    let _g = setup();
    let mut buf = [0i8; 256];
    let fn_c = CString::new("ping").unwrap();
    // SAFETY: test buffer + CString are valid.
    unsafe { RVExtension(buf.as_mut_ptr(), 256, fn_c.as_ptr()) };
    assert!(read_cstr(buf.as_ptr()).contains("PONG"));
}

#[test]
fn string_path_null_function_treated_as_empty() {
    let _g = setup();
    let mut buf = [0i8; 256];
    // SAFETY: RVExtension handles a null function pointer (empty input).
    unsafe { RVExtension(buf.as_mut_ptr(), 256, std::ptr::null()) };
    assert!(read_cstr(buf.as_ptr()).contains("\"OK\""));
}

// ── RVExtensionArgs vanilla + arma-rs paths ─────────────────────────

fn args_call(function: &str, argv: &[&str]) -> (i32, String) {
    let fn_c = CString::new(function).unwrap();
    let argv_c: Vec<CString> = argv.iter().map(|s| CString::new(*s).unwrap()).collect();
    let argv_ptrs: Vec<*const c_char> = argv_c.iter().map(|c| c.as_ptr()).collect();
    let mut buf = vec![0i8; 4096];
    // SAFETY: all pointers are valid, non-null; buf is zero-initialised.
    let rc = unsafe {
        RVExtensionArgs(
            buf.as_mut_ptr(),
            buf.len() as u32,
            fn_c.as_ptr(),
            argv_ptrs.as_ptr(),
            argv_ptrs.len() as u32,
        )
    };
    (rc, read_cstr(buf.as_ptr()))
}

#[test]
fn args_vanilla_path_create_and_select() {
    let _g = setup();
    let (rc, out) = args_call("CREATE TABLE ffi_unit_v (id STRING PRIMARY KEY, val STRING)", &[]);
    assert_eq!(rc, 0, "create: {out}");
    assert!(out.contains("\"OK\""), "{out}");
    let (rc, out) = args_call("INSERT INTO ffi_unit_v VALUES ('a', 'héllo')", &[]);
    assert_eq!(rc, 0, "insert: {out}");
    let (rc, out) = args_call("SELECT val FROM ffi_unit_v WHERE id = 'a'", &[]);
    assert_eq!(rc, 0, "select: {out}");
    assert!(out.contains("héllo"), "utf8 round-trip: {out}");
}

#[test]
fn args_vanilla_path_bind_params() {
    let _g = setup();
    let (rc, out) = args_call("CREATE TABLE ffi_unit_b (id STRING PRIMARY KEY, val INT)", &[]);
    assert_eq!(rc, 0, "create: {out}");
    let (rc, _) = args_call("INSERT INTO ffi_unit_b VALUES ('x', 42)", &[]);
    assert_eq!(rc, 0);
    let (rc, out) = args_call("SELECT val FROM ffi_unit_b WHERE id = $1", &["x"]);
    assert_eq!(rc, 0, "bind: {out}");
    assert!(out.contains("42"), "{out}");
}

#[test]
fn args_arma_rs_sql_path() {
    let _g = setup();
    // argv[0] is the SQF-encoded array string the wrapper produces.
    let (rc, out) = args_call("sql", &[r#"["SELECT 1"]"#]);
    assert_eq!(rc, 0, "sql path: code={rc}, out={out}");
    assert!(out.contains("\"OK\""), "{out}");
}

#[test]
fn args_arma_rs_sql_path_bind_params() {
    let _g = setup();
    let (rc, out) = args_call(
        "sql",
        &[r#"["CREATE TABLE ffi_unit_sq (id STRING PRIMARY KEY, val INT)"]"#],
    );
    assert_eq!(rc, 0, "create: {out}");
    let (rc, out) = args_call("sql", &[r#"["INSERT INTO ffi_unit_sq VALUES ('x', 42)"]"#]);
    assert_eq!(rc, 0, "insert: {out}");
    let (rc, out) = args_call("sql", &[r#"["SELECT val FROM ffi_unit_sq WHERE id = $1", "x"]"#]);
    assert_eq!(rc, 0, "bind: {out}");
    assert!(out.contains("42"), "bind param round-trip: {out}");
}

// ── RVExtensionRegisterCallback ────────────────────────────────────

extern "system" fn probe_callback(_name: *const c_char, _args: *const c_char, _ctx: *const c_char) -> c_int {
    7
}

#[test]
fn register_callback_stores_and_clears() {
    let _g = setup();
    // SAFETY: probe_callback is a valid extern fn pointer.
    unsafe { RVExtensionRegisterCallback(Some(probe_callback)) };
    let stored = CALLBACK.lock().unwrap();
    assert!(stored.is_some(), "callback stored");
    let got = stored.unwrap();
    drop(stored);
    assert_eq!(
        got(std::ptr::null(), std::ptr::null(), std::ptr::null()),
        7,
        "stored callback is the registered fn"
    );
    // SAFETY: None clears the slot.
    unsafe { RVExtensionRegisterCallback(None) };
    assert!(CALLBACK.lock().unwrap().is_none(), "callback cleared");
}
