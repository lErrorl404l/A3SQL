#include "../script_component.hpp"

params [
    ["_name", "", [""]],
    ["_params", [], [[]]],
    ["_extension", "a3sql"]
];

if (_name isEqualTo "") exitWith { [0, "ERR_EXEC", "Empty prepared statement name"] };

private _cmd = format ["execute_prepared %1", _name];
private _response = _extension callExtension [_cmd, _params];
_response call CBA_fnc_parseJSON
