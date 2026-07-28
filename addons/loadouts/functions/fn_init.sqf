#include "../script_component.hpp"

params [["_extension", "a3sql"]];

private _version = _extension callExtension "version";

["A3SQL Loadouts", "%1", _version] call CBA_fnc_info;

_version
