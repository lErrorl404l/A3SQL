#include "script_component.hpp"

["a3sql_patch_enabled", "CHECKBOX",
    [["STR_a3sql_patch_setting_enabled", "STR_a3sql_patch_setting_enabled_desc"]],
    "A3SQL Patch", true, false
] call CBA_fnc_addSetting;

["a3sql_patch_log_level", "LIST",
    [["STR_a3sql_patch_setting_log_level", "STR_a3sql_patch_setting_log_level_desc"]],
    "A3SQL Patch",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_check_interval_hz", "SLIDER",
    [["STR_a3sql_patch_setting_check_interval_hz", "STR_a3sql_patch_setting_check_interval_hz_desc"]],
    "A3SQL Patch",
    [0, 20, 5, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_allow_sqf_exec", "CHECKBOX",
    [["STR_a3sql_patch_setting_allow_sqf_exec", "STR_a3sql_patch_setting_allow_sqf_exec_desc"]],
    "A3SQL Patch", false, false
] call CBA_fnc_addSetting;

["a3sql_patch_stream_output", "CHECKBOX",
    [["STR_a3sql_patch_setting_stream_output", "STR_a3sql_patch_setting_stream_output_desc"]],
    "A3SQL Patch", false, false
] call CBA_fnc_addSetting;

// ── Keybinding ────────────────────────────────────────────────────
["a3sql_patch_editor_key", "EDITOR",
    ["Open Patch Editor", "Open the dynamic patch rule editor dialog."],
    "A3SQL Patch",
    { call a3sql_patch_fnc_openEditor; },
    {},
    []
] call CBA_fnc_addKeybind;
