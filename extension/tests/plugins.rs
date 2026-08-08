// Test C ABI plugin loading via plugin_dir
use a3sql::dispatch;
use std::sync::{Mutex, MutexGuard};

static TEST_MUTEX: Mutex<()> = Mutex::new(());
fn setup() -> MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap();
    dispatch("reset", &[]);
    g
}

#[test]
fn gap_c_abi_plugin() {
    let _g = setup();
    // Copy plugin to a temp dir and load it
    let dir = "/tmp/a3sql_plugins_test";
    std::fs::create_dir_all(dir).ok();
    let plugin_src = "/tmp/test_plugin3.so";
    let plugin_dst = format!("{}/test_plugin3.so", dir);
    std::fs::copy(plugin_src, &plugin_dst).ok();
    // Load plugins from directory
    let r = dispatch(&format!("plugin_dir {}", dir), &[]);
    assert!(r.contains("[0,"), "plugin_dir should succeed: {}", r);
    // Verify the registered function is callable
    std::fs::remove_dir_all(dir).ok();
}

// ── F-07: plugin_dir must reject non-regular files before dlopen ───────────
//
// A writable plugin directory is a code-execution vector: a symlink named
// `x.so` pointing at an arbitrary library elsewhere on disk would be dlopened
// as a plugin. The gate rejects symlinks and special files (regular files
// only) before dlopen.

/// Minimal valid plugin fixture: single-file cdylib exporting
/// `a3sql_plugin_init`, compiled by the rustc already on PATH (the repo is a
/// cargo project). The self-declared plugin name is `gate_test_plugin`.
const PLUGIN_SRC: &str = r#"
use std::ffi::c_void;
#[unsafe(no_mangle)]
pub extern "C" fn a3sql_plugin_init(_ctx: *mut c_void) -> *const std::ffi::c_char {
    b"gate_test_plugin\0".as_ptr() as *const std::ffi::c_char
}
"#;

#[test]
#[cfg(unix)]
fn plugin_dir_rejects_symlinks_before_dlopen() {
    use std::os::unix::fs::symlink;
    let _g = setup();

    let dir = std::env::temp_dir().join("a3sql_plugin_gate");
    std::fs::create_dir_all(&dir).ok();
    let src = dir.join("plug.rs");
    std::fs::write(&src, PLUGIN_SRC).unwrap();
    let real = dir.join("real.so");
    let rustc = std::process::Command::new("rustc")
        .args(["--crate-type", "cdylib", "--edition", "2024", "-O"])
        .arg(&src)
        .arg("-o")
        .arg(&real)
        .output()
        .expect("rustc must be on PATH");
    assert!(
        rustc.status.success(),
        "fixture build failed: {}",
        String::from_utf8_lossy(&rustc.stderr)
    );

    // Poisoning vector: a symlink named like a plugin pointing at the real
    // one. Pre-fix both dlopen (two `gate_test_plugin` entries); post-fix
    // only the regular file loads (one entry).
    let evil = dir.join("evil.so");
    let _ = std::fs::remove_file(&evil);
    symlink(&real, &evil).unwrap();

    let r = dispatch(&format!("plugin_dir {}", dir.display()), &[]);
    let loads = r.matches("gate_test_plugin").count();
    assert_eq!(loads, 1, "regular file must load and the symlink must be rejected: {r}");

    std::fs::remove_dir_all(&dir).ok();
}
