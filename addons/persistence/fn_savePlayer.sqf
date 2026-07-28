#include "script_component.hpp"

params [["_uid", "", [""]], ["_unit", objNull, [objNull]]];

if (_uid == "" || isNull _unit) exitWith {};

private _unitType = typeOf _unit;
private _pos = getPosASL _unit;
private _dir = getDir _unit;
private _damage = damage _unit;
private _loadout = str (getUnitLoadout _unit);
private _vehicle = "";
private _vehicleRole = "";

private _veh = vehicle _unit;
if (_veh != _unit) then {
    _vehicle = typeOf _veh;
    _vehicleRole = str (assignedVehicleRole _unit);
};

private _now = str date;
private _mission = missionName;

// ponytail: inline escape for SQL single-quotes, extract to shared fn if more fns need it
_loadout = _loadout regexReplace ["'", "''"];
_vehicleRole = _vehicleRole regexReplace ["'", "''"];

private _sql = format [
    "INSERT OR REPLACE INTO player_save (uid, unit_type, pos_x, pos_y, pos_z, dir, damage, loadout_json, vehicle, vehicle_role, mission_name, saved_at) VALUES ('%1', '%2', %3, %4, %5, %6, %7, '%8', '%9', '%10', '%11', '%12')",
    _uid, _unitType, _pos select 0, _pos select 1, _pos select 2, _dir, _damage, _loadout, _vehicle, _vehicleRole, _mission, _now
];

_sql call a3sql_fnc_execute;
