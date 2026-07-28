#include "..\script_component.hpp"

params ["_id", "_uid", "_name", "_jip", "_owner", "_idStr"];

if (!_jip) exitWith {};
if (!(["a3sql_persistence_restore_on_jip"] call CBA_fnc_getSetting)) exitWith {};

// Wait for player unit to exist (set by CBA via setPlayerVariable), then restore
[{
    params ["_uid"];
    !isNull (missionNamespace getVariable [format ["player_%1", _uid], objNull])
}, {
    params ["_uid"];
    [_uid] call a3sql_persistence_fnc_restorePlayer;
}, [_uid], 10, {
    params ["_uid"];
    ["A3SQL Persistence", "JIP restore timed out for UID %1", _uid] call CBA_fnc_error;
}] call CBA_fnc_waitUntilAndExec;
