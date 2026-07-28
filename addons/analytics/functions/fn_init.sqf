#include "..\script_component.hpp"

params [["_extension", "a3sql", [""]]];

private _version = _extension callExtension "version";
private _debug = ["a3sql_analytics_debug"] call CBA_fnc_getSetting;

if (_debug) then {
    ["A3SQL Analytics", "%1", _version] call CBA_fnc_info;
};

_version
