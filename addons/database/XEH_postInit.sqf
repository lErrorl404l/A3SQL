#include "script_component.hpp"

params [["_extension", "a3sql"]];

if (!isServer) exitWith {};

private _log_level = missionNamespace getVariable ["a3sql_log_level", 2];

// ── Auto-load on mission start ─────────────────────────────────────────
if (missionNamespace getVariable ["a3sql_auto_load", false]) then {
    private _path = missionNamespace getVariable ["a3sql_auto_save_path", "a3sql_autosave.bin"];
    private _result = _extension callExtension ["load", [_path]];
    if (_log_level >= 1) then {
        diag_log text format ["[A3SQL] Auto-load from '%1': %2", _path, _result];
    };
};

// ── TCP Listener ───────────────────────────────────────────────────────
if (missionNamespace getVariable ["a3sql_listener_enabled", false]) then {
    // Pass credentials to the extension before starting the listener
    private _user = missionNamespace getVariable ["a3sql_listener_user", ""];
    private _pass = missionNamespace getVariable ["a3sql_listener_password", ""];
    if (_user != "" && _pass != "") then {
        _extension callExtension ["set_credentials", [_user, _pass]];
    };

    private _port_str = missionNamespace getVariable ["a3sql_listener_port", "33306"];
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };
    private _result = _extension callExtension ["listen", [str _port]];
    if (_log_level >= 1) then {
        diag_log text format ["[A3SQL] Listener on port %1: %2", _port, _result];
    };
};

// ── Auto-save on mission end ───────────────────────────────────────────
if (missionNamespace getVariable ["a3sql_auto_save", false]) then {
    addMissionEventHandler ["Ended", {
        params ["_endType"];
        private _path = missionNamespace getVariable ["a3sql_auto_save_path", "a3sql_autosave.bin"];
        private _result = "a3sql" callExtension ["save", [_path]];
        diag_log text format ["[A3SQL] Auto-save to '%1' on mission %2: %3", _path, _endType, _result];
    }];
};
