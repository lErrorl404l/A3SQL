#include "../script_component.hpp"

params [["_command", "", [""]], ["_params", "", [""]], ["_source", "sqf", [""]]];
if (_command == "") exitWith { [1, "ERR_PARAM", "Command required"] };

private _sql = format ["INSERT INTO server_commands (command, params, source, created_at) VALUES ('%1', '%2', '%3', datetime('now'))", [_command] call a3sql_database_fnc_sqlEscape, [_params] call a3sql_database_fnc_sqlEscape, [_source] call a3sql_database_fnc_sqlEscape];
_sql call a3sql_database_fnc_execute;

[0, "OK", "Command queued"]
