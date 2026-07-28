#include "../script_component.hpp"

params [
    ["_matchValue", "", [""]],
    ["_targetType", "", [""]],
    ["_property", "", [""]],
    ["_value", "", [""]],
    ["_name", "", [""]]
];

if (_matchValue == "" || _targetType == "" || _property == "" || _value == "") exitWith {
    [1, "ERR_PARAM", "All fields required"]
};

private _name = if (_name == "") then { format ["override_%1", diag_tickTime] } else { _name };

private _sql = format [
    "INSERT INTO patch_rules (name, active, priority, match_type, match_value, target_type, property, operator, value) VALUES ('%1', 1, 0, 'init', '%2', '%3', '%4', 'set', '%5')",
    _name, _matchValue, _targetType, _property, _value
];
_sql call a3sql_database_fnc_execute;

[0, "OK", "Override registered"]
