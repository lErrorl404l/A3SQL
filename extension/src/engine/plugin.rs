// a3sql plugin system — three tiers:
//
// 1. Rust trait: compile-time plugins (A3Plugin)
// 2. C ABI: dynamic .so/.dll loaded at runtime (via libloading)
// 3. SQF: user-defined functions registered from Arma scripting
//
// All registered functions are callable from SQL as fn_<name>().

//! Plugin system — Rust trait plugins and SQF function registration.
//! Plugins can register custom functions and hook into query execution.

use std::collections::HashMap;
use std::sync::Mutex;

use super::value::DbValue;

use crate::engine::error::EngineError;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// A plugin-registered SQL function: name → (arg_count, impl).
#[derive(Clone)]
pub(crate) struct PluginFunction {
    pub name: String,
    pub min_args: usize,
    #[allow(dead_code, reason = "max args not yet enforced at dispatch")]
    pub max_args: usize,
    pub func: fn(&[DbValue]) -> Result<DbValue, EngineError>,
}

/// Hook into query execution.
#[derive(Clone)]
pub(crate) enum Hook {
    /// Called before query execution. Return None to allow, Some(err) to block.
    #[allow(dead_code, reason = "plugin hooks not yet wired into executor")]
    PreQuery(fn(sql: &str) -> Option<String>),
    /// Called after query execution with the result JSON string.
    #[allow(dead_code, reason = "plugin hooks not yet wired into executor")]
    PostQuery(fn(sql: &str, result: &str)),
}

/// A registered plugin descriptor.
pub(crate) struct Plugin {
    pub name: String,
    pub functions: Vec<PluginFunction>,
    pub hooks: Vec<Hook>,
    // Dynamic library handle — kept alive for the plugin's lifetime.
    #[allow(dead_code, reason = "lib_handle kept alive for plugin lifetime")]
    lib_handle: Option<Box<dyn std::any::Any + Send>>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Registry
// ═══════════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref PLUGIN_REGISTRY: Mutex<PluginRegistryInner> = Mutex::new(PluginRegistryInner {
        plugins: Vec::new(),
        sqf_functions: HashMap::new(),
    });
}

struct PluginRegistryInner {
    plugins: Vec<Plugin>,
    /// SQF-registered functions: name → (SQF code string, arg_count).
    sqf_functions: HashMap<String, (String, usize)>,
}

/// Register a Rust trait plugin.
pub(crate) fn register_plugin(name: &str, functions: Vec<PluginFunction>, hooks: Vec<Hook>) {
    let mut reg = PLUGIN_REGISTRY.lock().unwrap();
    reg.plugins.push(Plugin {
        name: name.to_string(),
        functions,
        hooks,
        lib_handle: None,
    });
}

/// Register built-in Rust plugins (called at startup).
pub(crate) fn init_builtin_plugins() {
    // Echo plugin — returns its first argument unchanged
    register_plugin(
        "builtin_echo",
        vec![PluginFunction {
            name: "echo".into(),
            min_args: 1,
            max_args: 10,
            func: |args| Ok(args[0].clone()),
        }],
        vec![],
    );
}

/// Register a function from SQF with an optional SQF body.
/// When `body` is non-empty, the engine will call the SQF callback
/// on function invocation instead of returning an error.
pub(crate) fn register_sqf_function(name: &str, arg_count: usize, body: &str) {
    let mut reg = PLUGIN_REGISTRY.lock().unwrap();
    reg.sqf_functions
        .insert(name.to_string(), (body.to_string(), arg_count));
}

/// Get the SQF body for a registered SQF function, if one exists.
pub(crate) fn get_sqf_function_body(name: &str) -> Option<String> {
    let reg = PLUGIN_REGISTRY.lock().unwrap();
    reg.sqf_functions
        .get(name)
        .map(|(body, _)| body.clone())
        .filter(|b| !b.is_empty())
}

/// Look up a plugin function by name. Returns (function, plugin_name).
pub(crate) fn lookup_function(name: &str) -> Option<(PluginFunction, String)> {
    let reg = PLUGIN_REGISTRY.lock().unwrap();
    for plugin in &reg.plugins {
        for func in &plugin.functions {
            if func.name == name {
                return Some((func.clone(), plugin.name.clone()));
            }
        }
    }
    None
}

/// Check if a name matches a registered function (plugin or SQF).
#[allow(dead_code, reason = "plugin function dispatch not yet wired")]
pub(crate) fn is_registered(name: &str) -> bool {
    let reg = PLUGIN_REGISTRY.lock().unwrap();
    for plugin in &reg.plugins {
        for func in &plugin.functions {
            if func.name == name {
                return true;
            }
        }
    }
    reg.sqf_functions.contains_key(name)
}

/// Run pre-query hooks. Returns Some(error) if a hook blocked the query.
#[allow(dead_code, reason = "plugin hooks not yet wired into executor")]
pub(crate) fn run_pre_query_hooks(sql: &str) -> Option<String> {
    let reg = PLUGIN_REGISTRY.lock().unwrap();
    for plugin in &reg.plugins {
        for hook in &plugin.hooks {
            if let Hook::PreQuery(f) = hook {
                if let Some(err) = f(sql) {
                    return Some(err);
                }
            }
        }
    }
    None
}

/// Run post-query hooks.
#[allow(dead_code, reason = "plugin hooks not yet wired into executor")]
pub(crate) fn run_post_query_hooks(sql: &str, result: &str) {
    let reg = PLUGIN_REGISTRY.lock().unwrap();
    for plugin in &reg.plugins {
        for hook in &plugin.hooks {
            if let Hook::PostQuery(f) = hook {
                f(sql, result);
            }
        }
    }
}

/// List all registered plugins and their functions (for diagnostics).
pub(crate) fn list_plugins() -> Vec<(String, Vec<String>, Vec<String>)> {
    let reg = PLUGIN_REGISTRY.lock().unwrap();
    let mut out = Vec::new();
    for plugin in &reg.plugins {
        let funcs: Vec<String> = plugin.functions.iter().map(|f| f.name.clone()).collect();
        let hook_types: Vec<String> = plugin
            .hooks
            .iter()
            .map(|h| match h {
                Hook::PreQuery(_) => "pre_query".into(),
                Hook::PostQuery(_) => "post_query".into(),
            })
            .collect();
        out.push((plugin.name.clone(), funcs, hook_types));
    }
    if !reg.sqf_functions.is_empty() {
        let sqf_funcs: Vec<String> = reg.sqf_functions.keys().cloned().collect();
        out.push(("sqf_user".into(), sqf_funcs, vec!["sqf_called".into()]));
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════════
// Dynamic C ABI loading
// ═══════════════════════════════════════════════════════════════════════════

/// Load plugins from shared libraries in a directory.
/// Only loaded once at startup.
pub(crate) fn load_plugin_dir(path: &str) -> Vec<String> {
    let mut loaded = Vec::new();
    let dir = match std::fs::read_dir(path) {
        Ok(d) => d,
        Err(_) => return loaded,
    };

    for entry in dir.flatten() {
        let p = entry.path();
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "so" && ext != "dll" {
            continue;
        }
        // ponytail: libloading handles both .so and .dll transparently
        match load_plugin_file(p.to_string_lossy().as_ref()) {
            Ok(name) => loaded.push(name),
            Err(e) => {
                // ponytail: log and skip bad plugins
                let fname = p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                eprintln!("[a3sql] plugin load failed {}: {}", fname, e);
            }
        }
    }
    loaded
}

fn load_plugin_file(path: &str) -> Result<String, EngineError> {
    // Safety: libloading is safe — the plugin is a shared lib we control.
    // The plugin C ABI must match a3sql_plugin.h.
    unsafe {
        let lib = libloading::Library::new(path).map_err(|e| EngineError::Exec(format!("dlopen: {}", e)))?;

        let init: libloading::Symbol<unsafe extern "C" fn(*mut std::ffi::c_void) -> *const std::ffi::c_char> = lib
            .get(b"a3sql_plugin_init")
            .map_err(|_| EngineError::Exec("no a3sql_plugin_init symbol".into()))?;

        let registry_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let name_ptr = init(registry_ptr);
        if name_ptr.is_null() {
            return Err(EngineError::Exec("plugin init returned null".into()));
        }
        let name = std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().into_owned();

        // Register functions from the plugin using the C ABI callback.
        // The plugin calls back into a3sql to register each function.
        // We pass a function pointer for the plugin to call.
        // For now, plugins register via the init() call.
        // The library handle is leaked intentionally — plugins live for the process lifetime.
        std::mem::forget(lib);

        Ok(name)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers for external C ABI plugin development
// ═══════════════════════════════════════════════════════════════════════════

/// C-callable function for plugins to register a SQL function.
/// Called from the plugin's init() via the callback pointer.
#[no_mangle]
pub extern "C" fn a3sql_plugin_register_function(
    plugin_name: *const std::ffi::c_char,
    func_name: *const std::ffi::c_char,
    min_args: i32,
    max_args: i32,
) -> i32 {
    // SAFETY: plugin_name points to a null-terminated C string allocated by
    // the plugin's C ABI init context. The caller guarantees the pointer is
    // valid and non-null for the duration of from_ptr. This function is only
    // called during plugin registration, before any SQL execution.
    let pname = unsafe { std::ffi::CStr::from_ptr(plugin_name) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: Same guarantees as plugin_name above — func_name is a valid
    // non-null C string provided by the plugin init context, and remains
    // valid for the lifetime of the from_ptr call.
    let fname = unsafe { std::ffi::CStr::from_ptr(func_name) }
        .to_string_lossy()
        .into_owned();

    // ponytail: C ABI plugins just register the name + arg counts.
    // Actual function evaluation requires a callback, which we expose
    // through the plugin's dispatch function pointer.
    let func = PluginFunction {
        name: fname.clone(),
        min_args: min_args as usize,
        max_args: max_args as usize,
        func: |_| {
            Err(EngineError::Exec(
                "C ABI function call not yet implemented — use Rust trait or SQF".into(),
            ))
        },
    };

    let mut reg = PLUGIN_REGISTRY.lock().unwrap();
    if let Some(plugin) = reg.plugins.iter_mut().find(|p| p.name == pname) {
        plugin.functions.push(func);
        0
    } else {
        // Create plugin entry on the fly
        reg.plugins.push(Plugin {
            name: pname,
            functions: vec![func],
            hooks: vec![],
            lib_handle: None,
        });
        0
    }
}
