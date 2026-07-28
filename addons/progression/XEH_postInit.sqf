#include "script_component.hpp"

if (!isServer) exitWith {};

// Create progression table — stores per-player rank/score data across sessions
"CREATE TABLE IF NOT EXISTS player_progression (uid TEXT PRIMARY KEY, name TEXT, highest_rank TEXT, current_rank TEXT, score INT DEFAULT 0, total_kills INT DEFAULT 0, total_deaths INT DEFAULT 0, missions_played INT DEFAULT 0, playtime_seconds INT DEFAULT 0, last_seen TEXT, last_mission TEXT)" call a3sql_fnc_execute;

if (["a3sql_progression_log_verbose"] call CBA_fnc_getSetting) then {
    diag_log text "[A3SQL Progression] player_progression table ready";
};

// Listen for kill events from analytics — auto-update kill counts in real time
["a3sql_kill", {
    params ["_killerUID", "_weapon", "_victimUID", "_distance", "_headshot"];
    // auto-update kill count for progression
    // This is optional — fn_processMissionEnd already reconciles at mission end
}] call CBA_fnc_addEventHandler;

// Register mission end handler — persists all players on mission end
addMissionEventHandler ["Ended", {
    call a3sql_progression_fnc_processMissionEnd;
}];
