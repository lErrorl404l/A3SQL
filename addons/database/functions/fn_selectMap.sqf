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
    private _headers = _data select 0;
    private _rows = _data select [1];
    _rows apply {
        private _row = _x;
        private _obj = createHashMap;
        {
            _obj set [_x, _row select _forEachIndex];
        } forEach _headers;
        _obj
    }
} else {
    []
}
