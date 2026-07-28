#include "../script_component.hpp"

// Record positions of all playable entities
// Uses allUnits + vehicles only (NOT allMissionObjects "All")
// INSERTs in batches of 50

private _mission = missionName;
private _now = str date;
private _batch = [];

// Snapshot allUnits
{
    private _pos = getPosASL _x;
    _batch pushBack format ["('%1', '%2', %3, %4, %5, %6, '%7', '%8')",
        _now, typeOf _x, _pos select 0, _pos select 1, _pos select 2,
        1 - damage _x, groupID (group _x), _mission
    ];
    if (count _batch >= 50) then {
        private _sql = format ["INSERT INTO replay_snapshots (timestamp, entity_type, pos_x, pos_y, pos_z, health, group_id, mission_name) VALUES %1", _batch joinString ","];
        _sql call a3sql_database_fnc_execute;
        _batch = [];
    };
} forEach allUnits;

// Snapshot vehicles
{
    private _pos = getPosASL _x;
    _batch pushBack format ["('%1', '%2', %3, %4, %5, %6, '%7', '%8')",
        _now, typeOf _x, _pos select 0, _pos select 1, _pos select 2,
        1 - damage _x, "", _mission
    ];
    if (count _batch >= 50) then {
        private _sql = format ["INSERT INTO replay_snapshots (timestamp, entity_type, pos_x, pos_y, pos_z, health, group_id, mission_name) VALUES %1", _batch joinString ","];
        _sql call a3sql_database_fnc_execute;
        _batch = [];
    };
} forEach vehicles;

// Flush remaining
if (_batch isNotEqualTo []) then {
    private _sql = format ["INSERT INTO replay_snapshots (timestamp, entity_type, pos_x, pos_y, pos_z, health, group_id, mission_name) VALUES %1", _batch joinString ","];
    _sql call a3sql_database_fnc_execute;
};
