#include "script_component.hpp"

["a3sql_patch_enabled", "CHECKBOX",
    ["Enable Patching", "Enable dynamic patching system."],
    "A3SQL Patch", true, false
] call CBA_fnc_addSetting;

["a3sql_patch_log_level", "LIST",
    ["Log Level", "Verbosity of .rpt diagnostic messages for patching."],
    "A3SQL Patch",
    [[0, 1, 2, 3], ["ERROR", "WARN", "INFO", "DEBUG"], 2],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_check_interval_hz", "SLIDER",
    ["Check Interval (Hz)", "How often PerFrame handler checks for new patch rules. 0 = every frame."],
    "A3SQL Patch",
    [0, 20, 5, 0],
    false
] call CBA_fnc_addSetting;

["a3sql_patch_allow_sqf_exec", "CHECKBOX",
    ["Allow SQF Exec Operator", "WARNING: Enables arbitrary SQF execution via sqf_exec operator. RCE risk."],
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
