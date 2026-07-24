#include "script_component.hpp"

params [["_extension", "a3db"]];

if (!isServer) exitWith {};

private _log_level = missionNamespace getVariable ["a3db_log_level", 2];

// ── Auto-load on mission start ─────────────────────────────────────────
if (missionNamespace getVariable ["a3db_auto_load", false]) then {
    private _path = missionNamespace getVariable ["a3db_auto_save_path", "a3db_autosave.bin"];
    private _result = _extension callExtension ["load", [_path]];
    if (_log_level >= 1) then {
        diag_log text format ["[A3DB] Auto-load from '%1': %2", _path, _result];
    };
};

// ── TCP Listener ───────────────────────────────────────────────────────
if (missionNamespace getVariable ["a3db_listener_enabled", false]) then {
    private _port_str = missionNamespace getVariable ["a3db_listener_port", "33306"];
    private _port = parseNumber _port_str;
    if (_port <= 0) then { _port = 33306; };
    private _result = _extension callExtension ["listen", [str _port]];
    if (_log_level >= 1) then {
        diag_log text format ["[A3DB] Listener on port %1: %2", _port, _result];
    };
};

// ── Auto-save on mission end ───────────────────────────────────────────
if (missionNamespace getVariable ["a3db_auto_save", false]) then {
    private _path = missionNamespace getVariable ["a3db_auto_save_path", "a3db_autosave.bin"];
    addMissionEventHandler ["Ended", {
        params ["_endType"];
        private _result = _extension callExtension ["save", [_path]];
        if (_log_level >= 1) then {
            diag_log text format ["[A3DB] Auto-save to '%1' on mission %2: %3", _path, _endType, _result];
        };
    }];
    if (_log_level >= 2) then {
        diag_log text format ["[A3DB] Auto-save enabled → '%1'", _path];
    };
};
