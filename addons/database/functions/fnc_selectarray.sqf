#include "../script_component.hpp"

params [
    ["_sql", "", [""]],
    ["_extension", "a3sql"]
];

if (_sql isEqualTo "") exitWith { [] };

private _result = [_sql, _extension] call FUNC(execute);
if ((_result select 0) != 0) exitWith { [] };

private _data = _result select 2;
if (_data isEqualType []) then {
    if (count _data < 2) exitWith { [] };
    _data select [1]  // rows only (skip header)
} else {
    []
}
