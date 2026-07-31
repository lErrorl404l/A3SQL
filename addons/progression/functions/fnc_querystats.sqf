#include "../script_component.hpp"

params [["_uid", "", [""]]];

if (_uid == "") exitWith { createHashMap };

private _result = [format ["SELECT * FROM player_progression WHERE uid = '%1'", [_uid] call a3sql_database_fnc_sqlEscape]] call a3sql_database_fnc_selectMap;

if (_result isEqualTo []) exitWith { createHashMap };

_result select 0
