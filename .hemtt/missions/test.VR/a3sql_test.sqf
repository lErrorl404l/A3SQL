// A3DB smoke test — paste this entire block into the debug console
// and press the Local Execute button. Results appear in the console
// and are logged to the .rpt file via diag_log.

private _ext = "a3sql";

diag_log "=== A3DB SMOKE TEST ===";

_version = _ext callExtension "version";
systemChat format ["[A3DB] version: %1", _version];

_create = _ext callExtension "CREATE TABLE smoke (id STRING PRIMARY KEY, val INT, data STRING)";
systemChat format ["[A3DB] create: %1", _create];

_insert1 = _ext callExtension "INSERT INTO smoke VALUES ('a', 10, 'hello')";
systemChat format ["[A3DB] insert1: %1", _insert1];

_insert2 = _ext callExtension "INSERT INTO smoke VALUES ('b', 20, 'world')";
systemChat format ["[A3DB] insert2: %1", _insert2];

_select_all = _ext callExtension "SELECT * FROM smoke";
systemChat format ["[A3DB] select *: %1", _select_all];

_select_where = _ext callExtension "SELECT val FROM smoke WHERE id = 'a'";
systemChat format ["[A3DB] select where: %1", _select_where];

_update = _ext callExtension "UPDATE smoke SET val = 15 WHERE id = 'a'";
systemChat format ["[A3DB] update: %1", _update];

_verify = _ext callExtension "SELECT val FROM smoke WHERE id = 'a'";
systemChat format ["[A3DB] update verify: %1", _verify];

_fuzzy = _ext callExtension "SELECT id FROM smoke WHERE val %% '15'";
systemChat format ["[A3DB] fuzzy: %1", _fuzzy];

_begin = _ext callExtension "BEGIN";
systemChat format ["[A3DB] txn begin: %1", _begin];

_txn_insert = _ext callExtension "INSERT INTO smoke VALUES ('c', 30, 'txn_test')";
systemChat format ["[A3DB] txn insert: %1", _txn_insert];

_rollback = _ext callExtension "ROLLBACK";
systemChat format ["[A3DB] txn rollback: %1", _rollback];

_txn_verify = _ext callExtension "SELECT COUNT(*) FROM smoke WHERE id = 'c'";
systemChat format ["[A3DB] txn verify (expect 0): %1", _txn_verify];

_ext callExtension "CREATE TABLE multi_test (k STRING PRIMARY KEY)";
_ext callExtension "INSERT INTO multi_test VALUES ('x')";
_ext callExtension "INSERT INTO multi_test VALUES ('y')";
_multi = _ext callExtension "SELECT * FROM multi_test";
systemChat format ["[A3DB] multi: %1", _multi];

_agg = _ext callExtension "SELECT COUNT(*), SUM(val), AVG(val) FROM smoke";
systemChat format ["[A3DB] agg: %1", _agg];

_order = _ext callExtension "SELECT id FROM smoke ORDER BY val DESC LIMIT 2";
systemChat format ["[A3DB] order: %1", _order];

_like = _ext callExtension "SELECT id FROM smoke WHERE data LIKE 'hel%'";
systemChat format ["[A3DB] like: %1", _like];

_ext callExtension "CREATE TABLE null_test (k STRING PRIMARY KEY, v INT)";
_ext callExtension "INSERT INTO null_test VALUES ('n1', NULL)";
_null = _ext callExtension "SELECT * FROM null_test WHERE v IS NULL";
systemChat format ["[A3DB] null: %1", _null];

_exp_json = _ext callExtension "export json smoke";
systemChat format ["[A3DB] export json: %1", _exp_json];

_exp_csv = _ext callExtension "export csv smoke";
systemChat format ["[A3DB] export csv: %1", _exp_csv];

_dump = _ext callExtension "dump_sql";
systemChat format ["[A3DB] dump sql: %1", _dump];

// Save/load round-trip
_save = _ext callExtension ["save", ["a3sql_smoke_test.bin"]];
systemChat format ["[A3DB] save: %1", _save];

_load = _ext callExtension ["load", ["a3sql_smoke_test.bin"]];
systemChat format ["[A3DB] load: %1", _load];

_reverify = _ext callExtension "SELECT * FROM smoke";
systemChat format ["[A3DB] load verify: %1", _reverify];

// Start TCP listener on port 33306
_listener = _ext callExtension ["listen", []];
systemChat format ["[A3DB] listener: %1", _listener];

diag_log "=== A3DB SMOKE TEST DONE ===";
systemChat "A3DB smoke test complete — check RPT for full results";
