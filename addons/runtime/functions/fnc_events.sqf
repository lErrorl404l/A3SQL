#include "../script_component.hpp"

// Register CBA event listeners for cross-mod control of the runtime
// engine. Any mod or mission can trigger these with:
//     ["a3sql_runtime_reload"] call CBA_fnc_globalEvent;
//     ["a3sql_runtime_toggle", [false]] call CBA_fnc_globalEvent;
//     ["a3sql_runtime_query"] call CBA_fnc_globalEvent;
// The engine replies on "a3sql_runtime_status" with [enabled, ruleCount].
// All listeners are server-side: the engine is server-authoritative and
// the a3sql extension lives in the server process.

// Reload the rule cache from the database. Use after external INSERT,
// UPDATE or DELETE against runtime_overrides.
["a3sql_runtime_reload", {
    if (!isServer) exitWith {};
    private _result = [] call FUNC(reload);
    if (missionNamespace getVariable ["a3sql_runtime_log_level", 1] >= 2) then {
        INFO_1("External reload requested: %1",_result);
    };
}] call CBA_fnc_addEventHandler;

// Toggle the whole engine on or off. Param: [<BOOL> enabled].
// Does not change the CBA setting; it flips a mission-level master switch
// so mission authors can control the engine per mission without touching
// the user setting.
["a3sql_runtime_toggle", {
    params [["_enabled", true, [true]]];
    if (!isServer) exitWith {};
    missionNamespace setVariable [QGVAR(enabled), _enabled];
    if (missionNamespace getVariable ["a3sql_runtime_log_level", 1] >= 2) then {
        INFO_1("Engine toggled: %1",_enabled);
    };
}] call CBA_fnc_addEventHandler;

// Query engine status. Replies on "a3sql_runtime_status" with
// [enabled, ruleCount]. The reply is a global event, so any machine
// that listens gets the answer.
["a3sql_runtime_query", {
    if (!isServer) exitWith {};
    private _rules = missionNamespace getVariable [QGVAR(rules), createHashMap];
    private _count = 0;
    {
        _count = _count + count _y;
    } forEach _rules;
    private _enabled = missionNamespace getVariable [QGVAR(enabled), true];
    ["a3sql_runtime_status", [_enabled, _count]] call CBA_fnc_globalEvent;
}] call CBA_fnc_addEventHandler;

// Status reply handler. Runs on every machine; logs where it is
// meaningful (the client that pressed the keybind sees the answer).
["a3sql_runtime_status", {
    params ["_enabled", "_ruleCount"];
    if (isServer && (missionNamespace getVariable ["a3sql_runtime_log_level", 1] >= 1)) then {
        INFO_2("Status: enabled=%1 rules=%2",_enabled,_ruleCount);
    };
}] call CBA_fnc_addEventHandler;

// Clear the per-object event handler cache. Used when the engine is
// restarted mid-mission (toggle off then on) so objects get re-scanned.
["a3sql_runtime_clearTracking", {
    if (!isServer) exitWith {};
    missionNamespace setVariable [QGVAR(trackedObjects), createHashMap];
}] call CBA_fnc_addEventHandler;

[0, "OK", "Runtime event listeners registered"]

