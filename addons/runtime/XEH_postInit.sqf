#include "script_component.hpp"

private _enabled = ["a3sql_runtime_enabled"] call CBA_fnc_getSetting;
if (!_enabled) exitWith {};

// ── Create table and load rules into memory ────────────────────────
[] call FUNC(register);
[] call FUNC(reload);

// ── Mission-wide EntityKilled handler ──────────────────────────────
addMissionEventHandler ["EntityKilled", {
    params ["_killed", "_killer", "_instigator", "_useEffects"];
    [_killed, _killer] call FUNC(handleKilled);
}];

// ── Per-object Fired + HitPart coverage via poll cycle ─────────────
// Fired and HitPart are object-level events, not mission-level.
// Attach once per new object. Server-authoritative: the EH runs on
// the server where the projectile and target are real objects.
GVAR(trackedObjects) = createHashMap;

private _pollHz = ["a3sql_runtime_poll_hz"] call CBA_fnc_getSetting;
if (_pollHz <= 0) then { _pollHz = 0; };
private _interval = if (_pollHz > 0) then { 1 / _pollHz } else { 0 };

if (_interval > 0) then {
    [_interval, {
        if !(["a3sql_runtime_enabled"] call CBA_fnc_getSetting) exitWith {};
        {
            private _key = str _x;
            if !(GVAR(trackedObjects) getOrDefault [_key, false]) then {
                _x addEventHandler ["Fired", {
                    params ["_unit", "_weapon", "_muzzle", "_mode", "_ammo", "_magazine", "_projectile"];
                    [_unit, _weapon, _ammo, _projectile] call FUNC(handleFired);
                }];
                _x addEventHandler ["HitPart", {
                    params ["_target", "_shooter", "_projectile", "_position", "_velocity", "_directHit", "_selection", "_ammo"];
                    [_target, _shooter, _ammo, _selection, _projectile] call FUNC(handleHit);
                }];
                GVAR(trackedObjects) set [_key, true];
            };
        } forEach allMissionObjects "All";
    }] call CBA_fnc_addPerFrameHandler;
};

// ── Mission end cleanup ────────────────────────────────────────────
addMissionEventHandler ["Ended", {
    GVAR(trackedObjects) = nil;
    if (["a3sql_runtime_log_level"] call CBA_fnc_getSetting >= 2) then {
        ["A3SQL Runtime", "Runtime engine cleanup complete"] call CBA_fnc_info;
    };
}];
