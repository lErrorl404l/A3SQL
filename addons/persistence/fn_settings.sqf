#include "script_component.hpp"

["a3sql_persistence_enabled", "CHECKBOX",
    [["STR_a3sql_persistence_setting_enabled", "STR_a3sql_persistence_setting_enabled_desc"]],
    "A3SQL Persistence", true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_restore_on_jip", "CHECKBOX",
    [["STR_a3sql_persistence_setting_restore_on_jip", "STR_a3sql_persistence_setting_restore_on_jip_desc"]],
    "A3SQL Persistence", true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_debug", "CHECKBOX",
    [["STR_a3sql_persistence_setting_debug", "STR_a3sql_persistence_setting_debug_desc"]],
    "A3SQL Persistence", false, false
] call CBA_fnc_addSetting;
