#include "script_component.hpp"

params [["_extension", "a3db"]];

private _version = _extension callExtension "version";
private _log_level = missionNamespace getVariable ["a3db_log_level", 2];

if (_log_level >= 2) then {
    diag_log text format ["[A3DB] %1", _version];
};

_version
