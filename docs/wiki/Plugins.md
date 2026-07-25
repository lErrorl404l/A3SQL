# Plugins

A3DB supports three ways to extend its functionality, from simple SQF functions to full native C/Rust plugins.

## 1. SQF Functions (Lightest)

Register an SQF function callable from SQL as `fn_<name>()`:

```sqf
// Register
["register_function", ["my_func", 2]] call a3sql_fnc_execute;

// Use in SQL
_result = ["SELECT fn_my_func('a', 'b') FROM t"] call a3sql_fnc_execute;
```

SQF functions are tracked by name only — the actual evaluation is handled by your SQF code before calling `fn_execute.sqf`.

## 2. Rust Trait Plugins

For built-in extensions compiled into the DLL:

```rust
use a3sql::engine::plugin::{PluginFunction, register_plugin};

register_plugin(
    "my_plugin",
    vec![PluginFunction {
        name: "hello".into(),
        min_args: 1,
        max_args: 1,
        func: |args| {
            let name = args[0].to_string();
            Ok(DbValue::String(format!("Hello, {}!", name)))
        },
    }],
    vec![],
);
```

Registered at startup via `init_builtin_plugins()` in `engine/plugin.rs`.
Callable from SQL: `SELECT fn_hello('World') FROM t` → `"Hello, World!"`

## 3. C ABI Dynamic Plugins (Full Power)

Shared libraries (`.so` / `.dll`) placed in the plugin directory are loaded at runtime.

### Writing a plugin

```c
// my_plugin.c
#include "a3sql_plugin.h"

A3DB_PLUGIN_INIT {
    a3sql_plugin_register_function("my_plugin", "echo", 1, 1);
    return "my_plugin";
}
```

### Building

```bash
gcc -shared -o my_plugin.so my_plugin.c -fPIC
```

### Loading

Drop the compiled `.so`/`.dll` into a directory and load from SQF:
```sqf
["plugin_dir @a3sql/plugins"] call a3sql_fnc_execute;
```

Or load via TCP:
```
plugin_dir /path/to/plugins
```

## Listing Plugins

```sqf
_result = ["plugins"] call a3sql_fnc_execute;
// → [0, "OK", [["builtin_echo", ["echo"], []], ["sqf_user", ["my_func"], ["sqf_called"]]]]
//         plugin_name    functions    hooks
```

## C API Reference

See `include/a3sql_plugin.h` in the repository for the full header.

| Function | Purpose |
|----------|---------|
| `a3sql_plugin_init()` | **Required.** Entry point, called at load. Returns plugin name. |
| `a3sql_plugin_register_function(name, fn_name, min_args, max_args)` | Register a SQL function (callable as `fn_<name>`) |

## Example

A complete example plugin is at `plugins/example/plugin.c` in the repository.

## Security Note

C ABI plugins have full access to the game process. Only load plugins from trusted sources.
