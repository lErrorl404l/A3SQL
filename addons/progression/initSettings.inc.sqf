#include "script_component.hpp"

["a3sql_progression_enabled", "CHECKBOX",
    ["STR_A3SQL_Progression_Enabled_DisplayName", "STR_A3SQL_Progression_Enabled_Description"],
    "STR_A3SQL_Progression_Category", true, false
] call CBA_fnc_addSetting;

["a3sql_progression_log_verbose", "CHECKBOX",
    ["STR_A3SQL_Progression_VerboseLogging_DisplayName", "STR_A3SQL_Progression_VerboseLogging_Description"],
    "STR_A3SQL_Progression_Category", false, false
] call CBA_fnc_addSetting;
