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

// ── Output-buffer overflow: fail-loud, never silent truncation ────────────

/// Number of data rows in a SELECT response (`[0,"OK",[["col1"],[val1],...]]`).
fn row_count(resp: &str) -> usize {
    resp.matches("],[").count()
}

#[test]
fn ffi_overflow_returns_err_envelope_with_byte_counts_and_cursor_hint() {
    let _g = setup();
    // Fixture: ~5.5KB response — past any small caller buffer, but well under
    // dispatch's own 30KB guard so the FFI boundary is what overflows.
    a3sql::dispatch("CREATE TABLE ffi_ovf (id INT PRIMARY KEY, val STRING)", &[]);
    for batch in 0..5 {
        let vals: String = (0..20)
            .map(|i| {
                let n = batch * 20 + i;
                format!("({}, 'padding_{}_abcdefghijklmnopqrstuvwxyz0123456789')", n, n)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let r = a3sql::dispatch(&format!("INSERT INTO ffi_ovf VALUES {}", vals), &[]);
        assert!(r.starts_with("[0,"), "insert batch {batch}: {r}");
    }
    let (full, _) = string_call("SELECT * FROM ffi_ovf ORDER BY id", 16384);
    assert!(
        full.len() > 256,
        "fixture must overflow a small buffer: len={}",
        full.len()
    );

    // Small caller buffer: must be an ERR_EXEC envelope, not truncated JSON.
    let (out, _) = string_call("SELECT * FROM ffi_ovf ORDER BY id", 256);
    assert!(out.starts_with("[-1,"), "must be an error envelope: {out}");
    assert!(out.contains("ERR_EXEC"), "{out}");
    assert!(out.contains("Result exceeds output buffer"), "{out}");
    assert!(
        out.contains(&format!("{} bytes > 256 limit", full.len())),
        "byte counts must show response size and output_size: {out}"
    );
    assert!(out.contains("cursor create"), "cursor paging hint: {out}");
    assert!(out.contains("cursor fetch"), "cursor paging hint: {out}");
}

#[test]
fn ffi_args_overflow_returns_err_envelope() {
    let _g = setup();
    a3sql::dispatch("CREATE TABLE ffi_ovf2 (id INT PRIMARY KEY, val STRING)", &[]);
    let vals: Vec<String> = (0..4)
        .map(|i| {
            format!(
                "({}, 'abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789')",
                i
            )
        })
        .collect();
    let r = a3sql::dispatch(&format!("INSERT INTO ffi_ovf2 VALUES {}", vals.join(", ")), &[]);
    assert!(r.starts_with("[0,"), "insert: {r}");
    // output_size 200 fits the ~170-byte ERR_EXEC envelope but not the
    // ~310-byte response: RVExtensionArgs must surface the full envelope.
    let (rc, out) = args_call("SELECT * FROM ffi_ovf2", &[], 200);
    assert_eq!(rc, 0, "ABI return code unchanged: {out}");
    assert!(out.starts_with("[-1,"), "{out}");
    assert!(out.contains("ERR_EXEC"), "{out}");
    assert!(out.contains("Result exceeds output buffer"), "{out}");
    assert!(out.contains("cursor create"), "{out}");
}

#[test]
fn ffi_exact_fit_returns_full_response_byte_identical() {
    let _g = setup();
    a3sql::dispatch("CREATE TABLE ffi_fit (id INT PRIMARY KEY, val STRING)", &[]);
    let r = a3sql::dispatch("INSERT INTO ffi_fit VALUES (1, 'héllo'), (2, 'wörld')", &[]);
    assert!(r.starts_with("[0,"), "insert: {r}");
    let (full, _) = string_call("SELECT * FROM ffi_fit ORDER BY id", 4096);
    assert!(full.starts_with("[0,"), "fixture: {full}");

    // output_size = full.len() + 1 leaves room for the full response + null.
    let (out, _) = string_call("SELECT * FROM ffi_fit ORDER BY id", full.len() as u32 + 1);
    assert_eq!(out, full, "exact fit must be byte-identical");

    // One byte less must NOT fit — the same query now errors loudly.
    let (out, _) = string_call("SELECT * FROM ffi_fit ORDER BY id", full.len() as u32);
    assert!(out.starts_with("[-1,"), "one byte short must overflow: {out}");
    assert!(out.contains("ERR_EXEC"), "{out}");
}

#[test]
fn ffi_cursor_paging_round_trips_large_table() {
    let _g = setup();
    // ~46KB of data — a single SELECT cannot fit Arma's buffer.
    a3sql::dispatch("CREATE TABLE ffi_big (id INT PRIMARY KEY, val STRING)", &[]);
    let total = 420;
    for batch in 0..(total / 20) {
        let vals: String = (0..20)
            .map(|i| {
                let n = batch * 20 + i;
                format!("({}, '{}')", n, "x".repeat(100))
            })
            .collect::<Vec<_>>()
            .join(", ");
        let r = a3sql::dispatch(&format!("INSERT INTO ffi_big VALUES {}", vals), &[]);
        assert!(r.starts_with("[0,"), "insert batch {batch}: {r}");
    }

    // Direct SELECT of the whole table must fail loud, never truncate.
    let (out, _) = string_call("SELECT * FROM ffi_big ORDER BY id", 4096);
    assert!(out.starts_with("[-1,"), "oversized select must error: {out}");
    assert!(out.contains("ERR_"), "error envelope: {out}");

    // Page through with a cursor: every inserted row must come back.
    let (out, _) = string_call("cursor create ffi_big_cur SELECT * FROM ffi_big ORDER BY id", 16384);
    assert!(out.starts_with("[0,"), "cursor create: {out}");
    let mut fetched = 0;
    loop {
        let (out, _) = string_call("cursor fetch ffi_big_cur 100", 16384);
        assert!(out.starts_with("[0,"), "cursor fetch page: {out}");
        let rows = row_count(&out);
        if rows == 0 {
            break;
        }
        fetched += rows;
    }
    assert_eq!(
        fetched, total,
        "cursor paging must round-trip every row (fetched {fetched} of {total})"
    );
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
