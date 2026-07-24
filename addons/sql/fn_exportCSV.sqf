#include "script_component.hpp"

params [
    ["_table", "", [""]],
    ["_extension", "a3db"]
];

private _result = _extension callExtension ["export csv", [_table]];
_result call CBA_fnc_parseJSON
