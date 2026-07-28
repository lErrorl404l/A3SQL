#include "script_component.hpp"

["a3sql_admin_enabled", "CHECKBOX",
    [["STR_a3sql_admin_setting_enabled", "STR_a3sql_admin_setting_enabled_desc"]],
    "A3SQL Admin", true, false
] call CBA_fnc_addSetting;

["a3sql_admin_log_level", "LIST",
    [["STR_a3sql_admin_setting_log_level", "STR_a3sql_admin_setting_log_level_desc"]],
    "A3SQL Admin",
    [[0, 1, 2], ["ERROR", "INFO", "DEBUG"], 1],
    false
] call CBA_fnc_addSetting;
