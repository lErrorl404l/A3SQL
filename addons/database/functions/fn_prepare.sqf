#include "../script_component.hpp"

params [
    ["_name", "", [""]],
    ["_sql", "", [""]],
    ["_extension", "a3sql"]
];

if (_name isEqualTo "") exitWith { [0, "ERR_EXEC", "Empty prepared statement name"] };
if (_sql isEqualTo "") exitWith { [0, "ERR_EXEC", "Empty SQL template"] };

private _response = _extension callExtension ["prepare", [_name, _sql]];
_response call CBA_fnc_parseJSON
