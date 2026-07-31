#include "../script_component.hpp"

params [["_unit", objNull, [objNull]], ["_killer", objNull, [objNull]], ["_instigator", objNull, [objNull]], ["_useEffects", false, [false]]];

private _now = str date;
private _mission = missionName;
private _killerUID = getPlayerUID _instigator;
private _killerUnit = typeOf _instigator;
private _killerWeapon = currentWeapon _instigator;
private _killerPos = getPosASL _instigator;
private _victimUID = getPlayerUID _unit;
private _victimUnit = typeOf _unit;
private _victimWeapon = currentWeapon _unit;
private _distance = _killerPos distance2D (getPosASL _unit);
private _headshot = parseNumber _useEffects;

private _sql = format [
    "INSERT INTO events_kills (timestamp, killer_uid, killer_unit, killer_weapon, killer_pos_x, killer_pos_y, killer_pos_z, victim_uid, victim_unit, victim_weapon, distance, headshot, mission_name) VALUES ('%1', '%2', '%3', '%4', %5, %6, %7, '%8', '%9', '%10', %11, %12, '%13')",
    [_now] call a3sql_database_fnc_sqlEscape,
    [_killerUID] call a3sql_database_fnc_sqlEscape,
    [_killerUnit] call a3sql_database_fnc_sqlEscape,
    [_killerWeapon] call a3sql_database_fnc_sqlEscape,
    _killerPos select 0,
    _killerPos select 1,
    _killerPos select 2,
    [_victimUID] call a3sql_database_fnc_sqlEscape,
    [_victimUnit] call a3sql_database_fnc_sqlEscape,
    [_victimWeapon] call a3sql_database_fnc_sqlEscape,
    _distance,
    _headshot,
    [_mission] call a3sql_database_fnc_sqlEscape
];
_sql call a3sql_database_fnc_execute;

["a3sql_kill", [_killerUID, _killerWeapon, _victimUID, _distance, _headshot]] call CBA_fnc_globalEvent;
