#include "..\script_component.hpp"

params [
    ["_dirty", true, [true]],
    ["_extension", "a3sql", [""]]
];

missionNamespace setVariable ["a3sql_patch_dirty", _dirty];

if (missionNamespace getVariable ["a3sql_patch_log_level", 2] >= 3) then {
    diag_log text format ["[A3SQL Patch] Dirty flag set to %1", _dirty];
};

[0, "OK", _dirty]
