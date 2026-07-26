// a3sql — C ABI entry points
//
// Build targets:
//   Linux:   x86_64-unknown-linux-gnu, i686-unknown-linux-gnu
//   Windows: x86_64-pc-windows-gnu,     i686-pc-windows-gnu
// Windows x86 (32-bit) needs a .def file or link args for decorated exports:
//   _RVExtensionVersion@8, _RVExtension@12, _RVExtensionArgs@20

//! C ABI entry points — RVExtension, RVExtensionArgs, RVExtensionVersion.
//! These are the interface between the Arma 3 engine and a3sql.

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{LazyLock, Mutex};

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

// ── ABI ─────────────────────────────────────────────────────────────────────

/// Output buffer size from Arma engine. Currently 10240 bytes.
pub(crate) const OUTPUT_BUF_SIZE: u32 = 10240;

/// Version string — max 32 bytes including null terminator.
const VERSION: &[u8] = b"a3sql 0.1.0\0";

/// Called by engine on extension load.
///
/// # Safety
/// `output` must be a valid, writable buffer of at least `output_size` bytes.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionVersion(output: *mut c_char, output_size: u32) {
    let len = (output_size as usize).min(VERSION.len());
    std::ptr::copy_nonoverlapping(VERSION.as_ptr(), output as *mut u8, len);
    // Init built-in plugins on first load
    engine::plugin::init_builtin_plugins();
}

/// STRING callExtension STRING — compatibility entry point.
///
/// # Safety
/// `output` and `function` must be valid, non-null pointers to C string buffers.
#[no_mangle]
pub unsafe extern "C" fn RVExtension(output: *mut c_char, output_size: u32, function: *const c_char) {
    if output.is_null() || function.is_null() {
        return;
    }

    let input = match CStr::from_ptr(function).to_str() {
        Ok(s) => s,
        Err(_) => {
            write_output(output, output_size, "[-1,\"ERROR\",\"INVALID_UTF8\"]");
            return;
        }
    };

    let result = dispatch::dispatch(input, &[]);
    write_output(output, output_size, &result);
}

/// STRING callExtension ARRAY — main entry point.
/// Returns 0 on success, -1 on error (extension return code).
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

    let input = match CStr::from_ptr(function).to_str() {
        Ok(s) => s,
        Err(_) => {
            write_output(output, output_size, "[-1,\"ERROR\",\"INVALID_UTF8\"]");
            return -1;
        }
    };

    let mut args: Vec<&str> = Vec::new();
    if !argv.is_null() {
        for i in 0..argc as isize {
            let ptr = *argv.offset(i);
            if !ptr.is_null() {
                if let Ok(s) = CStr::from_ptr(ptr).to_str() {
                    args.push(s);
                }
            }
        }
    }

    let result = dispatch::dispatch(input, &args);
    write_output(output, output_size, &result);
    0
}

// ── Callback registration ──────────────────────────────────────────────────

/// Register a callback function that the extension can call back into SQF.
/// Arma calls this automatically when the extension exports the symbol.
///
/// # Safety
/// `callbackProc` must be a valid function pointer provided by the Arma engine.
#[no_mangle]
pub unsafe extern "C" fn RVExtensionRegisterCallback(callbackProc: Option<unsafe extern "C" fn(i32, *mut c_char)>) {
    let mut cb = CALLBACK.lock().unwrap();
    *cb = callbackProc;
}

// ── Output helper ─────────────────────────────────────────────────────────

fn write_output(output: *mut c_char, output_size: u32, s: &str) {
    let bytes = s.as_bytes();
    let len = (output_size as usize - 1).min(bytes.len());
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), output as *mut u8, len);
        *output.add(len) = 0;
    }
}
