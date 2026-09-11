#include "../script_component.hpp"

// Built-in apply function: modify AI accuracy and dispersion.
// Called by fnc_apply when apply_function = "a3sql_runtime_fnc_applyAccuracy".
// Reads target unit's skill values and applies the operator.
// Value format: "aimingAccuracy,aimingShake,spotDistance" (any subset).
// Skills are 0.0–1.0 in Arma 3.
params [
    ["_target", objNull, [objNull]],
    ["_rule", createHashMap, [createHashMap]],
    ["_context", createHashMap, [createHashMap]]
];

if (isNull _target) exitWith { [1, "ERR_PARAM", "No target"] };

private _operator = toLower (_rule getOrDefault ["operator", "set"]);
private _value    = _rule getOrDefault ["value", ""];
private _parts    = _value splitString " ,";

private _aimAcc     = if (_parts isNotEqualTo []) then { parseNumber (_parts select 0) } else { -1 };
private _aimShake   = if (count _parts > 1) then { parseNumber (_parts select 1) } else { -1 };
private _spotDist   = if (count _parts > 2) then { parseNumber (_parts select 2) } else { -1 };

private _results = [];

// Aiming accuracy
if (_aimAcc > -1) do {
    private _current = _target skill "aimingAccuracy";
    private _val = _current;
    switch (_operator) do {
        case "set": { _val = _aimAcc; };
        case "mul": { _val = _current * _aimAcc; };
        case "add": { _val = (_current + _aimAcc) max 0 min 1; };
        case "clamp": { _val = _current min _aimAcc; };
    };
    _target setSkill ["aimingAccuracy", _val max 0 min 1];
    _results pushBack "aimingAccuracy";
};

// Aiming shake (recoil control)
if (_aimShake > -1) do {
    private _current = _target skill "aimingShake";
    private _val = _current;
    switch (_operator) do {
        case "set": { _val = _aimShake; };
        case "mul": { _val = _current * _aimShake; };
        case "add": { _val = (_current + _aimShake) max 0 min 1; };
        case "clamp": { _val = _current min _aimShake; };
    };
    _target setSkill ["aimingShake", _val max 0 min 1];
    _results pushBack "aimingShake";
};

// Spot distance
if (_spotDist > -1) do {
    private _current = _target skill "spotDistance";
    private _val = _current;
    switch (_operator) do {
        case "set": { _val = _spotDist; };
        case "mul": { _val = _current * _spotDist; };
        case "add": { _val = (_current + _spotDist) max 0 min 1; };
        case "clamp": { _val = _current min _spotDist; };
    };
    _target setSkill ["spotDistance", _val max 0 min 1];
    _results pushBack "spotDistance";
};

if (_results isEqualTo []) exitWith { [1, "ERR_PARAM", "No valid skill fields in value"] };

[0, "OK", _results]
