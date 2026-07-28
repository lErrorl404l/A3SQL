// A3SQL smoke test — paste this entire block into the debug console
// and press the Local Execute button. Results appear in the console
// and are logged to the .rpt file via diag_log.

private _ext = "a3sql";

diag_log "=== A3SQL SMOKE TEST ===";

_version = _ext callExtension "version";
systemChat format ["[A3SQL] version: %1", _version];

_create = _ext callExtension "CREATE TABLE smoke (id STRING PRIMARY KEY, val INT, data STRING)";
systemChat format ["[A3SQL] create: %1", _create];

_insert1 = _ext callExtension "INSERT INTO smoke VALUES ('a', 10, 'hello')";
systemChat format ["[A3SQL] insert1: %1", _insert1];

_insert2 = _ext callExtension "INSERT INTO smoke VALUES ('b', 20, 'world')";
systemChat format ["[A3SQL] insert2: %1", _insert2];

_select_all = _ext callExtension "SELECT * FROM smoke";
systemChat format ["[A3SQL] select *: %1", _select_all];

_select_where = _ext callExtension "SELECT val FROM smoke WHERE id = 'a'";
systemChat format ["[A3SQL] select where: %1", _select_where];

_update = _ext callExtension "UPDATE smoke SET val = 15 WHERE id = 'a'";
systemChat format ["[A3SQL] update: %1", _update];

_verify = _ext callExtension "SELECT val FROM smoke WHERE id = 'a'";
systemChat format ["[A3SQL] update verify: %1", _verify];

_fuzzy = _ext callExtension "SELECT id FROM smoke WHERE val %% '15'";
systemChat format ["[A3SQL] fuzzy: %1", _fuzzy];

_begin = _ext callExtension "BEGIN";
systemChat format ["[A3SQL] txn begin: %1", _begin];

_txn_insert = _ext callExtension "INSERT INTO smoke VALUES ('c', 30, 'txn_test')";
systemChat format ["[A3SQL] txn insert: %1", _txn_insert];

_rollback = _ext callExtension "ROLLBACK";
systemChat format ["[A3SQL] txn rollback: %1", _rollback];

_txn_verify = _ext callExtension "SELECT COUNT(*) FROM smoke WHERE id = 'c'";
systemChat format ["[A3SQL] txn verify (expect 0): %1", _txn_verify];

_ext callExtension "CREATE TABLE multi_test (k STRING PRIMARY KEY)";
_ext callExtension "INSERT INTO multi_test VALUES ('x')";
_ext callExtension "INSERT INTO multi_test VALUES ('y')";
_multi = _ext callExtension "SELECT * FROM multi_test";
systemChat format ["[A3SQL] multi: %1", _multi];

_agg = _ext callExtension "SELECT COUNT(*), SUM(val), AVG(val) FROM smoke";
systemChat format ["[A3SQL] agg: %1", _agg];

_order = _ext callExtension "SELECT id FROM smoke ORDER BY val DESC LIMIT 2";
systemChat format ["[A3SQL] order: %1", _order];

_like = _ext callExtension "SELECT id FROM smoke WHERE data LIKE 'hel%'";
systemChat format ["[A3SQL] like: %1", _like];

_ext callExtension "CREATE TABLE null_test (k STRING PRIMARY KEY, v INT)";
_ext callExtension "INSERT INTO null_test VALUES ('n1', NULL)";
_null = _ext callExtension "SELECT * FROM null_test WHERE v IS NULL";
systemChat format ["[A3SQL] null: %1", _null];

_exp_json = _ext callExtension "export json smoke";
systemChat format ["[A3SQL] export json: %1", _exp_json];

_exp_csv = _ext callExtension "export csv smoke";
systemChat format ["[A3SQL] export csv: %1", _exp_csv];

_dump = _ext callExtension "dump_sql";
systemChat format ["[A3SQL] dump sql: %1", _dump];

// Save/load round-trip
_save = _ext callExtension ["save", ["a3sql_smoke_test.bin"]];
systemChat format ["[A3SQL] save: %1", _save];

_load = _ext callExtension ["load", ["a3sql_smoke_test.bin"]];
systemChat format ["[A3SQL] load: %1", _load];

_reverify = _ext callExtension "SELECT * FROM smoke";
systemChat format ["[A3SQL] load verify: %1", _reverify];

// Start TCP listener on port 33306
_listener = _ext callExtension ["listen", []];
systemChat format ["[A3SQL] listener: %1", _listener];

diag_log "=== A3SQL SMOKE TEST DONE ===";
systemChat "A3SQL smoke test complete — check RPT for full results";

// ── PATCH SYSTEM SMOKE TESTS ──
diag_log "=== A3SQL PATCH TEST ===";

// 1. live_patch dispatch command
private _lp1 = _ext callExtension ["live_patch", ["texture", "tex1", "a3sql\\test.paa"]];
systemChat format ["[PATCH] live_patch: %1", _lp1];

// 2. patch_rules table auto-created
private _tbl = _ext callExtension "SELECT name FROM sqlite_master WHERE type='table' AND name='patch_rules'";
systemChat format ["[PATCH] patch_rules exists: %1", _tbl];

// 3. Insert a test rule and read it back
private _ins = _ext callExtension "INSERT INTO patch_rules (name, active, priority, target_type, property, value) VALUES ('test_rule', 1, 0, 'texture', 'tex1', 'test.paa')";
systemChat format ["[PATCH] insert rule: %1", _ins];

private _read = _ext callExtension "SELECT name, target_type, property, value FROM patch_rules WHERE name = 'test_rule'";
systemChat format ["[PATCH] read rule: %1", _read];

// 4. fn_setDirty
private _dirty = _ext callExtension "SELECT * FROM patch_rules WHERE active=1 ORDER BY priority";
systemChat format ["[PATCH] select active: %1", _dirty];

// 5. Verify operator functions work (basic call test via SQF eval)
private _op_test = _ext callExtension "SELECT 1+1";
systemChat format ["[PATCH] op test: %1", _op_test];

// 6. Delete test rule and verify
_ext callExtension "DELETE FROM patch_rules WHERE name = 'test_rule'";
private _verify_del = _ext callExtension "SELECT COUNT(*) FROM patch_rules WHERE name = 'test_rule'";
systemChat format ["[PATCH] delete verify: %1", _verify_del];

diag_log "=== A3SQL PATCH TEST DONE ===";
systemChat "A3SQL patch test complete — check RPT for results";
