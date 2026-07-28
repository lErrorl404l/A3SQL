#include "../script_component.hpp"

params [
    ["_ruleId", 0, [0]],
    ["_extension", "a3sql", [""]]
];

if (_ruleId <= 0) exitWith { [1, "ERR_PARAM", "Invalid rule ID"] };

private _sql = format ["SELECT * FROM patch_rules WHERE id = %1", _ruleId];
private _response = _extension callExtension _sql;
if (_response isEqualTo "") exitWith { [1, "ERR_CONN", "No response from extension"] };

private _parsed = parseSimpleArray _response;
if ((_parsed select 0) != 0) exitWith { _parsed };

private _data = _parsed select 2;
if !(_data isEqualType []) exitWith { [1, "ERR_PARSE", "Unexpected result format"] };
if (count _data < 2) exitWith { [1, "ERR_NODATA", "Rule not found"] };

private _headers = _data select 0;
private _rows = _data select [1];
if (_rows isEqualTo []) exitWith { [1, "ERR_NODATA", "Rule not found"] };

private _row = _rows select 0;
private _rule = createHashMap;
{
    _rule set [_x, _row select _forEachIndex];
} forEach _headers;

// Parse match_value if it looks like a JSON / SQF array
private _mv = _rule getOrDefault ["match_value", ""];
if (_mv isEqualType "" && {count _mv > 1 && {_mv select [0, 1] == "["} && {_mv select [count _mv - 1, 1] == "]"}}) then {
    _rule set ["match_value", parseSimpleArray _mv];
};

[0, "OK", _rule]
