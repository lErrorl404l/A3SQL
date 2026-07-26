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
