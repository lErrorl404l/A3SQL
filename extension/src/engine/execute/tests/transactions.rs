// Transaction tests: BEGIN, COMMIT, ROLLBACK

use super::helpers::*;

#[test]
fn transaction_rollback() {
    let mut db = make_test_db();
    parse_and_exec("BEGIN", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('rx', 'rollback_test', 99)", &mut db).unwrap();
    parse_and_exec("ROLLBACK", &mut db).unwrap();
    let t = db.get_table("items").unwrap();
    assert_eq!(t.rows.len(), 0, "rows should be 0 after rollback");
}

#[test]
fn transaction_commit() {
    let mut db = make_test_db();
    parse_and_exec("BEGIN", &mut db).unwrap();
    parse_and_exec("INSERT INTO items VALUES ('cx', 'commit_test', 99)", &mut db).unwrap();
    parse_and_exec("COMMIT", &mut db).unwrap();
    let t = db.get_table("items").unwrap();
    assert_eq!(t.rows.len(), 1, "rows should be 1 after commit");
}
