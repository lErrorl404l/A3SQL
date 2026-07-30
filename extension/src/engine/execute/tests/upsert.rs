// Upsert (INSERT … ON CONFLICT DO UPDATE) tests

use super::helpers::*;

#[test]
fn upsert_on_conflict_do_update_excluded() {
    let mut db = Database::new();
    parse_and_exec("CREATE TABLE t (id STRING PRIMARY KEY, v INT)", &mut db).unwrap();
    parse_and_exec("INSERT INTO t VALUES ('a', 100)", &mut db).unwrap();
    parse_and_exec(
        "INSERT INTO t VALUES ('a', 200) ON CONFLICT (id) DO UPDATE SET v = EXCLUDED.v",
        &mut db,
    )
    .unwrap();
    let result = parse_and_exec("SELECT v FROM t WHERE id = 'a'", &mut db).unwrap();
    assert!(result.contains("200"), "UPSERT should update v to 200, got: {}", result);
}
