#include "script_component.hpp"

["a3sql_admin_enabled", "CHECKBOX",
    ["STR_A3SQL_Admin_Enabled_DisplayName", "STR_A3SQL_Admin_Enabled_Description"],
    "STR_A3SQL_Admin_Category", true, false
] call CBA_fnc_addSetting;

["a3sql_admin_log_level", "LIST",
    ["STR_A3SQL_Admin_LogLevel_DisplayName", "STR_A3SQL_Admin_LogLevel_Description"],
    "STR_A3SQL_Admin_Category",
    [[0, 1, 2], ["ERROR", "INFO", "DEBUG"], 1],
    false
] call CBA_fnc_addSetting;
