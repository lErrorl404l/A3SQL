#include "script_component.hpp"

["a3sql_patch_enabled", "CHECKBOX",
    ["STR_A3SQL_Patch_Enabled_DisplayName", "STR_A3SQL_Patch_Enabled_Description"],
    "STR_A3SQL_Patch_Category", true, false
] call CBA_fnc_addSetting;

["a3sql_patch_log_level", "LIST",
    ["STR_A3SQL_Patch_LogLevel_DisplayName", "STR_A3SQL_Patch_LogLevel_Description"],
    "STR_A3SQL_Patch_Category",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_check_interval_hz", "SLIDER",
    ["STR_A3SQL_Patch_CheckIntervalHz_DisplayName", "STR_A3SQL_Patch_CheckIntervalHz_Description"],
    "STR_A3SQL_Patch_Category",
    [0, 20, 5, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_allow_sqf_exec", "CHECKBOX",
    ["STR_A3SQL_Patch_AllowSqfExec_DisplayName", "STR_A3SQL_Patch_AllowSqfExec_Description"],
    "STR_A3SQL_Patch_Category", false, false
] call CBA_fnc_addSetting;

["a3sql_patch_stream_output", "CHECKBOX",
    ["STR_A3SQL_Patch_StreamOutput_DisplayName", "STR_A3SQL_Patch_StreamOutput_Description"],
    "STR_A3SQL_Patch_Category", false, false
] call CBA_fnc_addSetting;
