#include "../script_component.hpp"

params [
    ["_dirty", true, [true]],
    ["_extension", "a3sql", [""]]
];

if (isNil QGVAR(namespace)) then { GVAR(namespace) = [] call CBA_fnc_createNamespace; };
GVAR(namespace) setVariable ["dirty", _dirty];

// Debounced auto-save: save patch_rules 5 seconds after last change
if (_dirty) then {
    [QGVAR(autosave), [], 5, {
        "save patch_rules" call a3sql_fnc_execute;
        ["A3SQL Patch", "Auto-saved patch_rules"] call CBA_fnc_info;
    }] call CBA_fnc_waitAndExec;
};

if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 3) then {
    ["A3SQL Patch", "Dirty flag set to %1", _dirty] call CBA_fnc_debug;
};

[0, "OK", _dirty]
