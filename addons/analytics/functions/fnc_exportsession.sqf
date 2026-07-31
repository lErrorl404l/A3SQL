#include "../script_component.hpp"
params [["_mission", "", [""]]];
private _sql = if (_mission == "") then {
    "SELECT * FROM replay_snapshots ORDER BY timestamp"
} else {
    format ["SELECT * FROM replay_snapshots WHERE mission_name = '%1' ORDER BY timestamp", [_mission] call a3sql_database_fnc_sqlEscape]
};
[_sql] call a3sql_database_fnc_selectMap;
