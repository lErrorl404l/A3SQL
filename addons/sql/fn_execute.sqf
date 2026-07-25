#include "script_component.hpp"

params [
    ["_sql", "", [""]],
    ["_extension", "a3sql"],
    ["_params", [], [[]]]
];

if (_sql isEqualTo "") exitWith { ["ERROR", "Empty SQL"] };

private _response = if (_params isEqualTo []) then {
    _extension callExtension _sql
} else {
    _extension callExtension [_sql, _params]
};
if (_response isEqualTo "") exitWith { [0, "", []] };

_response call CBA_fnc_parseJSON
