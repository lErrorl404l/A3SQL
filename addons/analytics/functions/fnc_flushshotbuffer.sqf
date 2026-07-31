#include "../script_component.hpp"

private _buffer = GVAR(shot_buffer);
if (_buffer isEqualTo []) exitWith {};

// Build multi-VALUES INSERT
private _values = [];
{
    _x params ["_ts", "_uid", "_weapon", "_mission"];
    _values pushBack format ["('%1', '%2', '%3', '%4')", [_ts] call a3sql_database_fnc_sqlEscape, [_uid] call a3sql_database_fnc_sqlEscape, [_weapon] call a3sql_database_fnc_sqlEscape, [_mission] call a3sql_database_fnc_sqlEscape];
} forEach _buffer;

private _sql = format ["INSERT INTO events_shots (timestamp, shooter_uid, weapon, mission_name) VALUES %1", _values joinString ","];
_sql call a3sql_database_fnc_execute;

// Clear buffer
GVAR(shot_buffer) = [];
