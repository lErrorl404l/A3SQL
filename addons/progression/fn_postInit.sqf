#include "script_component.hpp"

if (!isServer) exitWith {};

// Create progression table — stores per-player rank/score data across sessions
"CREATE TABLE IF NOT EXISTS player_progression (uid TEXT PRIMARY KEY, name TEXT, highest_rank TEXT, current_rank TEXT, score INT DEFAULT 0, total_kills INT DEFAULT 0, total_deaths INT DEFAULT 0, missions_played INT DEFAULT 0, playtime_seconds INT DEFAULT 0, last_seen TEXT, last_mission TEXT)" call a3sql_fnc_execute;

if (missionNamespace getVariable ["a3sql_progression_log_verbose", false]) then {
    diag_log text "[A3SQL Progression] player_progression table ready";
};

// Register mission end handler — persists all players on mission end
addMissionEventHandler ["Ended", {
    call a3sql_progression_fnc_processMissionEnd;
}];
