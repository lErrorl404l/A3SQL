#include "../script_component.hpp"
params [["_mission", "", [""]]];
private _sql = if (_mission == "") then {
    "SELECT COUNT(*) as count FROM replay_snapshots"
} else {
    format ["SELECT COUNT(*) as count FROM replay_snapshots WHERE mission_name = '%1'", _mission]
};
[_sql] call a3sql_fnc_selectMap;
