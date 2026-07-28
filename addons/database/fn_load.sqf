#include "script_component.hpp"

params [
    ["_path", "a3sql.bin", [""]],
    ["_extension", "a3sql"]
];

private _result = _extension callExtension ["load", [_path]];
_result call CBA_fnc_parseJSON
