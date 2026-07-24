#include "script_component.hpp"

params [["_extension", "a3db"]];

private _version = _extension callExtension "version";
private _log_level = missionNamespace getVariable ["a3db_log_level", 2];

if (_log_level >= 2) then {
    diag_log text format ["[A3DB] %1", _version];
};

// Start TCP listener at game startup (not mission start)
// Removes the need to join a mission before the listener is available.
if (missionNamespace getVariable ["a3db_listener_enabled", true]) then {
    private _port_str = missionNamespace getVariable ["a3db_listener_port", "33306"];
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };
    private _result = _extension callExtension ["listen", [str _port]];
    if (_log_level >= 1) then {
        diag_log text format ["[A3DB] Listener on port %1: %2", _port, _result];
    };
};

_version
