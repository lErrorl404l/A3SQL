#include "../script_component.hpp"

params ["_uid", "_reason"];
if (_uid == "") exitWith { [1, "ERR_PARAM", "UID required"] };

private _params = if (_reason == "" || isNil "_reason") then { _uid } else { format ["%1 %2", _uid, _reason] };
private _sql = format ["INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('kick', '%1', 'sqf', 'pending', datetime('now')) RETURNING id", _params];
private _response = "a3sql" callExtension _sql;
parseSimpleArray _response
