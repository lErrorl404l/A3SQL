#include "script_component.hpp"

params [["_rule", createHashMap, [createHashMap]]];

private _validTargets = ["weapon","vehicle","magazine","unit","texture","material","entity"];
private _validOps = ["set","inc","sub","mul","div","mod","cat","default","round","clamp","negate","replace","format","sqf_exec"];

private _targetType = toLower (_rule getOrDefault ["target_type", ""]);
private _property = _rule getOrDefault ["property", ""];
private _operator = toLower (_rule getOrDefault ["operator", "set"]);
private _value = _rule getOrDefault ["value", ""];

// Check custom handlers
private _customHandler = missionNamespace getVariable [format ["a3sql_patch_handler_%1", _targetType], nil];
if (!(_targetType in _validTargets) && isNil "_customHandler") exitWith { [false, format ["Invalid target_type: %1", _targetType]] };
if (_property == "") exitWith { [false, "Property cannot be empty"] };
if (!(_operator in _validOps)) exitWith { [false, format ["Invalid operator: %1", _operator]] };
if (_value == "") exitWith { [false, "Value cannot be empty"] };

[true, "OK"]
