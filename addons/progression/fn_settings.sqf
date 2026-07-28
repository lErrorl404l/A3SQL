#include "script_component.hpp"

["a3sql_progression_enabled", "CHECKBOX",
    ["Enable Progression", "Enable persistent rank/score tracking across sessions."],
    "A3SQL Progression", true, false
] call CBA_fnc_addSetting;

["a3sql_progression_log_verbose", "CHECKBOX",
    ["Verbose Logging", "Enable verbose logging for progression operations."],
    "A3SQL Progression", false, false
] call CBA_fnc_addSetting;
