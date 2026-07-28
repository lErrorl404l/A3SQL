#include "script_component.hpp"

["a3sql_progression_enabled", "CHECKBOX",
    [["STR_a3sql_progression_setting_enabled", "STR_a3sql_progression_setting_enabled_desc"]],
    "A3SQL Progression", true, false
] call CBA_fnc_addSetting;

["a3sql_progression_log_verbose", "CHECKBOX",
    [["STR_a3sql_progression_setting_log_verbose", "STR_a3sql_progression_setting_log_verbose_desc"]],
    "A3SQL Progression", false, false
] call CBA_fnc_addSetting;
