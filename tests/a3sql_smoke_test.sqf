/* A3SQL in-game smoke test — comprehensive feature verification
 * Run from Arma 3 debug console (server). Outputs to RPT.
 * Usage: execVM "z\a3db\addons\main\tests\a3sql_smoke_test.sqf"
 */

private _fail = 0;
private _pass = 0;
private _test = {
    params ["_name", "_sql", "_expected_ok"];
    private _result = [_sql] call a3sql_fnc_execute;
    private _code = _result select 0;
    private _ok = _code == 0;
    if (_ok == _expected_ok) then {
        _pass = _pass + 1;
        diag_log text format ["[A3SQL] ✓ %1", _name];
    } else {
        _fail = _fail + 1;
        diag_log text format ["[A3SQL] ✗ %1 — code=%2 msg=%3", _name, _code, _result select 1];
    };
};

// ═══════════════════════════════════════════
// Core SQL
// ═══════════════════════════════════════════
["CREATE TABLE sqf_test (k STRING PRIMARY KEY, v INT, name STRING)", "CREATE TABLE sqf_test (k STRING PRIMARY KEY, v INT, name STRING)", true] call _test;
["INSERT INTO sqf_test VALUES ('a', 10, 'alpha')", "INSERT INTO sqf_test VALUES ('a', 10, 'alpha')", true] call _test;
["INSERT INTO sqf_test VALUES ('b', 20, 'beta')", "INSERT INTO sqf_test VALUES ('b', 20, 'beta')", true] call _test;
["INSERT INTO sqf_test VALUES ('c', 30, 'gamma')", "INSERT INTO sqf_test VALUES ('c', 30, 'gamma')", true] call _test;
["SELECT all rows", "SELECT * FROM sqf_test ORDER BY k", true] call _test;
["WHERE clause", "SELECT * FROM sqf_test WHERE v > 15", true] call _test;
["ORDER BY DESC", "SELECT k, v FROM sqf_test ORDER BY v DESC", true] call _test;
["UPDATE row", "UPDATE sqf_test SET v = 99 WHERE k = 'a'", true] call _test;
["DELETE row", "DELETE FROM sqf_test WHERE k = 'b'", true] call _test;
["COUNT(*)", "SELECT COUNT(*) FROM sqf_test", true] call _test;
["SUM, AVG", "SELECT SUM(v), AVG(v) FROM sqf_test", true] call _test;
["GROUP BY", "SELECT v > 30 AS high, COUNT(*) FROM sqf_test GROUP BY high", true] call _test;
["ORDER BY alias", "SELECT k AS key, v AS val FROM sqf_test ORDER BY key", true] call _test;
["LIMIT", "SELECT * FROM sqf_test LIMIT 1", true] call _test;
["OFFSET", "SELECT * FROM sqf_test OFFSET 1", true] call _test;
["DISTINCT", "SELECT DISTINCT v FROM sqf_test", true] call _test;
["COUNT DISTINCT", "SELECT COUNT(DISTINCT v) FROM sqf_test", true] call _test;

// ═══════════════════════════════════════════
// Transactions
// ═══════════════════════════════════════════
["BEGIN", "BEGIN", true] call _test;
["INSERT in txn", "INSERT INTO sqf_test VALUES ('d', 40, 'delta')", true] call _test;
["ROLLBACK", "ROLLBACK", true] call _test;
["Verify rollback", "SELECT * FROM sqf_test WHERE k = 'd'", true] call _test;
["BEGIN/COMMIT", "BEGIN", true] call _test;
["COMMIT", "COMMIT", true] call _test;

// ═══════════════════════════════════════════
// RETURNING
// ═══════════════════════════════════════════
["INSERT RETURNING", "INSERT INTO sqf_test VALUES ('x', 1, 'return') RETURNING *", true] call _test;
["UPDATE RETURNING", "UPDATE sqf_test SET v = 2 WHERE k = 'x' RETURNING k, v", true] call _test;
["DELETE RETURNING", "DELETE FROM sqf_test WHERE k = 'x' RETURNING k", true] call _test;

// ═══════════════════════════════════════════
// CREATE TABLE AS, VIEWS
// ═══════════════════════════════════════════
["CREATE TABLE AS", "CREATE TABLE sqf_copy AS SELECT * FROM sqf_test", true] call _test;
["DROP TABLE copy", "DROP TABLE sqf_copy", true] call _test;
["CREATE VIEW", "CREATE VIEW sqf_view AS SELECT k, v FROM sqf_test WHERE v > 15", true] call _test;
["SELECT FROM view", "SELECT * FROM sqf_view ORDER BY k", true] call _test;
["DROP VIEW", "DROP VIEW sqf_view", true] call _test;

// ═══════════════════════════════════════════
// CTE (WITH clause)
// ═══════════════════════════════════════════
["CTE simple", "WITH t AS (SELECT * FROM sqf_test) SELECT * FROM t ORDER BY k", true] call _test;
["CTE aggregate", "WITH t AS (SELECT v, COUNT(*) AS cnt FROM sqf_test GROUP BY v) SELECT * FROM t", true] call _test;
["CTE WHERE", "WITH t AS (SELECT * FROM sqf_test WHERE v > 15) SELECT COUNT(*) FROM t", true] call _test;

// ═══════════════════════════════════════════
// Recursive CTE
// ═══════════════════════════════════════════
["Recursive CTE", "WITH RECURSIVE nums(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 5) SELECT COUNT(*) FROM nums", true] call _test;

// ═══════════════════════════════════════════
// Joins
// ═══════════════════════════════════════════
["INNER JOIN", "SELECT a.k, a.v FROM sqf_test a INNER JOIN sqf_test b ON a.k = b.k", true] call _test;
["LEFT JOIN", "SELECT a.k, b.v FROM sqf_test a LEFT JOIN sqf_test b ON a.k = b.k", true] call _test;

// ═══════════════════════════════════════════
// Window functions
// ═══════════════════════════════════════════
["ROW_NUMBER", "SELECT ROW_NUMBER() OVER (ORDER BY v) AS rn, k, v FROM sqf_test", true] call _test;
["RANK", "SELECT RANK() OVER (ORDER BY v) AS rk, k, v FROM sqf_test", true] call _test;

// ═══════════════════════════════════════════
// Set operations
// ═══════════════════════════════════════════
["UNION ALL", "SELECT k FROM sqf_test UNION ALL SELECT k FROM sqf_test", true] call _test;
["EXCEPT", "SELECT 'a' AS k EXCEPT SELECT k FROM sqf_test WHERE k = 'b'", true] call _test;
["INTERSECT", "SELECT 'a' AS k INTERSECT SELECT k FROM sqf_test WHERE k = 'a'", true] call _test;

// ═══════════════════════════════════════════
// Subqueries
// ═══════════════════════════════════════════
["Subquery WHERE", "SELECT * FROM sqf_test WHERE v > (SELECT AVG(v) FROM sqf_test)", true] call _test;
["Subquery FROM", "SELECT * FROM (SELECT k, v FROM sqf_test) sub ORDER BY k", true] call _test;

// ═══════════════════════════════════════════
// EXPLAIN
// ═══════════════════════════════════════════
["EXPLAIN SELECT", "EXPLAIN SELECT * FROM sqf_test", true] call _test;

// ═══════════════════════════════════════════
// DESCRIBE / SHOW
// ═══════════════════════════════════════════
["DESCRIBE TABLE", "DESCRIBE TABLE sqf_test", true] call _test;
["SHOW CREATE TABLE", "SHOW CREATE TABLE sqf_test", true] call _test;
["SHOW TABLES", "SHOW TABLES", true] call _test;

// ═══════════════════════════════════════════
// Constraints
// ═══════════════════════════════════════════
["CREATE TABLE with CHECK", "CREATE TABLE sqf_check (k STRING PRIMARY KEY, v INT CHECK (v > 0))", true] call _test;
["CHECK passes", "INSERT INTO sqf_check VALUES ('a', 5)", true] call _test;
["CHECK fails", "INSERT INTO sqf_check VALUES ('b', -1)", false] call _test;
["DROP TABLE check", "DROP TABLE sqf_check", true] call _test;

// ═══════════════════════════════════════════
// Full-text search
// ═══════════════════════════════════════════
["FTS trigram", "SELECT * FROM sqf_test WHERE name %% 'alp'", true] call _test;

// ═══════════════════════════════════════════
// Persistence
// ═══════════════════════════════════════════
["SAVE", "SAVE sqf_save_test", true] call _test;
["LOAD", "LOAD sqf_save_test", true] call _test;

// ═══════════════════════════════════════════
// Errors
// ═══════════════════════════════════════════
["Table not found", "SELECT * FROM nonexistent", false] call _test;
["Syntax error", "SELECT *", false] call _test;

// ═══════════════════════════════════════════
// Cleanup
// ═══════════════════════════════════════════
["DROP TABLE sqf_test", "DROP TABLE sqf_test", true] call _test;

diag_log text format ["=== A3SQL Smoke Test: %1/%2 passed (%3 failed) ===", _pass, _pass + _fail, _fail];
if (_fail > 0) then { diag_log text "[A3SQL] ⚠ Some tests FAILED" };
_pass
