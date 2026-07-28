#include "..\script_component.hpp"

params [["_extension", "a3sql"]];

private _version = _extension callExtension "version";
private _log_level = missionNamespace getVariable ["a3sql_log_level", 2];

if (_log_level >= 2) then {
    diag_log text format ["[A3SQL] %1", _version];
};

// Pass credentials to Rust extension for TCP auth (does nothing if empty)
private _user = missionNamespace getVariable ["a3sql_listener_user", ""];
private _pass = missionNamespace getVariable ["a3sql_listener_password", ""];
if (_user != "" && _pass != "") then {
    _extension callExtension ["set_credentials", [_user, _pass]];
};

// Fallback: start listener if PreInit didn't already start it
if (missionNamespace getVariable ["a3sql_listener_enabled", true]) then {
    private _port_str = missionNamespace getVariable ["a3sql_listener_port", "33306"];
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };
    private _result = _extension callExtension ["listen", [str _port]];
    if (_log_level >= 1) then {
        diag_log text format ["[A3SQL] Listener on port %1: %2", _port, _result];
    };
};

_version
