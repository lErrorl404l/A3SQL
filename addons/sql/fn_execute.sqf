#include "script_component.hpp"

params [
    ["_sql", "", [""]],
    ["_extension", "a3db"]
];

if (_sql isEqualTo "") exitWith { ["ERROR", "Empty SQL"] };

private _response = _extension callExtension _sql;
if (_response isEqualTo "") exitWith { [0, "", []] };

_response call CBA_fnc_parseJSON
