#include "../script_component.hpp"

params [["_unit", objNull, [objNull]], ["_weapon", "", [""]], ["_muzzle", "", [""]], ["_mode", "", [""]], ["_ammo", "", [""]], ["_magazine", "", [""]], ["_projectile", objNull, [objNull]], ["_vehicle", objNull, [objNull]], ["_global", false, [false]], ["_fireGunner", false, [false]]];

private _uid = getPlayerUID _unit;
private _now = str date;
private _mission = missionName;

// Add to buffer — batch insert handled by PerFrame
GVAR(shot_buffer) pushBack [_now, _uid, _weapon, _mission];
