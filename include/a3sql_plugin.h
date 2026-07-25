// a3sql Plugin API — C ABI for external plugins
//
// Write a shared library (.so / .dll) that exports a3sql_plugin_init.
//
// Build (example):
//   gcc -shared -o my_plugin.so my_plugin.c -fPIC
//
// Drop into @a3sql/plugins/ — loaded at startup.
//
// For full docs: https://github.com/lErrorl404l/a3sql/wiki/Plugins

#ifndef A3SQL_PLUGIN_H
#define A3SQL_PLUGIN_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// ── Plugin entry point ─────────────────────────────────────────────────

// Every plugin MUST export this function.
// Returns the plugin name (static string, not freed).
// Called once at startup.
typedef const char* (*a3sql_plugin_init_t)(void);
#define A3SQL_PLUGIN_INIT __attribute__((visibility("default"))) const char* a3sql_plugin_init

// ── Registration callbacks (call from init) ────────────────────────────

// Register a SQL function callable as fn_<name>(args).
// min_args/max_args: argument count constraints (-1 = unlimited).
// The function name is prefixed with fn_ automatically.
int32_t a3sql_plugin_register_function(
    const char* plugin_name,
    const char* function_name,
    int32_t min_args,
    int32_t max_args
);

// ── Example plugin template ─────────────────────────────────────────────
//
// #include "a3sql_plugin.h"
//
// static int my_echo(const char* arg) {
//     // ...
//     return 0;
// }
//
// A3SQL_PLUGIN_INIT {
//     a3sql_plugin_register_function("my_plugin", "echo", 1, 1);
//     return "my_plugin";
// }

#ifdef __cplusplus
}
#endif

#endif // A3SQL_PLUGIN_H
