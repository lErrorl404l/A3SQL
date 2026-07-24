#include "script_component.hpp"

params [
    ["_path", "a3db.bin", [""]],
    ["_extension", "a3db"]
];

private _result = _extension callExtension ["save", [_path]];
_result call CBA_fnc_parseJSON
