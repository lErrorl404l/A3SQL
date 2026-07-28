#include "..\script_component.hpp"

params [
    ["_targetType", "", [""]],
    ["_matchValue", "", ["", []]],
    ["_extension", "a3sql", [""]]
];

if (_targetType isEqualTo "") exitWith { [1, "ERR_PARAM", "No target type specified"] };

// Escape single quotes in the match_value for SQL safety
private _mvEscaped = if (_matchValue isEqualType "") then {
    _matchValue
} else {
    str _matchValue
};

private _sql = format [
    "SELECT * FROM patch_rules WHERE active = 1 AND target_type = '%1' AND match_value = '%2' ORDER BY priority DESC, id ASC",
    _targetType, _mvEscaped
];
private _response = _extension callExtension _sql;
if (_response isEqualTo "") exitWith { [1, "ERR_CONN", "No response from extension"] };

private _parsed = parseSimpleArray _response;
if ((_parsed select 0) != 0) exitWith { _parsed };

private _data = _parsed select 2;
if !(_data isEqualType []) exitWith { [0, "OK", [0, 0]] };
if (count _data < 2) exitWith { [0, "OK", [0, 0]] };

private _headers = _data select 0;
private _rows = _data select [1];
if (_rows isEqualTo []) exitWith { [0, "OK", [0, 0]] };

private _totalApplied = 0;
private _totalErrors = 0;

{
    private _row = _x;
    private _rule = createHashMap;
    {
        _rule set [_x, _row select _forEachIndex];
    } forEach _headers;

    private _mv = _rule getOrDefault ["match_value", ""];
    if (_mv isEqualType "" && {count _mv > 1 && {_mv select [0, 1] == "["} && {_mv select [count _mv - 1, 1] == "]"}}) then {
        _rule set ["match_value", parseSimpleArray _mv];
    };

    private _result = [_rule, _extension] call FUNC(applyRule);
    if ((_result select 0) == 0) then {
        private _counts = _result select 2;
        _totalApplied = _totalApplied + (_counts param [0, 0]);
        _totalErrors  = _totalErrors  + (_counts param [1, 0]);
    } else {
        _totalErrors = _totalErrors + 1;
    };
} forEach _rows;

[0, "OK", [_totalApplied, _totalErrors]]
