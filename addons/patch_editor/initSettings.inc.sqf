#include "script_component.hpp"

// ── Keybinding ────────────────────────────────────────────────────
["a3sql_patch_editor_key", "EDITOR",
    [LSTRING(EditorKey_DisplayName), LSTRING(EditorKey_Description)],
    LSTRING(Editor_Category),
    { call a3sql_patch_editor_fnc_openEditor; },
    {},
    []
] call CBA_fnc_addKeybind;
