#include "script_component.hpp"

["a3sql_patch_enabled", "CHECKBOX",
    [LSTRING(Enabled_DisplayName), LSTRING(Enabled_Description)],
    LSTRING(Category), true, false
] call CBA_fnc_addSetting;

["a3sql_patch_log_level", "LIST",
    [LSTRING(LogLevel_DisplayName), LSTRING(LogLevel_Description)],
    LSTRING(Category),
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_check_interval_hz", "SLIDER",
    [LSTRING(CheckIntervalHz_DisplayName), LSTRING(CheckIntervalHz_Description)],
    LSTRING(Category),
    [0, 20, 5, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_allow_sqf_exec", "CHECKBOX",
    [LSTRING(AllowSqfExec_DisplayName), LSTRING(AllowSqfExec_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;

["a3sql_patch_stream_output", "CHECKBOX",
    [LSTRING(StreamOutput_DisplayName), LSTRING(StreamOutput_Description)],
    LSTRING(Category), false, false
] call CBA_fnc_addSetting;
