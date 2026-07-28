#include "script_component.hpp"

["a3sql_persistence_enabled", "CHECKBOX",
    ["Enable Persistence", "Enable player state persistence on disconnect/join."],
    "A3SQL Persistence", true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_restore_on_jip", "CHECKBOX",
    ["Restore on JIP", "Restore player state when they JIP back into the mission."],
    "A3SQL Persistence", true, false
] call CBA_fnc_addSetting;

["a3sql_persistence_debug", "CHECKBOX",
    ["Debug", "Enable verbose logging and systemChat for restore operations."],
    "A3SQL Persistence", false, false
] call CBA_fnc_addSetting;
