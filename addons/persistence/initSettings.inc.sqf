#include "script_component.hpp"

["a3sql_persistence_enabled", "CHECKBOX",
    ["STR_A3SQL_Persistence_Enabled_DisplayName", "STR_A3SQL_Persistence_Enabled_Description"],
    "STR_A3SQL_Persistence_Category", true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_restore_on_jip", "CHECKBOX",
    ["STR_A3SQL_Persistence_RestoreOnJIP_DisplayName", "STR_A3SQL_Persistence_RestoreOnJIP_Description"],
    "STR_A3SQL_Persistence_Category", true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_debug", "CHECKBOX",
    ["STR_A3SQL_Persistence_Debug_DisplayName", "STR_A3SQL_Persistence_Debug_Description"],
    "STR_A3SQL_Persistence_Category", false, false
] call CBA_fnc_addSetting;
