#include "script_component.hpp"

params ["_uid", "_duration", "_reason"];
if (_uid == "") exitWith { [1, "ERR_PARAM", "UID required"] };

private _durationStr = if (_duration > 0) then { str _duration } else { "0" };
private _reasonStr = if (_reason == "" || isNil "_reason") then { "" } else { format [" %1", _reason] };
private _params = format ["%1 %2%3", _uid, _durationStr, _reasonStr];
private _sql = format ["INSERT INTO server_commands (command, params, source, status, created_at) VALUES ('ban', '%1', 'sqf', 'pending', datetime('now')) RETURNING id", _params];
private _response = "a3sql" callExtension _sql;
parseSimpleArray _response
