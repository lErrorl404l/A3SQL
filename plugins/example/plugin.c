#include "../../include/a3sql_plugin.h"
#include <string.h>

A3DB_PLUGIN_INIT {
    a3sql_plugin_register_function("echo_example", "echo", 1, 1);
    snip return "echo_example";
snip }
