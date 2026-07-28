#include "script_component.hpp"

params [["_extension", "a3sql", [""]]];

// ── Create tables ──────────────────────────────────────────────
"CREATE TABLE IF NOT EXISTS perf_metrics (id INTEGER PRIMARY KEY, timestamp TEXT, fps FLOAT, fps_min FLOAT, entities INT, vehicles INT, players INT, bandwidth FLOAT, mission_name TEXT)" call a3sql_database_fnc_execute;
"CREATE TABLE IF NOT EXISTS events_kills (id INTEGER PRIMARY KEY, timestamp TEXT, killer_uid TEXT, killer_unit TEXT, killer_weapon TEXT, killer_pos_x FLOAT, killer_pos_y FLOAT, killer_pos_z FLOAT, victim_uid TEXT, victim_unit TEXT, victim_weapon TEXT, distance FLOAT, headshot INT, mission_name TEXT)" call a3sql_database_fnc_execute;
"CREATE TABLE IF NOT EXISTS events_shots (id INTEGER PRIMARY KEY, timestamp TEXT, shooter_uid TEXT, weapon TEXT, mission_name TEXT)" call a3sql_database_fnc_execute;
"CREATE TABLE IF NOT EXISTS replay_snapshots (id INTEGER PRIMARY KEY, timestamp TEXT, entity_type TEXT, pos_x FLOAT, pos_y FLOAT, pos_z FLOAT, health FLOAT, group_id TEXT, mission_name TEXT)" call a3sql_database_fnc_execute;

// ── PerFrame handler (server-only) ─────────────────────────────────
private _sampleInterval = ["a3sql_analytics_sample_interval"] call CBA_fnc_getSetting;
if (_sampleInterval <= 0) then { _sampleInterval = 60; };

[{  // PerFrame code: fires every _sampleInterval seconds
    if (!isServer) exitWith {};

    private _now = str date;
    private _fps = diag_fps;
    private _fpsMin = diag_fpsMin;
    private _entities = count allUnits;
    private _vehicles = count vehicles;
    private _players = {isPlayer _x} count allUnits;
    private _bandwidth = diag_tickTime;  // placeholder — no direct bandwidth SQF cmd
    private _mission = missionName;

    private _sql = format [
        "INSERT INTO perf_metrics (timestamp, fps, fps_min, entities, vehicles, players, bandwidth, mission_name) VALUES ('%1', %2, %3, %4, %5, %6, %7, '%8')",
        _now, _fps, _fpsMin, _entities, _vehicles, _players, _bandwidth, _mission
    ];
    _sql call a3sql_database_fnc_execute;

    if (["a3sql_analytics_debug"] call CBA_fnc_getSetting) then {
        ["A3SQL Analytics", "Perf sample: FPS=%1, Entities=%2, Players=%3", _fps, _entities, _players] call CBA_fnc_debug;
    };
}, _sampleInterval, []] call CBA_fnc_addPerFrameHandler;

// ── Initialize shot buffer ─────────────────────────────────────
GVAR(shot_buffer) = [];

// ── Register event handlers ────────────────────────────────────
addMissionEventHandler ["EntityKilled", { _this call a3sql_analytics_fnc_handleKilled; }];
call a3sql_analytics_fnc_registerFiredMan;

// ── PerFrame handler for shot buffer flush ──────────────────────
[{  // Flush buffer when threshold reached
    private _buffer = GVAR(shot_buffer);
    if (count _buffer >= 10) then {
        call a3sql_analytics_fnc_flushShotBuffer;
    };
}, _sampleInterval, []] call CBA_fnc_addPerFrameHandler;

// ── Mission-end flush ──────────────────────────────────────────
addMissionEventHandler ["Ended", {
    call a3sql_analytics_fnc_flushShotBuffer;
}];

// ── Replay snapshot timer (every 30 seconds) ─────────────────────
[{ call a3sql_analytics_fnc_takeSnapshot; }, 30, []] call CBA_fnc_addPerFrameHandler;
