diag_log "[A3SQL Test] Starting analytics tests...";

// Test: queryKills (table may be empty)
private _weapons = ["weapons"] call a3sql_analytics_fnc_queryKills;
diag_log format ["[A3SQL Test] queryKills weapons: %1", _weapons];

// Test: getSnapshotCount
private _count = ["test_mission"] call a3sql_analytics_fnc_getSnapshotCount;
diag_log format ["[A3SQL Test] getSnapshotCount: %1", _count];

// Test: takeSnapshot
private _snapshot = call a3sql_analytics_fnc_takeSnapshot;
diag_log format ["[A3SQL Test] takeSnapshot: %1", _snapshot];

diag_log "[A3SQL Test] Analytics tests complete.";
