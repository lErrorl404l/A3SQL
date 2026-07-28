#include "..\script_component.hpp"

params [["_extension", "a3sql"]];

private _version = _extension callExtension "version";
private _log_level = missionNamespace getVariable ["a3sql_patch_log_level", 2];

if (_log_level >= 2) then {
    diag_log text format ["[A3SQL Patch] %1", _version];
};

_version
