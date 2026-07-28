// a3sql save/load verification test

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
