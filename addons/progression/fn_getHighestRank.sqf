#include "script_component.hpp"

params [["_uid", "", [""]]];

if (_uid == "") exitWith { "" };

private _result = [format ["SELECT highest_rank FROM player_progression WHERE uid = '%1'", _uid]] call a3sql_fnc_selectMap;

if (_result isEqualTo []) exitWith { "" };

(_result select 0) get "highest_rank"
