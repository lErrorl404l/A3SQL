#include "../script_component.hpp"

params [
    ["_extension", "a3sql"]
];

private _result = _extension callExtension ["dump_sql", []];
_result call CBA_fnc_parseJSON
