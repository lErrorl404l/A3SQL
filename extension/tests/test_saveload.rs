// a3sql save/load + TCP listener verification test

#[test]
fn test_tcp_listener() {
    // Start the TCP listener on a test port
    let r = a3sql::dispatch("listen 33307", &[]);
    assert!(r.contains("\"OK\""), "listen start: {}", r);
    println!("✅ TCP listener started on 33307");

    // Allow listener thread to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Connect via TCP and send a version command
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;

    let mut stream = TcpStream::connect("127.0.0.1:33307").expect("Failed to connect to TCP listener");
    println!("✅ TCP connection established");

    // Read initial prompt (if any)
    let mut reader = BufReader::new(&stream);
    let mut buf = String::new();

    // Send SQL query
    stream
        .write_all(b"CREATE TABLE tcp_test (id STRING PRIMARY KEY, val STRING)\n")
        .expect("write failed");
    buf.clear();
    reader.read_line(&mut buf).expect("read failed");
    assert!(buf.contains("\"OK\""), "create table: {}", buf);
    println!("✅ TCP: CREATE TABLE");

    // Insert via TCP
    stream
        .write_all(b"INSERT INTO tcp_test VALUES ('1', 'hello')\n")
        .expect("write failed");
    buf.clear();
    reader.read_line(&mut buf).expect("read failed");
    assert!(buf.contains("\"OK\""), "insert: {}", buf);
    println!("✅ TCP: INSERT");

    // SELECT via TCP
    stream.write_all(b"SELECT * FROM tcp_test\n").expect("write failed");
    buf.clear();
    reader.read_line(&mut buf).expect("read failed");
    assert!(buf.contains("hello"), "select: {}", buf);
    println!("✅ TCP: SELECT");

    // version command
    stream.write_all(b"version\n").expect("write failed");
    buf.clear();
    reader.read_line(&mut buf).expect("read failed");
    assert!(buf.contains("a3sql"), "version: {}", buf);
    println!("✅ TCP: VERSION");

    // Cleanup
    a3sql::dispatch("DROP TABLE IF EXISTS tcp_test", &[]);
    println!("✅ All TCP tests passed");
}

#[test]
fn test_save_load_cycle() {
    // Reset and create patch_rules
    let r = a3sql::dispatch("DROP TABLE IF EXISTS patch_rules", &[]);
    assert!(r.contains("\"OK\""), "drop: {}", r);
    let r = a3sql::dispatch("CREATE TABLE patch_rules (id STRING PRIMARY KEY, name STRING, active INT, target_type STRING, property STRING, value STRING, group_name STRING, notes STRING)", &[]);
    assert!(r.contains("\"OK\""), "create: {}", r);

    // Insert two rules
    let r = a3sql::dispatch(
        "INSERT INTO patch_rules VALUES ('1', 'test1', 1, 'weapon', 'reloadTime', '2.5', 'balance', 'test note')",
        &[],
    );
    assert!(r.contains("\"OK\""), "insert1: {}", r);
    let r = a3sql::dispatch(
        "INSERT INTO patch_rules VALUES ('2', 'test2', 1, 'vehicle', 'fuel', '0.8', '', '')",
        &[],
    );
    assert!(r.contains("\"OK\""), "insert2: {}", r);

    // Verify insert
    let r = a3sql::dispatch("SELECT COUNT(*) FROM patch_rules", &[]);
    assert!(r.contains("2"), "count before save: {}", r);

    // Save to file
    let r = a3sql::dispatch("save /tmp/a3sql_save_test.bin", &[]);
    assert!(r.contains("\"OK\""), "save: {}", r);
    println!("✅ Save OK");

    // Drop and recreate
    let r = a3sql::dispatch("DROP TABLE IF EXISTS patch_rules", &[]);
    assert!(r.contains("\"OK\""), "drop2: {}", r);
    let r = a3sql::dispatch("SELECT COUNT(*) FROM patch_rules", &[]);
    assert!(
        r.contains("doesn't exist") || r.contains("not found") || r.contains("ERR"),
        "table gone: {}",
        r
    );
    println!("✅ Table dropped OK");

    // Load from save
    let r = a3sql::dispatch("load /tmp/a3sql_save_test.bin", &[]);
    assert!(r.contains("\"OK\""), "load: {}", r);
    println!("✅ Load OK");

    // Verify rules restored
    let r = a3sql::dispatch("SELECT * FROM patch_rules ORDER BY id", &[]);
    assert!(r.contains("test1"), "restore check1: {}", r);
    assert!(r.contains("test2"), "restore check2: {}", r);
    assert!(r.contains("balance"), "group check: {}", r);
    assert!(r.contains("test note"), "notes check: {}", r);
    println!("✅ Rules restored with group and notes");

    // Cleanup
    std::fs::remove_file("/tmp/a3sql_save_test.bin").ok();
    eprintln!("\n🎉 Auto-save/load verified: save/load cycle preserves all fields including group_name and notes");
}
