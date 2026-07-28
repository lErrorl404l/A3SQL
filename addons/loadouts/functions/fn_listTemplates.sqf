#include "../script_component.hpp"

private _sql = "SELECT faction, COUNT(*) as count FROM loadout_templates GROUP BY faction";
private _response = "a3sql" callExtension _sql;

if (_response isEqualTo "") exitWith { [1, "ERR_CONN", "No response from extension"] };

private _parsed = parseSimpleArray _response;
if ((_parsed select 0) != 0) exitWith { _parsed };

private _data = _parsed select 2;
if !(_data isEqualType []) exitWith { [1, "ERR_PARSE", "Unexpected result format"] };
if (count _data < 2) exitWith { [1, "ERR_PARSE", "No data in result"] };

private _headers = _data select 0;
private _rows = _data select [1];
private _result = [];

{
    private _row = _x;
    _result pushBack [_row select 0, _row select 1];
} forEach _rows;

[0, "OK", _result]
