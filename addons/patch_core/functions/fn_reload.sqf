#include "..\script_component.hpp"

params [
    ["_extension", "a3sql", [""]]
];

missionNamespace setVariable ["a3sql_patch_dirty", true];

if (missionNamespace getVariable ["a3sql_patch_log_level", 2] >= 2) then {
    diag_log text "[A3SQL Patch] Reload triggered — dirty flag set";
};

// Immediately run applyAll so the reload takes effect on next frame
if (missionNamespace getVariable ["a3sql_patch_enabled", true]) then {
    [] call FUNC(applyAll);
};

[0, "OK", "Reload queued"]
