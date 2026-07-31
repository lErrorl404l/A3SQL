#include "../script_component.hpp"

params [["_uid", "", [""]]];

if (_uid == "") exitWith {};

private _sql = format [
    "SELECT unit_type, pos_x, pos_y, pos_z, dir, damage, loadout_json, vehicle, vehicle_role FROM player_save WHERE uid = '%1' AND mission_name = '%2'",
    [_uid] call a3sql_database_fnc_sqlEscape, [missionName] call a3sql_database_fnc_sqlEscape
];
private _result = _sql call a3sql_database_fnc_selectAll;

// Check result: [0, "OK", [[headers], ...rows]]
if (_result isEqualTo [] || {_result select 0 != 0}) exitWith {};

private _rows = _result select 2;
if (count _rows < 2) exitWith {}; // header only, no data

private _data = _rows select 1;
_data params ["_unitType", "_posX", "_posY", "_posZ", "_dir", "_damage", "_loadoutJSON", "_vehicle", "_vehicleRole"];

// Find player unit (JIP handler waits for unit existence before calling us,
// but guard against edge cases)
private _unit = missionNamespace getVariable [format ["player_%1", _uid], objNull];
if (isNull _unit) exitWith {
    ["A3SQL Persistence", "Restore failed: player unit null for UID %1", _uid] call CBA_fnc_error;
};

// Restore loadout
private _loadout = parseSimpleArray _loadoutJSON;
_unit setUnitLoadout _loadout;

// Restore position
_unit setPosASL [_posX, _posY, _posZ];
_unit setDir _dir;

// Restore damage (cap at 0.95 to avoid respawn-on-load)
if (_damage >= 1) then { _damage = 0.95; };
_unit setDamage _damage;

// Vehicle restore
if (_vehicle != "") then {
    private _nearVehs = nearestObjects [_unit, [_vehicle], 50];
    if (_nearVehs isNotEqualTo []) then {
        private _veh = _nearVehs select 0;
        if (!isNull _veh) then {
            private _role = parseSimpleArray _vehicleRole;
            if (_role isNotEqualTo []) then {
                private _roleType = _role select 0;
                private _roleIndex = if (count _role > 1) then { _role select 1 } else { -1 };
                switch (_roleType) do {
                    case "driver": { _unit moveInDriver _veh; };
                    case "gunner": { _unit moveInGunner _veh; };
                    case "commander": { _unit moveInCommander _veh; };
                    default {
                        if (_roleIndex >= 0) then {
                            _unit moveInCargo [_veh, _roleIndex];
                        } else {
                            _unit moveInAny _veh;
                        };
                    };
                };
            };
        };
    };
};

if (["a3sql_persistence_debug"] call CBA_fnc_getSetting) then {
    ["A3SQL Persistence", format ["Restored player %1", _uid]] call CBA_fnc_notify;
};
