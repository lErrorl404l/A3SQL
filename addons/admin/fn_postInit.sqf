#include "script_component.hpp"

params [["_extension", "a3sql", [""]]];

if (!isServer) exitWith {};

"CREATE TABLE IF NOT EXISTS server_commands (id INTEGER PRIMARY KEY, command TEXT NOT NULL, params TEXT, status TEXT DEFAULT 'pending', result TEXT, source TEXT, created_at TEXT, executed_at TEXT)" call a3sql_fnc_execute;

[{
    if (!missionNamespace getVariable ["a3sql_admin_enabled", true]) exitWith {};
    if (!isServer) exitWith {};

    private _commands = ["SELECT * FROM server_commands WHERE status='pending' ORDER BY id LIMIT 10"] call a3sql_fnc_selectMap;

    {
        private _id = _x get "id";
        private _cmd = _x get "command";
        private _params = _x get "params";

        private _fullCmd = if (_params == "" || isNil "_params") then { _cmd } else { format ["%1 %2", _cmd, _params] };
        private _result = serverCommand _fullCmd;

        private _status = if (_result) then { "executed" } else { "failed" };
        private _sql = format ["UPDATE server_commands SET status='%1', result='%2', executed_at=datetime('now') WHERE id=%3", _status, _result, _id];
        _sql call a3sql_fnc_execute;

        if (missionNamespace getVariable ["a3sql_admin_log_level", 1] >= 2) then {
            diag_log text format ["[A3SQL Admin] %1: %2 -> %3", _status, _fullCmd, _result];
        };
    } forEach _commands;

}, 2, []] call CBA_fnc_addPerFrameHandler;
