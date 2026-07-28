#include "script_component.hpp"

// Create player save table
"CREATE TABLE IF NOT EXISTS player_save (uid TEXT PRIMARY KEY, unit_type TEXT, pos_x FLOAT, pos_y FLOAT, pos_z FLOAT, dir FLOAT, damage FLOAT, loadout_json TEXT, vehicle TEXT, vehicle_role TEXT, mission_name TEXT, saved_at TEXT)" call a3sql_database_fnc_execute;

// Register HandleDisconnect — saves player state on disconnect
addMissionEventHandler ["HandleDisconnect", { _this call a3sql_persistence_fnc_handleDisconnect; }];

// Register JIP restore — restores player state on JIP
addMissionEventHandler ["PlayerConnected", { _this call a3sql_persistence_fnc_handleJIP; }];
