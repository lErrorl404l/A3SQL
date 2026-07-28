diag_log "[A3SQL Test] Starting admin tests...";

// Test: addCommand
private _result = ["kick", "76561198000000001", "test"] call a3sql_admin_fnc_addCommand;
diag_log format ["[A3SQL Test] addCommand: %1", _result];

// Test: listCommands
private _cmds = call a3sql_admin_fnc_listCommands;
diag_log format ["[A3SQL Test] listCommands returned: %1 commands", count _cmds];

// Test: listPlayers (table may be empty in dev)
private _players = call a3sql_admin_fnc_listPlayers;
diag_log format ["[A3SQL Test] listPlayers returned: %1", _players];

diag_log "[A3SQL Test] Admin tests complete.";
