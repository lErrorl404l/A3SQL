#include "script_component.hpp"

private _extension = "a3sql";
private _enabled = missionNamespace getVariable ["a3sql_patch_enabled", true];
private _log_level = missionNamespace getVariable ["a3sql_patch_log_level", 2];

// ── Auto-create patch_rules table ──────────────────────────────────
private _createTable = "CREATE TABLE IF NOT EXISTS patch_rules (id INTEGER PRIMARY KEY, name TEXT NOT NULL, active INTEGER DEFAULT 1, priority INTEGER DEFAULT 0, match_type TEXT NOT NULL DEFAULT 'exact', match_value TEXT DEFAULT '', target_type TEXT NOT NULL, property TEXT NOT NULL, operator TEXT DEFAULT 'set', value TEXT NOT NULL, created_at TEXT DEFAULT '')";
private _result = _extension callExtension _createTable;
if (_log_level >= 2) then {
    diag_log text format ["[A3SQL Patch] Table init: %1", _result];
};

// ── PerFrame handler ───────────────────────────────────────────────
if (_enabled) then {
    private _interval = missionNamespace getVariable ["a3sql_patch_check_interval_hz", 5];
    if (_interval <= 0) then { _interval = 0; };
    private _hz = if (_interval > 0) then { 1 / _interval } else { 0 };
    private _maxTicks = round(60 / (if (_hz > 0) then { _hz } else { 0.05 }));

    [_hz, [0, _maxTicks], {
        params ["_args"];
        _args params ["_tick", "_maxTicks"];
        if (!missionNamespace getVariable ["a3sql_patch_enabled", true]) exitWith {};
        private _dirty = missionNamespace getVariable ["a3sql_patch_dirty", true];
        private _timeout = (_tick >= _maxTicks);
        if (_dirty || _timeout) then {
            [] call a3sql_patch_fnc_applyAll;
            _args set [0, 0];
        } else {
            _args set [0, _tick + 1];
        };
    }] call CBA_fnc_addPerFrameHandler;
};

// ── JIP handler ────────────────────────────────────────────────────
addMissionEventHandler ["PlayerConnected", {
    params ["_id", "_uid", "_name", "_jip", "_owner", "_idStr"];
    if (_jip) then {
        if (_log_level >= 2) then {
            diag_log text format ["[A3SQL Patch] JIP player %1 (%2) — applying patches", _name, _uid];
        };
        [] call a3sql_patch_fnc_applyAll;
    };
}];

// ── Mission end cleanup ────────────────────────────────────────────
addMissionEventHandler ["Ended", {
    if (missionNamespace getVariable ["a3sql_patch_log_level", 2] >= 2) then {
        diag_log text "[A3SQL Patch] Mission ended — patch system cleanup complete";
    };
}];
