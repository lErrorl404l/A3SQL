#include "..\script_component.hpp"

params [
    ["_extension", "a3sql", [""]]
];

missionNamespace setVariable ["a3sql_patch_dirty", true];

if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 3) then {
    diag_log text "[A3SQL Patch] Reload triggered — dirty flag set";
};

// Immediately run applyAll so the reload takes effect on next frame
if (["a3sql_patch_enabled"] call CBA_fnc_getSetting) then {
    [] call FUNC(applyAll);
};

[0, "OK", "Reload queued"]
