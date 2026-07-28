#include "script_component.hpp"

// ── Keybinding ────────────────────────────────────────────────────
["a3sql_patch_editor_key", "EDITOR",
    ["STR_A3SQL_Patch_EditorKey_DisplayName", "STR_A3SQL_Patch_EditorKey_Description"],
    "STR_A3SQL_Patch_Editor_Category",
    { call a3sql_patch_editor_fnc_openEditor; },
    {},
    []
] call CBA_fnc_addKeybind;
