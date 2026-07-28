
// ── Keybinding ────────────────────────────────────────────────────
["a3sql_patch_editor_key", "EDITOR",
    "Open Patch Editor",
    "A3SQL Patch Editor",
    { call a3sql_patch_editor_fnc_openEditor; },
    {},
    []
] call CBA_fnc_addKeybind;
