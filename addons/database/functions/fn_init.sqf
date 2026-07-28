#include "..\script_component.hpp"

params [["_extension", "a3sql"]];

private _version = _extension callExtension "version";
private _log_level = ["a3sql_log_level"] call CBA_fnc_getSetting;

if (_log_level >= 2) then {
    diag_log text format ["[A3SQL] %1", _version];
};

// Pass credentials to Rust extension for TCP auth (does nothing if empty)
private _user = ["a3sql_listener_user"] call CBA_fnc_getSetting;
private _pass = ["a3sql_listener_password"] call CBA_fnc_getSetting;
if (_user != "" && _pass != "") then {
    _extension callExtension ["set_credentials", [_user, _pass]];
};

// Fallback: start listener if PreInit didn't already start it
if (["a3sql_listener_enabled"] call CBA_fnc_getSetting) then {
    private _port_str = ["a3sql_listener_port"] call CBA_fnc_getSetting;
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };
    private _result = _extension callExtension ["listen", [str _port]];
    if (_log_level >= 1) then {
        diag_log text format ["[A3SQL] Listener on port %1: %2", _port, _result];
    };
};

_version
