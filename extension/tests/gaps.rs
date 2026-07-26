// Spot-check remaining doc claims vs actual engine behavior
use a3sql::dispatch;
use std::sync::{Mutex, MutexGuard};

static TEST_MUTEX: Mutex<()> = Mutex::new(());
fn ok(sql: &str, label: &str) {
    let r = dispatch(sql, &[]);
    assert!(r.contains("[0,"), "FAIL {}: {}", label, r);
}
fn setup() -> MutexGuard<'static, ()> {
    let g = TEST_MUTEX.lock().unwrap();
    dispatch("reset", &[]);
    g
}

#[test]
fn gap_having() {
    let _g = setup();
    dispatch("CREATE TABLE a_h (id STRING PRIMARY KEY, cat STRING, v INT)", &[]);
    dispatch(
        "INSERT INTO a_h VALUES ('a', 'x', 10), ('b', 'x', 20), ('c', 'y', 30)",
        &[],
    );
    ok(
        "SELECT cat, COUNT(*) FROM a_h GROUP BY cat HAVING COUNT(*) > 1",
        "HAVING",
    );
}
#[test]
fn gap_dense_rank() {
    let _g = setup();
    dispatch("CREATE TABLE a_dr (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO a_dr VALUES ('a', 1), ('b', 2), ('c', 2), ('d', 3)", &[]);
    ok(
        "SELECT id, DENSE_RANK() OVER (ORDER BY v) AS dr FROM a_dr",
        "DENSE_RANK",
    );
}
#[test]
fn gap_date_timestamp_types() {
    let _g = setup();
    ok(
        "CREATE TABLE a_dt (id STRING PRIMARY KEY, d DATE, ts TIMESTAMP)",
        "DATE/TIMESTAMP types",
    );
}
#[test]
fn gap_insert_ignore() {
    let _g = setup();
    dispatch("CREATE TABLE a_ii (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO a_ii VALUES ('a', 1)", &[]);
    ok("INSERT IGNORE INTO a_ii VALUES ('a', 999)", "INSERT IGNORE");
}
#[test]
fn gap_on_conflict_do_nothing() {
    let _g = setup();
    dispatch("CREATE TABLE a_cdn (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO a_cdn VALUES ('a', 1)", &[]);
    ok(
        "INSERT INTO a_cdn VALUES ('a', 999) ON CONFLICT (id) DO NOTHING",
        "ON CONFLICT DO NOTHING",
    );
}
#[test]
fn gap_select_into() {
    let _g = setup();
    dispatch("CREATE TABLE a_si_src (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("INSERT INTO a_si_src VALUES ('a', 10), ('b', 20)", &[]);
    let r = dispatch("SELECT * INTO a_si_dst FROM a_si_src", &[]);
    assert!(r.contains("[0,"), "SELECT INTO: {}", r);
    let r2 = dispatch("SELECT COUNT(*) FROM a_si_src", &[]);
    assert!(r2.contains("2"), "source count: {}", r2);
}
#[test]
fn gap_concat_operator() {
    let _g = setup();
    dispatch("CREATE TABLE a_co (id STRING PRIMARY KEY, a STRING, b STRING)", &[]);
    dispatch("INSERT INTO a_co VALUES ('x', 'hello', 'world')", &[]);
    let r = dispatch("SELECT a || ' ' || b AS combined FROM a_co WHERE id = 'x'", &[]);
    assert!(r.contains("hello world"), "CONCAT operator: {}", r);
}
#[test]
fn gap_on_update_set_null() {
    let _g = setup();
    dispatch("CREATE TABLE a_oup (id STRING PRIMARY KEY)", &[]);
    dispatch("INSERT INTO a_oup VALUES ('p1')", &[]);
    ok(
        "CREATE TABLE a_ouc (id STRING PRIMARY KEY, pid STRING REFERENCES a_oup(id) ON UPDATE SET NULL)",
        "ON UPDATE SET NULL",
    );
}
#[test]
fn gap_if_not_exists() {
    let _g = setup();
    ok(
        "CREATE TABLE IF NOT EXISTS a_ine (id STRING PRIMARY KEY)",
        "IF NOT EXISTS",
    );
    ok(
        "CREATE TABLE IF NOT EXISTS a_ine (id STRING PRIMARY KEY)",
        "IF NOT EXISTS idempotent",
    );
}
#[test]
fn gap_before_trigger() {
    let _g = setup();
    dispatch("CREATE TABLE a_bt_main (id STRING PRIMARY KEY, v INT)", &[]);
    dispatch("CREATE TABLE a_bt_log (msg STRING)", &[]);
    ok(
        "CREATE TRIGGER a_bt_ins BEFORE INSERT ON a_bt_main BEGIN INSERT INTO a_bt_log VALUES ('before') END",
        "BEFORE INSERT",
    );
    dispatch("INSERT INTO a_bt_main VALUES ('a', 10)", &[]);
    let r = dispatch("SELECT * FROM a_bt_log", &[]);
    assert!(r.contains("before"), "BEFORE trigger fired: {}", r);
}
#[test]
fn gap_raise_abort() {
    let _g = setup();
    dispatch("CREATE TABLE a_ra_main (id STRING PRIMARY KEY, v INT)", &[]);
    ok(
        "CREATE TRIGGER a_ra_chk BEFORE INSERT ON a_ra_main BEGIN SELECT RAISE(ABORT, 'no inserts') END",
        "CREATE RAISE trigger",
    );
    let r = dispatch("INSERT INTO a_ra_main VALUES ('x', 1)", &[]);
    assert!(!r.contains("[0,"), "RAISE should abort insert: {}", r);
}
#[test]
fn gap_tcp_login() {
    let _g = setup();
    use std::io::{BufRead, Write};
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    dispatch("set_credentials", &["admin", "secret"]);
    let addr = format!("127.0.0.1:{}", port);
    std::thread::spawn(move || {
        let _ = a3sql::start_server("127.0.0.1", port, None);
    });
    std::thread::sleep(std::time::Duration::from_millis(400));
    if let Ok(mut stream) = std::net::TcpStream::connect(&addr) {
        writeln!(stream, "SELECT 1").ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut resp = String::new();
        std::io::BufReader::new(&mut stream).read_line(&mut resp).ok();
        assert!(resp.contains("ERR_AUTH"), "Need LOGIN: {}", resp);
    }
    if let Ok(mut stream) = std::net::TcpStream::connect(&addr) {
        writeln!(stream, "LOGIN admin wrong").ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut resp = String::new();
        std::io::BufReader::new(&mut stream).read_line(&mut resp).ok();
        assert!(resp.contains("Invalid"), "Wrong pwd: {}", resp);
    }
    if let Ok(mut stream) = std::net::TcpStream::connect(&addr) {
        writeln!(stream, "LOGIN admin secret").ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut resp = String::new();
        std::io::BufReader::new(&mut stream).read_line(&mut resp).ok();
        assert!(resp.contains("Authenticated"), "Login: {}", resp);
        writeln!(stream, "SELECT 1 AS n").ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut resp2 = String::new();
        std::io::BufReader::new(&mut stream).read_line(&mut resp2).ok();
        assert!(resp2.contains("[0,"), "Query: {}", resp2);
    }
}
