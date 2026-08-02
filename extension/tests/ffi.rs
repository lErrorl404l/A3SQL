// a3sql — FFI boundary integration tests.
//
// Exercises the actual `#[unsafe(no_mangle)] extern "C"` entry points the
// Arma 3 engine loads (RVExtension / RVExtensionArgs / RVExtensionVersion /
// RVExtensionRegisterCallback) with real C pointers — the shape the .so
// receives at runtime. This surface is deliberately excluded from the miri
// scope, so these run only under the native test harness.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::{Mutex, MutexGuard};

static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Serialise against the shared global DB and reset to a clean state.
fn setup() -> MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    a3sql::dispatch("reset", &[]);
    g
}

/// Read a NUL-terminated string from a buffer. Only safe for the
/// zero-initialised, fixed-size buffers the tests build.
fn read_cstr(ptr: *const c_char) -> String {
    // SAFETY: callers pass pointers into zero-initialised buffers whose size
    // bounds the read; the FFI functions under test always write a terminator.
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
}

// ── RVExtensionVersion ─────────────────────────────────────────────────────

#[test]
fn ffi_version_roundtrip() {
    let mut buf = [0i8; 64];
    // SAFETY: buf is a valid 64-byte output buffer.
    unsafe { a3sql::ffi::RVExtensionVersion(buf.as_mut_ptr(), buf.len() as u32) };
    let s = read_cstr(buf.as_ptr());
    assert!(s.starts_with("a3sql "), "version: {s}");
}

#[test]
fn ffi_version_small_buffer_is_null_terminated() {
    // A caller with an undersized buffer must still get a valid C string.
    let mut buf = [0i8; 64];
    // SAFETY: buf is larger than output_size, so reads stay in-bounds even
    // if the write were buggy.
    unsafe { a3sql::ffi::RVExtensionVersion(buf.as_mut_ptr(), 8) };
    assert_eq!(buf[7], 0, "null terminator within claimed size");
    let s = read_cstr(buf.as_ptr());
    assert!(s.starts_with("a3sql "), "version prefix: {s}");
    assert!(s.len() <= 7, "no bytes beyond output_size - 1: {s}");
}

// ── RVExtension (STRING callExtension) ─────────────────────────────────────

/// Call the string entry point and return (output string, raw buffer).
fn string_call(input: &str, output_size: u32) -> (String, Vec<i8>) {
    let c = CString::new(input).unwrap();
    let mut buf = vec![0i8; output_size as usize];
    // SAFETY: c and buf are valid; output_size bounds the write.
    unsafe { a3sql::ffi::RVExtension(buf.as_mut_ptr(), output_size, c.as_ptr()) };
    (read_cstr(buf.as_ptr()), buf)
}

#[test]
fn ffi_string_call_ping() {
    let (out, _) = string_call("ping", 1024);
    assert!(out.contains("PONG"), "{out}");
}

#[test]
fn ffi_string_call_utf8_roundtrip() {
    let _g = setup();
    assert!(
        string_call("CREATE TABLE ffi_int_utf8 (id STRING PRIMARY KEY, name STRING)", 1024)
            .0
            .contains("\"OK\"")
    );
    assert!(
        string_call("INSERT INTO ffi_int_utf8 VALUES ('a', 'héllo wörld')", 1024)
            .0
            .contains("\"OK\"")
    );
    let (out, _) = string_call("SELECT name FROM ffi_int_utf8 WHERE id = 'a'", 1024);
    assert!(out.contains("héllo wörld"), "utf8 round-trip: {out}");
}

#[test]
fn ffi_string_call_null_function_treated_as_empty() {
    let _g = setup();
    let mut buf = vec![0i8; 1024];
    // SAFETY: RVExtension accepts a null function pointer (empty input).
    unsafe { a3sql::ffi::RVExtension(buf.as_mut_ptr(), 1024, std::ptr::null()) };
    assert!(read_cstr(buf.as_ptr()).contains("\"OK\""));
}

// ── Small output buffer: UTF-8 invariant + size cap ───────────────────────

#[test]
fn ffi_string_call_respects_small_buffer_and_stays_valid_utf8() {
    let _g = setup();
    assert!(
        string_call("CREATE TABLE ffi_int_small (id STRING PRIMARY KEY, name STRING)", 1024)
            .0
            .contains("\"OK\"")
    );
    assert!(
        string_call("INSERT INTO ffi_int_small VALUES ('a', 'héllo wörld Ω')", 1024)
            .0
            .contains("\"OK\"")
    );
    // Full response (unbounded) so we know what we are truncating.
    let (full, _) = string_call("SELECT name FROM ffi_int_small WHERE id = 'a'", 4096);
    assert!(full.contains("héllo"), "fixture: {full}");

    // Sweep every output_size from 2 up to the full length: the buffer must
    // always be NUL-terminated, never exceed output_size - 1 content bytes,
    // and never end mid-UTF-8-codepoint.
    // (output_size = 1 is the degenerate "room for nothing" case where
    // write_output no-ops and leaves the buffer untouched — asserted below.)
    for size in 2..=full.len().max(48) {
        let mut buf = vec![0x41; size]; // sentinel: must be overwritten up to the terminator
        let c = CString::new("SELECT name FROM ffi_int_small WHERE id = 'a'").unwrap();
        // SAFETY: buf is exactly `size` bytes; the call may only write <= size.
        unsafe { a3sql::ffi::RVExtension(buf.as_mut_ptr(), size as u32, c.as_ptr()) };
        // Find the terminator within the buffer.
        let term = buf.iter().position(|&b| b == 0).expect("must be NUL-terminated");
        assert!(
            term < size,
            "terminator must sit at <= output_size - 1, got {term} for size {size}"
        );
        let content = &buf[..term];
        assert!(
            std::str::from_utf8(unsafe { std::slice::from_raw_parts(content.as_ptr() as *const u8, content.len()) })
                .is_ok(),
            "truncated output must stay valid UTF-8 at size {size}: {content:?}"
        );
        assert!(
            buf[term + 1..].iter().all(|&b| b == 0x41),
            "bytes after terminator untouched (sentinel 0x41) at size {size}"
        );
    }

    // Degenerate 1-byte buffer: no room for content, so nothing is written.
    let mut buf = [0x41i8; 1];
    let c = CString::new("SELECT name FROM ffi_int_small WHERE id = 'a'").unwrap();
    // SAFETY: buf is a valid 1-byte output buffer.
    unsafe { a3sql::ffi::RVExtension(buf.as_mut_ptr(), 1, c.as_ptr()) };
    assert_eq!(buf, [0x41i8], "1-byte buffer is a no-op (untouched)");
}

// ── RVExtensionArgs (ARRAY callExtension) ──────────────────────────────────

/// Call the array entry point and return (return code, output string).
fn args_call(function: &str, argv: &[&str], output_size: u32) -> (i32, String) {
    let fn_c = CString::new(function).unwrap();
    let argv_c: Vec<CString> = argv.iter().map(|s| CString::new(*s).unwrap()).collect();
    let argv_ptrs: Vec<*const c_char> = argv_c.iter().map(|c| c.as_ptr()).collect();
    let mut buf = vec![0i8; output_size as usize];
    // SAFETY: all pointers valid; buf zero-initialised and sized.
    let rc = unsafe {
        a3sql::ffi::RVExtensionArgs(
            buf.as_mut_ptr(),
            output_size,
            fn_c.as_ptr(),
            argv_ptrs.as_ptr(),
            argv_ptrs.len() as u32,
        )
    };
    (rc, read_cstr(buf.as_ptr()))
}

#[test]
fn ffi_args_vanilla_path_lifecycle() {
    let _g = setup();
    let (rc, out) = args_call("CREATE TABLE ffi_int_args (id STRING PRIMARY KEY, val INT)", &[], 4096);
    assert_eq!(rc, 0, "create: {out}");
    let (rc, out) = args_call("INSERT INTO ffi_int_args VALUES ('a', 10), ('b', 20)", &[], 4096);
    assert_eq!(rc, 0, "insert: {out}");
    let (rc, out) = args_call("SELECT id FROM ffi_int_args ORDER BY val DESC LIMIT 1", &[], 4096);
    assert_eq!(rc, 0, "select: {out}");
    assert!(out.contains("b"), "{out}");
}

#[test]
fn ffi_args_vanilla_path_bind_params() {
    let _g = setup();
    let (rc, _) = args_call("CREATE TABLE ffi_int_bind (id STRING PRIMARY KEY, val INT)", &[], 4096);
    assert_eq!(rc, 0);
    let (rc, _) = args_call("INSERT INTO ffi_int_bind VALUES ('x', 42)", &[], 4096);
    assert_eq!(rc, 0);
    let (rc, out) = args_call("SELECT val FROM ffi_int_bind WHERE id = $1", &["x"], 4096);
    assert_eq!(rc, 0, "bind: {out}");
    assert!(out.contains("42"), "{out}");
}

#[test]
fn ffi_args_arma_rs_sql_path() {
    let _g = setup();
    // argv[0] is the SQF-encoded array string the wrapper produces.
    let (rc, out) = args_call(
        "sql",
        &[r#"["CREATE TABLE ffi_int_sql (id STRING PRIMARY KEY)"]"#],
        4096,
    );
    assert_eq!(rc, 0, "sql create: {out}");
    let (rc, out) = args_call("sql", &[r#"["INSERT INTO ffi_int_sql VALUES ('k')"]"#], 4096);
    assert_eq!(rc, 0, "sql insert: {out}");
    let (rc, out) = args_call("sql", &[r#"["SELECT id FROM ffi_int_sql"]"#], 4096);
    assert_eq!(rc, 0, "sql select: {out}");
    assert!(out.contains("k"), "{out}");
}

#[test]
fn ffi_args_arma_rs_sql_path_bind_params() {
    let _g = setup();
    let (rc, out) = args_call(
        "sql",
        &[r#"["CREATE TABLE ffi_int_sqlb (id STRING PRIMARY KEY, val INT)"]"#],
        4096,
    );
    assert_eq!(rc, 0, "create: {out}");
    let (rc, out) = args_call("sql", &[r#"["INSERT INTO ffi_int_sqlb VALUES ('y', 7)"]"#], 4096);
    assert_eq!(rc, 0, "insert: {out}");
    let (rc, out) = args_call("sql", &[r#"["SELECT val FROM ffi_int_sqlb WHERE id = $1", "y"]"#], 4096);
    assert_eq!(rc, 0, "bind: {out}");
    assert!(out.contains("7"), "{out}");
}

#[test]
fn ffi_args_unknown_function_goes_through_dispatch() {
    let _g = setup();
    // Any non-"sql" function string falls through to the vanilla dispatch
    // path — including commands the SQF wrapper issues.
    let (rc, out) = args_call("version", &[], 4096);
    assert_eq!(rc, 0, "version: {out}");
    assert!(out.contains("a3sql"), "{out}");
}

#[test]
fn ffi_args_wrong_arg_count_vanilla_is_graceful() {
    let _g = setup();
    // argc=0 with argv present: vanilla path, dispatch with no args.
    let (rc, out) = args_call("ping", &[], 4096);
    assert_eq!(rc, 0, "{out}");
    assert!(out.contains("PONG"), "{out}");
}

// ── Pointer-contract violations ────────────────────────────────────────────

#[test]
fn ffi_args_null_output_returns_minus_one() {
    let fn_c = CString::new("ping").unwrap();
    // SAFETY: deliberately violating the contract — null output.
    let rc = unsafe { a3sql::ffi::RVExtensionArgs(std::ptr::null_mut(), 64, fn_c.as_ptr(), std::ptr::null(), 0) };
    assert_eq!(rc, -1);
}

#[test]
fn ffi_args_null_function_returns_minus_one() {
    let mut buf = [0i8; 64];
    // SAFETY: deliberately violating the contract — null function.
    let rc = unsafe { a3sql::ffi::RVExtensionArgs(buf.as_mut_ptr(), 64, std::ptr::null(), std::ptr::null(), 0) };
    assert_eq!(rc, -1);
}

#[test]
fn ffi_args_sql_null_argv_returns_minus_one() {
    // A null argv on the arma-rs "sql" path used to panic across the FFI
    // boundary (arma-rs unwraps argv when argc matches the handler arity).
    let fn_c = CString::new("sql").unwrap();
    let mut buf = [0i8; 256];
    // SAFETY: deliberately violating the contract — null argv with argc=1.
    let rc = unsafe { a3sql::ffi::RVExtensionArgs(buf.as_mut_ptr(), 256, fn_c.as_ptr(), std::ptr::null(), 1) };
    assert_eq!(rc, -1, "null argv must not panic");
}

#[test]
fn ffi_args_sql_null_argv_zero_argc_returns_minus_one() {
    let fn_c = CString::new("sql").unwrap();
    let mut buf = [0i8; 256];
    // SAFETY: deliberately violating the contract — null argv regardless of argc.
    let rc = unsafe { a3sql::ffi::RVExtensionArgs(buf.as_mut_ptr(), 256, fn_c.as_ptr(), std::ptr::null(), 0) };
    assert_eq!(rc, -1);
}

// ── RVExtensionRegisterCallback ────────────────────────────────────────────

extern "system" fn probe_callback(_name: *const c_char, _args: *const c_char, _ctx: *const c_char) -> c_int {
    7
}

#[test]
fn ffi_register_callback_smoke() {
    // Registering and clearing the callback must not disturb the engine.
    // Storage verification lives in the dir.rs unit tests (CALLBACK is
    // crate-private); here we only prove the exported ABI call is safe.
    let _g = setup();
    // SAFETY: probe_callback is a valid extern fn pointer.
    unsafe { a3sql::ffi::RVExtensionRegisterCallback(Some(probe_callback)) };
    let (out, _) = string_call("ping", 1024);
    assert!(out.contains("PONG"), "call still works while callback installed: {out}");
    // SAFETY: None clears the slot.
    unsafe { a3sql::ffi::RVExtensionRegisterCallback(None) };
}
