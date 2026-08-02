// a3sql — C ABI entry points
//
// Build targets:
//   Linux:   x86_64-unknown-linux-gnu, i686-unknown-linux-gnu
//   Windows: x86_64-pc-windows-gnu,     i686-pc-windows-gnu
//
// 32-bit Windows mingw needs `-Wl,--kill-at` (set in .cargo/config.toml) to
// strip stdcall @N decoration so Arma can find RVExtension/RVExtensionArgs.
//
// Binary naming (post-build):
//   extension/target/release/liba3sql.so  →  a3sql_x64.so      (Linux x86_64)
//   extension/target/release/a3sql.dll    →  a3sql_x64.dll     (Windows x86_64)
//   target/i686-unknown-linux-gnu/release/liba3sql.so  →  a3sql.so   (Linux i686)
//   target/i686-pc-windows-gnu/release/a3sql.dll       →  a3sql.dll  (Windows i686)
//
// See tools/copy_ext_binaries.sh for the canonical rename script.

//! C ABI entry points — RVExtension, RVExtensionArgs, RVExtensionVersion.
//! These are the interface between the Arma 3 engine and a3sql.
//!
//! Command routing and testing infrastructure provided by [`arma_rs`].

use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::sync::{LazyLock, Mutex, Once};

use arma_rs::Extension;

use crate::dispatch;
use crate::engine;
use crate::engine::error::{ErrorCode, error_response};

/// SQF callback ABI — mirrors [`arma_rs::Callback`](https://docs.rs/arma-rs/latest/arma_rs/type.Callback.html):
/// `extern "system" fn(*const c_char, *const c_char, *const c_char) -> c_int`.
/// arma-rs does not re-export the type (it is `#[doc(hidden)]`), so it is
/// defined locally to keep the [`CALLBACK`] static and
/// [`RVExtensionRegisterCallback`] in lockstep with the engine's calling
/// convention.
pub(crate) type Callback = extern "system" fn(*const c_char, *const c_char, *const c_char) -> c_int;

/// Global database instance (single-threaded, mutex-protected).
pub(crate) static DB: LazyLock<Mutex<engine::Database>> = LazyLock::new(|| Mutex::new(engine::Database::new()));

/// Optional pointer to the SQF callback function registered by the engine.
///
/// # Security boundary
/// This callback is hijackable by co-loaded extensions that can call
/// `RVExtensionRegisterCallback` before we do. This is accepted because
/// co-loaded extensions already run in the same process with the same
/// privileges — they can read/write any memory reachable from the DLL.
/// The callback pointer is a convenience, not a security boundary.
pub(crate) static CALLBACK: LazyLock<Mutex<Option<Callback>>> = LazyLock::new(|| Mutex::new(None));
// ponytail: external TCP listener — global lock on a single listener
pub(crate) static LISTENER: LazyLock<Mutex<Option<std::net::TcpListener>>> = LazyLock::new(|| Mutex::new(None));

/// Stored credentials for TCP authentication. Empty = anonymous access.
pub(crate) static CREDENTIALS: LazyLock<Mutex<(String, String)>> =
    LazyLock::new(|| Mutex::new((String::new(), String::new())));
pub(crate) static REMOTE: LazyLock<Mutex<Option<std::net::TcpStream>>> = LazyLock::new(|| Mutex::new(None));

/// Output buffer size from Arma engine. 30 KB matches Arma 3 v2.20's
/// `callExtension` ceiling — bigger result sets fit without round-trips.
///
/// # FFI output-buffer cap (fail-loud, never silent data loss)
/// `OUTPUT_BUF_SIZE` is the *caller-supplied* buffer ceiling, not a server
/// limit: the REPL server is unbounded and returns full result sets. A host
/// that passes a smaller `output_size` to [`write_output`](fn.write_output)
/// now receives an `ERR_EXEC` envelope (with byte counts and a cursor-paging
/// hint) instead of a silently truncated response — see
/// [`write_response`](fn.write_response). Arma always passes the full 30 KB,
/// so the truncation path is latent, not live, but when it fires it is loud.
#[allow(
    dead_code,
    reason = "OUTPUT_BUF_SIZE is a public constant checked in dispatch.rs and passed to Arma via RVExtensionArgs"
)]
pub(crate) const OUTPUT_BUF_SIZE: u32 = 30720;

/// Version string — max 32 bytes including null terminator.
/// Derived from Cargo.toml so they stay in sync.
static VERSION: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
fn version_bytes() -> &'static [u8] {
    VERSION.get_or_init(|| {
        let mut v = format!("a3sql {}\0", env!("CARGO_PKG_VERSION")).into_bytes();
        v.truncate(32);
        v
    })
}

// ── Arma-rs Extension ─────────────────────────────────────────────────────────

/// Lazy-initialized [`arma_rs::Extension`]. Built exactly once on first C ABI
/// call, then reused for the lifetime of the extension.
///
/// `Extension` contains `Rc` (from arma-rs's context manager) and is therefore
/// `!Send + !Sync`, which rules out [`OnceLock`](std::sync::OnceLock) and
/// [`Mutex`]. Instead we use [`Once`] + `static mut` — the standard Rust pattern
/// for `!Send` statics. Safety rests on:
/// - [`Once`] guarantees single-threaded initialisation.
/// - After init the value is never mutated, only immutably borrowed.
/// - Arma always calls extensions from a single thread.
static INIT: Once = Once::new();
static mut RV_EXTENSION: Option<Extension> = None;

/// Acquire the extension, initialising it on first access.
fn with_extension<F, R>(f: F) -> R
where
    F: FnOnce(&Extension) -> R,
{
    INIT.call_once(|| {
        // SAFETY: Called exactly once via Once.
        unsafe { RV_EXTENSION = Some(build_extension()) }
    });
    // SAFETY: After `INIT.call_once`, `RV_EXTENSION` is `Some` and never mutated.
    // `addr_of!` avoids the `static_mut_refs` lint.
    let ext = unsafe { std::ptr::addr_of!(RV_EXTENSION).as_ref().unwrap().as_ref().unwrap() };
    f(ext)
}

/// Build the arma-rs [`Extension`] with registered commands.
///
/// Command structure:
/// - `"sql"` — receives an SQF-encoded array as a single `Vec<String>` parameter.
///   The first element is the SQL input, remaining elements are bind params.
///
/// Public so integration tests can create a [`testing::Extension`](arma_rs::testing::Extension).
pub fn build_extension() -> Extension {
    Extension::build()
        .version(concat!("a3sql ", env!("CARGO_PKG_VERSION")).to_string())
        .command("sql", sql_handler)
        .finish()
}

/// Handler for the `sql` command.
///
/// The SQF wrapper encodes the call as an SQF array string:
/// ```text
/// _payload = format ['["%1"%2]', _stmt, _args]; // _args is prefixed with commas
/// "a3sql" callExtension ["sql", [_payload]];
/// ```
///
/// `payload` is deserialized by arma-rs's [`Vec<T>: FromArma`](arma_rs::FromArma):
/// - `payload[0]` — SQL input string
/// - `payload[1..]` — bind parameter strings (substituted for `$1`, `$2`, ...)
fn sql_handler(payload: Vec<String>) -> String {
    let mut db = DB.lock().unwrap_or_else(|e| e.into_inner());
    if payload.is_empty() {
        return dispatch::dispatch_inner(&mut db, "", &[]);
    }
    let input = &payload[0];
    let args: Vec<&str> = payload[1..].iter().map(|s| s.as_str()).collect();
    dispatch::dispatch_inner(&mut db, input, &args)
}

// ── C ABI ─────────────────────────────────────────────────────────────────────

/// Called by Arma engine on extension load. Returns the version string.
///
/// Also triggers extension initialisation (plugins, state) on first call.
///
/// # Safety
/// `output` must be a valid, writable buffer of at least `output_size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn RVExtensionVersion(output: *mut c_char, output_size: u32) {
    with_extension(|_| {}); // ensure extension + plugins are initialised
    if output.is_null() || output_size == 0 {
        return;
    }
    let version = version_bytes();
    // Bound to `output_size - 1` and always null-terminate: a caller with an
    // undersized buffer must still receive a valid C string, not a truncated
    // one without its terminator.
    let len = (output_size as usize - 1).min(version.len());
    // SAFETY: `output` is non-null, `output_size > 0`, and `len <= output_size - 1`
    // keeps the null terminator at `output.add(len)` inside the buffer.
    unsafe {
        std::ptr::copy_nonoverlapping(version.as_ptr(), output as *mut u8, len);
        *output.add(len) = 0;
    }
}

/// Write a response string into an Arma engine output buffer, bounded by
/// `output_size - 1` bytes and null-terminated. A no-op for empty responses
/// (leaves the buffer untouched, matching Arma's expectation of `""`).
///
/// Returns `true` when the full response did not fit (i.e. was truncated) —
/// the caller must then report the truncation instead of handing the mod a
/// partial result as if it were complete.
///
/// # Safety
/// `output` must be a valid, writable buffer of at least `output_size` bytes
/// (or null, in which case nothing is written).
unsafe fn write_output(output: *mut c_char, output_size: u32, resp: &str) -> bool {
    let bytes = resp.as_bytes();
    let mut len = bytes.len().min(output_size.saturating_sub(1) as usize);
    // Back off to the last complete UTF-8 codepoint boundary: a truncation
    // that lands mid-codepoint would leave invalid UTF-8 bytes in the buffer
    // (SQF's toArray/toString round-trip chokes on those). The response is
    // always valid UTF-8, so at most 3 trailing bytes are dropped.
    while len > 0 && !resp.is_char_boundary(len) {
        len -= 1;
    }
    if len > 0 && !output.is_null() {
        // SAFETY: `output` is non-null and `len <= output_size - 1` guarantees
        // the null terminator at `output.add(len)` is within the buffer.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), output as *mut u8, len);
            *output.add(len) = 0;
        }
    }
    len < bytes.len()
}

/// Write a dispatch response into the engine output buffer. When the full
/// response does not fit, the buffer is instead re-written with an `ERR_EXEC`
/// envelope (byte counts + cursor-paging hint) so a mod sees an explicit
/// error rather than silently truncated data. The envelope itself is bounded
/// by the same UTF-8 back-off: a pathological tiny `output_size` yields
/// valid UTF-8 in the buffer, never a panic.
unsafe fn write_response(output: *mut c_char, output_size: u32, resp: &str) {
    if unsafe { write_output(output, output_size, resp) } {
        let msg = format!(
            "Result exceeds output buffer ({} bytes > {} limit) — use 'cursor create <name> <query>' + 'cursor fetch <name> [limit]' to page large results",
            resp.len(),
            output_size
        );
        // SAFETY: same buffer contract as above; the write is back-off bounded.
        unsafe { write_output(output, output_size, &error_response(ErrorCode::Exec, &msg)) };
    }
}

/// STRING callExtension STRING — compatibility entry point.
///
/// The SQF wrapper uses the array form for bind params, but many call sites
/// issue plain `"a3sql" callExtension _sql` / `callExtension "version"`.
/// Route the string through the dispatcher so those calls work too.
///
/// # Safety
/// `output` must be a valid, writable buffer of at least `output_size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn RVExtension(output: *mut c_char, output_size: u32, function: *const c_char) {
    let input = if function.is_null() {
        Cow::Borrowed("")
    } else {
        // SAFETY: `function` is a null-terminated string from the Arma engine contract.
        unsafe { CStr::from_ptr(function) }.to_string_lossy()
    };
    let resp = dispatch::dispatch(&input, &[]);
    // SAFETY: `output`/`output_size` are the engine buffer contract.
    unsafe { write_response(output, output_size, &resp) };
}

/// STRING callExtension ARRAY — main entry point.
///
/// Two call conventions are supported:
/// - **arma-rs convention**: `"a3sql" callExtension ["sql", [_payload]]` where
///   `_payload` is an SQF-encoded array string (`["stmt", "arg1", ...]`).
///   Routed via [`arma_rs::Extension::handle_call`].
/// - **Vanilla Arma convention**: `"a3sql" callExtension [cmd, [arg1, ...]]`
///   where the first element is any command name (`save`, `load`, `prepare`,
///   `cursor create`, ...) or a SQL statement with bind params
///   (`[sql, [p1, p2]]`). Routed straight to [`dispatch::dispatch`], which
///   understands every command the SQF wrapper issues.
///
/// Returns arma-rs status codes:
/// - `0` = success
/// - `1` = command not found (arma-rs path only)
/// - `2N` = wrong argument count (N = received count)
/// - `9` = application error
///
/// Returns `-1` when any pointer contract is violated (null `output`,
/// `function`, or `argv`).
///
/// # Safety
/// All pointer arguments must be valid, non-null pointers from the Arma engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn RVExtensionArgs(
    output: *mut c_char,
    output_size: u32,
    function: *const c_char,
    argv: *const *const c_char,
    argc: u32,
) -> i32 {
    if output.is_null() || function.is_null() {
        return -1;
    }
    let function_str = unsafe { CStr::from_ptr(function) }.to_string_lossy();

    // arma-rs path: only the "sql" command (SQF-encoded payload array).
    if function_str == "sql" {
        if argv.is_null() {
            // arma-rs's command macro `unwrap()`s the argv pointer whenever
            // argc matches the handler's arity (1 for `sql`), so a null argv
            // would panic across the FFI boundary instead of returning a code.
            return -1;
        }
        return with_extension(|ext| {
            // SAFETY: All pointer arguments are guaranteed valid, non-null by the Arma engine contract.
            unsafe {
                ext.handle_call(
                    function as *mut c_char,
                    output,
                    output_size as usize,
                    Some(argv as *mut *mut i8),
                    Some(argc as i32),
                    true,
                )
            }
        });
    }

    // Vanilla Arma convention: function is the command name or SQL statement,
    // argv carries the args (bind params for SQL, operands for commands).
    let mut args: Vec<&str> = Vec::with_capacity(argc as usize);
    if !argv.is_null() {
        for i in 0..argc as usize {
            let p = unsafe { *argv.add(i) };
            if !p.is_null() {
                // SAFETY: `p` is a null-terminated string from the Arma engine contract.
                if let Ok(s) = unsafe { CStr::from_ptr(p) }.to_str() {
                    args.push(s);
                }
            }
        }
    }
    let resp = dispatch::dispatch(&function_str, &args);
    // SAFETY: `output`/`output_size` are the engine buffer contract.
    unsafe { write_response(output, output_size, &resp) };
    0
}

// ── Callback registration ──────────────────────────────────────────────────

/// Register a callback function that the extension can call back into SQF.
/// Arma calls this automatically when the extension exports the symbol.
///
/// Stored in the [`CALLBACK`] static for access by [`eval`](crate::engine::functions::eval).
///
/// # Safety
/// `callbackProc` must be a valid function pointer provided by the Arma engine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn RVExtensionRegisterCallback(callbackProc: Option<Callback>) {
    let mut cb = CALLBACK.lock().unwrap();
    *cb = callbackProc;
}

// ── Tests ──────────────────────────────────────────────────────────────────
// Unit tests for the FFI surface live in `tests.rs` (sibling module): they
// need private access to `write_output` and `CALLBACK`. They run under the
// normal test harness; the miri scope deliberately excludes the FFI surface.
#[cfg(test)]
mod tests;
