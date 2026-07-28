#include "..\script_component.hpp"

params [["_include_pending", true, [true]], ["_include_executed", true, [true]], ["_limit", 50, [0]]];

private _conditions = [];
if (_include_pending) then { _conditions pushBack "status='pending'" };
if (_include_executed) then { _conditions pushBack "status='executed'" };
if (_conditions isEqualTo []) exitWith { [] };

private _where = _conditions joinString " OR ";
private _sql = format ["SELECT * FROM server_commands WHERE %1 ORDER BY id DESC LIMIT %2", _where, _limit];

[_sql] call a3sql_fnc_selectMap
