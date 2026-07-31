#include "../script_component.hpp"

params ["_uid"];
if (_uid == "") exitWith { [1, "ERR_PARAM", "UID required"] };

[format ["SELECT * FROM players WHERE uid = '%1'", [_uid] call a3sql_database_fnc_sqlEscape]] call a3sql_database_fnc_selectMap;
