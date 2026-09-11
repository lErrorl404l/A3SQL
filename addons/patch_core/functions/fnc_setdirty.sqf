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
        "save patch_rules" call a3sql_database_fnc_execute;
        INFO("Auto-saved patch_rules");
    }] call CBA_fnc_waitAndExec;
};

if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 3) then {
    INFO_1("Dirty flag set to %1",_dirty);
};

[0, "OK", _dirty]

