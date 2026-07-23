#include "script_component.hpp"

params [
    ["_sql", "", [""]],
    ["_extension", "a3db"]
];

if (_sql isEqualTo "") exitWith { ["ERROR", "Empty SQL"] };

private _result = _extension callExtension _sql;
[_result] call a3db_fnc_parseResult
