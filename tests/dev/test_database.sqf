diag_log "[A3SQL Test] Starting database tests...";

// Test: basic execute
private _create = ["CREATE TABLE IF NOT EXISTS test_dev (id INT, val TEXT)"] call a3sql_database_fnc_execute;
diag_log format ["[A3SQL Test] execute CREATE: %1", _create];

// Test: insert
private _insert = ["INSERT INTO test_dev VALUES (1, 'hello')"] call a3sql_database_fnc_execute;
diag_log format ["[A3SQL Test] execute INSERT: %1", _insert];

// Test: selectAll
private _rows = ["SELECT * FROM test_dev"] call a3sql_database_fnc_selectAll;
diag_log format ["[A3SQL Test] selectAll: %1", _rows];

// Test: selectArray
private _arr = ["SELECT val FROM test_dev"] call a3sql_database_fnc_selectArray;
diag_log format ["[A3SQL Test] selectArray: %1", _arr];

// Test: selectMap
private _map = ["SELECT id, val FROM test_dev"] call a3sql_database_fnc_selectMap;
diag_log format ["[A3SQL Test] selectMap: %1", _map];

// Cleanup
["DROP TABLE test_dev"] call a3sql_database_fnc_execute;

diag_log "[A3SQL Test] Database tests complete.";
