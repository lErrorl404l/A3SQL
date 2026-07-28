#include "script_component.hpp"

params [["_extension", "a3sql", [""]]];

if (!isServer) exitWith {};

// ── Tables ──────────────────────────────────────────────────────
"CREATE TABLE IF NOT EXISTS server_commands (id INTEGER PRIMARY KEY, command TEXT NOT NULL, params TEXT, status TEXT DEFAULT 'pending', result TEXT, source TEXT, created_at TEXT, executed_at TEXT)" call a3sql_fnc_execute;
"CREATE TABLE IF NOT EXISTS players (uid TEXT PRIMARY KEY, name TEXT, ip TEXT, object_id TEXT, unit_type TEXT, rank TEXT, score INT, side TEXT, position TEXT, connected_at TEXT, last_seen TEXT, online INT DEFAULT 1)" call a3sql_fnc_execute;

// ── Player tracking event handlers ──────────────────────────────
addMissionEventHandler ["PlayerConnected", {
    params ["_id", "_uid", "_name", "_jip", "_owner", "_idStr"];
    if (_uid == "") exitWith {};
    private _sql = format [
        "INSERT OR REPLACE INTO players (uid, name, object_id, connected_at, last_seen, online) VALUES ('%1', '%2', '%3', datetime('now'), datetime('now'), 1)",
        _uid, _name, _idStr
    ];
    _sql call a3sql_fnc_execute;
}];

addMissionEventHandler ["HandleDisconnect", {
    params ["_unit", "_id", "_uid", "_name"];
    if (_uid == "") exitWith {};
    private _sql = format [
        "UPDATE players SET online=0, last_seen=datetime('now') WHERE uid='%1'",
        _uid
    ];
    _sql call a3sql_fnc_execute;
}];

// ── PerFrame: periodic player state update + command execution ───
[{
    if (!missionNamespace getVariable ["a3sql_admin_enabled", true]) exitWith {};
    if (!isServer) exitWith {};

    // Update player positions/ranks
    {
        private _uid = getPlayerUID _x;
        if (_uid == "") then { continue };
        private _pos = getPosASL _x;
        private _posStr = format ["%1,%2,%3", _pos select 0, _pos select 1, _pos select 2];
        private _sql = format [
            "UPDATE players SET unit_type='%1', rank='%2', score=%3, side='%4', position='%5', last_seen=datetime('now') WHERE uid='%6'",
            typeOf _x, rank _x, score _x, side _x, _posStr, _uid
        ];
        _sql call a3sql_fnc_execute;
    } forEach (allUnits select {isPlayer _x});

    // Execute pending server commands
    private _commands = ["SELECT * FROM server_commands WHERE status='pending' ORDER BY id LIMIT 10"] call a3sql_fnc_selectMap;
    {
        private _id = _x get "id";
        private _cmd = _x get "command";
        private _params = _x get "params";
        private _fullCmd = if (_params == "" || isNil "_params") then { _cmd } else { format ["%1 %2", _cmd, _params] };
        private _result = str (serverCommand _fullCmd);

        private _status = if (_result == "true") then { "executed" } else { "failed" };
        private _sql = format ["UPDATE server_commands SET status='%1', result='%2', executed_at=datetime('now') WHERE id=%3", _status, _result, _id];
        _sql call a3sql_fnc_execute;

        if (missionNamespace getVariable ["a3sql_admin_log_level", 1] >= 2) then {
            diag_log text format ["[A3SQL Admin] %1: %2 -> %3", _status, _fullCmd, _result];
        };
    } forEach _commands;

}, 5, []] call CBA_fnc_addPerFrameHandler;
