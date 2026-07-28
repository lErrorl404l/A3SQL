#include "../script_component.hpp"

params [["_command", "", [""]], ["_params", "", [""]], ["_source", "sqf", [""]]];
if (_command == "") exitWith { [1, "ERR_PARAM", "Command required"] };

private _valid = ["kick","ban","missions","lock","unlock","exec","restart","shutdown","admin","vote","say","loadBanlist","saveBanlist","reassign","maxPlayers","password"];
if !(_command in _valid) exitWith { [1, "ERR_PARAM", format ["Invalid command: %1", _command]] };

private _sql = format ["INSERT INTO server_commands (command, params, source, created_at) VALUES ('%1', '%2', '%3', datetime('now'))", _command, _params, _source];
_sql call a3sql_database_fnc_execute;

[0, "OK", "Command queued"]
