#include "../../include/a3sql_plugin.h"
#include <string.h>

const char* a3sql_plugin_init(void) {
    a3sql_plugin_register_function("echo_example", "echo", 1, 1);
    return "echo_example";
}
