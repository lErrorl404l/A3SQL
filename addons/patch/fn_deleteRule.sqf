#include "script_component.hpp"

params [
    ["_ruleId", 0, [0]],
    ["_extension", "a3sql", [""]]
];

if (_ruleId <= 0) exitWith { [1, "ERR_PARAM", "Invalid rule ID"] };

private _sql = format ["DELETE FROM patch_rules WHERE id = %1", _ruleId];
private _response = _extension callExtension _sql;
private _parsed = parseSimpleArray _response;

if ((_parsed select 0) == 0) then {
    // Mark dirty so applyAll picks up the change
    missionNamespace setVariable ["a3sql_patch_dirty", true];
    if (missionNamespace getVariable ["a3sql_patch_log_level", 2] >= 2) then {
        diag_log text format ["[A3SQL Patch] Rule %1 deleted", _ruleId];
    };
};

_parsed
