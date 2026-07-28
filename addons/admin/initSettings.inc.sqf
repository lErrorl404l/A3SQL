#include "script_component.hpp"

["a3sql_admin_enabled", "CHECKBOX",
    [LSTRING(Enabled_DisplayName), LSTRING(Enabled_Description)],
    LSTRING(Category), true, false
] call CBA_fnc_addSetting;

["a3sql_admin_log_level", "LIST",
    [LSTRING(LogLevel_DisplayName), LSTRING(LogLevel_Description)],
    LSTRING(Category),
    [[0, 1, 2], ["ERROR", "INFO", "DEBUG"], 1],
    false
] call CBA_fnc_addSetting;
