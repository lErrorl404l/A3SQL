#include "../script_component.hpp"
params [["_mission", "", [""]]];
private _sql = if (_mission == "") then {
    "SELECT COUNT(*) as count FROM replay_snapshots"
} else {
    format ["SELECT COUNT(*) as count FROM replay_snapshots WHERE mission_name = '%1'", [_mission] call a3sql_database_fnc_sqlEscape]
};
[_sql] call a3sql_database_fnc_selectMap;
