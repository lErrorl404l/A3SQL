#include "script_component.hpp"

params [["_extension", "a3sql", [""]]];

private _version = _extension callExtension "version";
private _debug = missionNamespace getVariable ["a3sql_analytics_debug", false];

if (_debug) then {
    diag_log text format ["[A3SQL Analytics] %1", _version];
};

_version
