#include "script_component.hpp"

if (!isServer) exitWith {};

// ── Version guard ────────────────────────────────────────────────────
// CBA is declared in requiredAddons so the game refuses to load without
// it. This guard is defensive: it stops the engine cleanly if CBA is
// present but the runtime-critical modules are missing.
if !(isClass (configFile >> "CfgPatches" >> "cba_events") && isClass (configFile >> "CfgPatches" >> "cba_keybinding")) exitWith {
    ERROR("CBA events/keybinding modules missing - runtime engine disabled");
};

// ── Cross-mod event listeners + operator keybinds ───────────────────
[] call FUNC(events);
[] call FUNC(keybinds);

// ── Master switch ────────────────────────────────────────────────────
// The CBA setting a3sql_runtime_enabled is the default. Missions can
// override per-mission via the a3sql_runtime_toggle event. Default on.
missionNamespace setVariable [QGVAR(enabled), ["a3sql_runtime_enabled"] call CBA_fnc_getSetting];

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
        if !(missionNamespace getVariable [QGVAR(enabled), true]) exitWith {};
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

// ── DB init: create table + load rules ─────────────────────────────
// Defer to mission start (time > 0). At postInit on a dedicated server
// time is still 0 during briefing and the a3sql extension may not have
// completed its own listener setup. waitUntilAndExecute runs unscheduled
// and re-checks every frame, so rules are live from the first gameplay
// frame without blocking mission load.
[{
    time > 0
}, {
    [] call FUNC(register);
    [] call FUNC(reload);
}, [], 30, {
    ERROR("DB init timed out after 30s - rules not loaded");
}] call CBA_fnc_waitUntilAndExecute;

// ── Mission-wide EntityKilled handler ──────────────────────────────
addMissionEventHandler ["EntityKilled", {
    params ["_killed", "_killer", "_instigator", "_useEffects"];
    if !(missionNamespace getVariable [QGVAR(enabled), true]) exitWith {};
    [_killed, _killer] call FUNC(handleKilled);
}];

// ── Mission end cleanup ────────────────────────────────────────────
addMissionEventHandler ["Ended", {
    GVAR(trackedObjects) = nil;
    if (missionNamespace getVariable ["a3sql_runtime_log_level", 1] >= 2) then {
        INFO("Runtime engine cleanup complete");
    };
}];

