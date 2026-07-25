/* A3DB in-game smoke test
 * Run this from the Arma 3 debug console (server only) to verify all features.
 * Outputs results to RPT log.
 */

private _fail = 0;
private _pass = 0;

private _fnc_test = {
    params ["_name", "_sql", "_expected_ok"];
    private _result = [_sql] call a3sql_fnc_execute;
    private _ok = (_result select 0) == 0;
    if (_ok == _expected_ok) then {
        _pass = _pass + 1;
        diag_log text format ["[A3DB TEST] ✓ %1", _name];
    } else {
        _fail = _fail + 1;
        diag_log text format ["[A3DB TEST] ✗ %1 → %2", _name, _result];
    };
};

// Setup
["CREATE TABLE smoke_test (k STRING PRIMARY KEY, v INT, name STRING)", true] call _fnc_test;
// CRUD
["INSERT INTO smoke_test VALUES ('a', 10, 'alpha')", true] call _fnc_test;
["INSERT INTO smoke_test VALUES ('b', 20, 'beta')", true] call _fnc_test;
["SELECT * FROM smoke_test ORDER BY k", true] call _fnc_test;
["UPDATE smoke_test SET v = 99 WHERE k = 'a'", true] call _fnc_test;
["DELETE FROM smoke_test WHERE k = 'b'", true] call _fnc_test;
// Aggregates
["SELECT COUNT(*) FROM smoke_test", true] call _fnc_test;
// Error
["SELECT * FROM nonexistent", false] call _fnc_test;
["DROP TABLE smoke_test", true] call _fnc_test;

diag_log text format ["=== A3DB Smoke Test: %1/%2 passed (%3 failed) ===", _pass, _pass + _fail, _fail];
