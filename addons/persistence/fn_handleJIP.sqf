#include "script_component.hpp"

params ["_id", "_uid", "_name", "_jip", "_owner", "_idStr"];

if (!_jip) exitWith {};
if (!missionNamespace getVariable ["a3sql_persistence_restore_on_jip", true]) exitWith {};

[_uid] spawn {
    params ["_uid"];
    private _timeout = time + 10;
    waitUntil { sleep 0.5; !isNull (missionNamespace getVariable [format ["player_%1", _uid], objNull]) || time > _timeout };
    if (time > _timeout) exitWith {
        diag_log text format ["[A3SQL Persistence] JIP restore timed out for UID %1", _uid];
    };
    [_uid] call a3sql_persistence_fnc_restorePlayer;
};
