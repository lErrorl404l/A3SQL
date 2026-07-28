// a3sql — C ABI entry points
//
// Build targets:
//   Linux:   x86_64-unknown-linux-gnu, i686-unknown-linux-gnu
//   Windows: x86_64-pc-windows-gnu,     i686-pc-windows-gnu
// Windows x86 (32-bit) needs a .def file or link args for decorated exports:
//   _RVExtensionVersion@8, _RVExtension@12, _RVExtensionArgs@20

//! C ABI entry points — RVExtension, RVExtensionArgs, RVExtensionVersion.
//! These are the interface between the Arma 3 engine and a3sql.
//!
//! Command routing and testing infrastructure provided by [`arma_rs`].

use std::os::raw::c_char;
use std::sync::{LazyLock, Mutex, Once};

use arma_rs::Extension;

use crate::dispatch;
use crate::engine;

/// Global database instance (single-threaded, mutex-protected).
pub(crate) static DB: LazyLock<Mutex<engine::Database>> = LazyLock::new(|| Mutex::new(engine::Database::new()));

/// Optional pointer to the SQF callback function registered by the engine.
pub(crate) static CALLBACK: LazyLock<Mutex<Option<unsafe extern "C" fn(i32, *mut std::os::raw::c_char)>>> =
    LazyLock::new(|| Mutex::new(None));
// ponytail: external TCP listener — global lock on a single listener
pub(crate) static LISTENER: LazyLock<Mutex<Option<std::net::TcpListener>>> = LazyLock::new(|| Mutex::new(None));

/// Stored credentials for TCP authentication. Empty = anonymous access.
pub(crate) static CREDENTIALS: LazyLock<Mutex<(String, String)>> =
    LazyLock::new(|| Mutex::new((String::new(), String::new())));
pub(crate) static REMOTE: LazyLock<Mutex<Option<std::net::TcpStream>>> = LazyLock::new(|| Mutex::new(None));

/// Output buffer size from Arma engine. Currently 10240 bytes.
pub(crate) const OUTPUT_BUF_SIZE: u32 = 10240;

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
    if payload.is_empty() {
        return dispatch::dispatch("", &[]);
    }
    let input = &payload[0];
    let args: Vec<&str> = payload[1..].iter().map(|s| s.as_str()).collect();
    dispatch::dispatch(input, &args)
}

// ── C ABI ─────────────────────────────────────────────────────────────────────

/// Called by Arma engine on extension load. Returns the version string.
///
/// Also triggers extension initialisation (plugins, state) on first call.
///
/// # Safety
/// `output` must be a valid, writable buffer of at least `output_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionVersion(output: *mut c_char, output_size: u32) {
    with_extension(|_| {}); // ensure extension + plugins are initialised
    let version = version_bytes();
    let len = (output_size as usize).min(version.len());
    std::ptr::copy_nonoverlapping(version.as_ptr(), output as *mut u8, len);
}

/// STRING callExtension STRING — compatibility entry point.
///
/// Not supported with arma-rs command routing. The SQF wrapper should always
/// use the array form: `callExtension ["sql", [payload]]`.
///
/// # Safety
/// `output` must be a valid, writable buffer.
#[no_mangle]
pub unsafe extern "C" fn RVExtension(output: *mut c_char, output_size: u32, _function: *const c_char) {
    if !output.is_null() && output_size > 0 {
        *output = 0;
    }
}

/// STRING callExtension ARRAY — main entry point.
///
/// Delegates to [`arma_rs::Extension::handle_call`] for command routing. The
/// command name is the first array element (`function`); remaining elements are
/// passed as args to the command handler.
///
/// Returns arma-rs status codes:
/// - `0` = success
/// - `1` = command not found
/// - `2N` = wrong argument count (N = received count)
/// - `9` = application error
///
/// # Safety
/// All pointer arguments must be valid, non-null pointers from the Arma engine.
#[no_mangle]
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
    with_extension(|ext| {
        ext.handle_call(
            function as *mut c_char,
            output,
            output_size as usize,
            Some(argv as *mut *mut i8),
            Some(argc as i32),
            true,
        )
    })
}

// ── Callback registration ──────────────────────────────────────────────────

/// Register a callback function that the extension can call back into SQF.
/// Arma calls this automatically when the extension exports the symbol.
///
/// Stored in the [`CALLBACK`] static for access by [`eval`](crate::engine::functions::eval).
///
/// # Safety
/// `callbackProc` must be a valid function pointer provided by the Arma engine.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionRegisterCallback(callbackProc: Option<unsafe extern "C" fn(i32, *mut c_char)>) {
    let mut cb = CALLBACK.lock().unwrap();
    *cb = callbackProc;
}
