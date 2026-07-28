#include "..\script_component.hpp"

params [
    ["_handlerName", "", [""]],
    ["_code", {}, [{}]],
    ["_extension", "a3sql", [""]]
];

if (_handlerName isEqualTo "") exitWith { [1, "ERR_PARAM", "No handler name provided"] };

private _varName = format [QGVAR(handler_%1), _handlerName];
missionNamespace setVariable [_varName, _code];

if (["a3sql_patch_log_level"] call CBA_fnc_getSetting >= 3) then {
    ["A3SQL Patch", "Handler registered: %1", _varName] call CBA_fnc_info;
};

[0, "OK", _varName]
