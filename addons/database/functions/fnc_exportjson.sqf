#include "../script_component.hpp"

params [
    ["_table", "", [""]],
    ["_extension", "a3sql"]
];

private _result = _extension callExtension [format ["export json %1", _table], []];
_result call CBA_fnc_parseJSON
