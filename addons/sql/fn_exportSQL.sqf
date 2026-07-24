#include "script_component.hpp"

params [
    ["_extension", "a3db"]
];

private _result = _extension callExtension ["export_sql", []];
_result call CBA_fnc_parseJSON
