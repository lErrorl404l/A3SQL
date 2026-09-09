diag_log "[A3SQL Test] Starting progression tests...";

// Test: getProgression (table may be empty)
private _prog = ["test_uid"] call a3sql_progression_fnc_getProgression;
diag_log format ["[A3SQL Test] getProgression: %1", _prog];

// Test: getHighestRank (no data, expect empty/default)
private _highest = ["test_uid"] call a3sql_progression_fnc_getHighestRank;
diag_log format ["[A3SQL Test] getHighestRank: %1", _highest];

// Test: queryStats
private _stats = call a3sql_progression_fnc_queryStats;
diag_log format ["[A3SQL Test] queryStats: %1", _stats];

diag_log "[A3SQL Test] Progression tests complete.";
