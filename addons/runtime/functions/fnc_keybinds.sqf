#include "../script_component.hpp"

// Register CBA keybinds for operator control of the runtime engine.
// Keybinds are client-side: they fire on the machine where the key is
// pressed and forward the command to the server via CBA global events.
// The engine itself stays server-authoritative.
//
// Keys are [DIK, [shift, ctrl, alt]]. DIK codes from
// \x\cba\addons\main\script_dikCodes.hpp (DIK_F1 = 0x3B).

// Reload rules from the database. Default: Ctrl + F5.
["A3SQL Runtime", "Reload Rules", [LSTRING(KeyReload_DisplayName), LSTRING(KeyReload_Description)], {
    ["a3sql_runtime_reload"] call CBA_fnc_globalEvent;
}, {}, [0x3F, [false, true, false]]] call CBA_fnc_addKeybind;

// Toggle the engine on or off. Default: Ctrl + F6.
["A3SQL Runtime", "Toggle Engine", [LSTRING(KeyToggle_DisplayName), LSTRING(KeyToggle_Description)], {
    private _enabled = missionNamespace getVariable [QGVAR(enabled), true];
    ["a3sql_runtime_toggle", [!_enabled]] call CBA_fnc_globalEvent;
}, {}, [0x40, [false, true, false]]] call CBA_fnc_addKeybind;

// Query engine status (rule count, enabled state). Default: Ctrl + F7.
["A3SQL Runtime", "Query Status", [LSTRING(KeyStatus_DisplayName), LSTRING(KeyStatus_Description)], {
    ["a3sql_runtime_query"] call CBA_fnc_globalEvent;
}, {}, [0x41, [false, true, false]]] call CBA_fnc_addKeybind;

[0, "OK", "Runtime keybinds registered"]
