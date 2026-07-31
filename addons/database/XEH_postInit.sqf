#include "script_component.hpp"

params [["_extension", "a3sql"]];

if (!isServer) exitWith {};

private _log_level = ["a3sql_log_level"] call CBA_fnc_getSetting;

// ── Auto-load on mission start ─────────────────────────────────────────
if (["a3sql_auto_load"] call CBA_fnc_getSetting) then {
    private _path = ["a3sql_database_auto_save_path"] call CBA_fnc_getSetting;
    private _result = _extension callExtension ["load", [_path]];
    if (_log_level >= 1) then {
        ["A3SQL", "Auto-load from '%1': %2", _path, _result] call CBA_fnc_info;
    };
};

// ── TCP Listener ───────────────────────────────────────────────────────
if (["a3sql_listener_enabled"] call CBA_fnc_getSetting) then {
    // Pass credentials to the extension before starting the listener
    private _user = ["a3sql_database_listener_user"] call CBA_fnc_getSetting;
    private _pass = ["a3sql_database_listener_password"] call CBA_fnc_getSetting;
    if (_user != "" && _pass != "") then {
        _extension callExtension ["set_credentials", [_user, _pass]];
    };

    private _port_str = ["a3sql_database_listener_port"] call CBA_fnc_getSetting;
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };
    private _result = _extension callExtension ["listen", [str _port]];
    if (_log_level >= 1) then {
        ["A3SQL", "Listener on port %1: %2", _port, _result] call CBA_fnc_info;
    };
};

// ── Auto-save on mission end ───────────────────────────────────────────
if (["a3sql_auto_save"] call CBA_fnc_getSetting) then {
    addMissionEventHandler ["Ended", {
        params ["_endType"];
        private _path = ["a3sql_database_auto_save_path"] call CBA_fnc_getSetting;
        private _result = "a3sql" callExtension ["save", [_path]];
        ["A3SQL", "Auto-save to '%1' on mission %2: %3", _path, _endType, _result] call CBA_fnc_info;
    }];
};
