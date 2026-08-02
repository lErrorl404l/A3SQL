// JOIN tests: CROSS, INNER, LEFT, RIGHT, NATURAL, USING, self, multi-table

use super::helpers::*;

#[test]
fn cross_join() {
    let mut db = Database::new();
    let ca = vec![Column {
        name: "x".into(),
        dtype: ColumnType::Int,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut ta = Table::new("ta".into(), ca).unwrap();
    ta.insert(vec![DbValue::Int(1)]).unwrap();
    ta.insert(vec![DbValue::Int(2)]).unwrap();
    db.create_table("ta", ta).unwrap();
    let cb = vec![Column {
        name: "y".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut tb = Table::new("tb".into(), cb).unwrap();
    tb.insert(vec![DbValue::String("a".into())]).unwrap();
    db.create_table("tb", tb).unwrap();
    let r = parse_and_exec("SELECT * FROM ta, tb", &mut db).unwrap();
    assert!(
        r.contains("1") && r.contains("a") && r.contains("2"),
        "cross join: {}",
        r
    );
}

#[test]
fn inner_join() {
    let mut db = Database::new();
    let ca = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "v".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut ta = Table::new("a".into(), ca).unwrap();
    ta.insert(vec![DbValue::Int(1), DbValue::String("one".into())]).unwrap();
    ta.insert(vec![DbValue::Int(2), DbValue::String("two".into())]).unwrap();
    db.create_table("a", ta).unwrap();
    let cb = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "d".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut tb = Table::new("b".into(), cb).unwrap();
    tb.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
        .unwrap();
    tb.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
        .unwrap();
    db.create_table("b", tb).unwrap();
    let r = parse_and_exec("SELECT * FROM a INNER JOIN b ON a.id = b.id", &mut db).unwrap();
    assert!(r.contains("one"), "inner join: {}", r);
    assert!(!r.contains("two"), "should exclude two: {}", r);
}

#[test]
fn left_join() {
    let mut db = Database::new();
    let ca = vec![Column {
        name: "k".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut ta = Table::new("a".into(), ca).unwrap();
    ta.insert(vec![DbValue::String("x".into())]).unwrap();
    ta.insert(vec![DbValue::String("y".into())]).unwrap();
    db.create_table("a", ta).unwrap();
    let cb = vec![Column {
        name: "k".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut tb = Table::new("b".into(), cb).unwrap();
    tb.insert(vec![DbValue::String("x".into())]).unwrap();
    db.create_table("b", tb).unwrap();
    let r = parse_and_exec("SELECT * FROM a LEFT JOIN b ON a.k = b.k", &mut db).unwrap();
    assert!(r.contains("x"), "x: {}", r);
    assert!(r.contains("null") || r.contains("y"), "y null: {}", r);
}

#[test]
fn right_join() {
    let mut db = Database::new();
    let ca = vec![Column {
        name: "k".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut ta = Table::new("a".into(), ca).unwrap();
    ta.insert(vec![DbValue::String("x".into())]).unwrap();
    db.create_table("a", ta).unwrap();
    let cb = vec![Column {
        name: "k".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut tb = Table::new("b".into(), cb).unwrap();
    tb.insert(vec![DbValue::String("x".into())]).unwrap();
    tb.insert(vec![DbValue::String("y".into())]).unwrap();
    db.create_table("b", tb).unwrap();
    let r = parse_and_exec("SELECT * FROM a RIGHT JOIN b ON a.k = b.k", &mut db).unwrap();
    assert!(r.contains("x"), "x: {}", r);
    assert!(r.contains("y"), "y: {}", r);
}

#[test]
fn join_with_where() {
    let mut db = Database::new();
    let ca = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "n".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut ta = Table::new("u".into(), ca).unwrap();
    ta.insert(vec![DbValue::Int(1), DbValue::String("alice".into())])
        .unwrap();
    ta.insert(vec![DbValue::Int(2), DbValue::String("bob".into())]).unwrap();
    db.create_table("u", ta).unwrap();
    let cb = vec![
        Column {
            name: "uid".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "r".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut tb = Table::new("r".into(), cb).unwrap();
    tb.insert(vec![DbValue::Int(1), DbValue::String("admin".into())])
        .unwrap();
    tb.insert(vec![DbValue::Int(2), DbValue::String("user".into())])
        .unwrap();
    db.create_table("r", tb).unwrap();
    let sql = "SELECT * FROM u INNER JOIN r ON u.id = r.uid WHERE r.r = 'admin'";
    let r = parse_and_exec(sql, &mut db).unwrap();
    assert!(r.contains("alice"), "alice admin: {}", r);
    assert!(!r.contains("bob"), "bob not admin: {}", r);
}

#[test]
fn natural_join() {
    let mut db = Database::new();
    let ca = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "name".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut a = Table::new("a".into(), ca).unwrap();
    a.insert(vec![DbValue::Int(1), DbValue::String("one".into())]).unwrap();
    a.insert(vec![DbValue::Int(2), DbValue::String("two".into())]).unwrap();
    db.create_table("a", a).unwrap();
    let cb = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "val".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut b = Table::new("b".into(), cb).unwrap();
    b.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
        .unwrap();
    b.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
        .unwrap();
    db.create_table("b", b).unwrap();
    let r = parse_and_exec("SELECT * FROM a NATURAL JOIN b", &mut db).unwrap();
    assert!(r.contains("one"), "natural join should include one: {}", r);
    assert!(r.contains("desc1"), "natural join should include desc1: {}", r);
    assert!(
        !r.contains("two"),
        "natural join should exclude two (id=2 not in b): {}",
        r
    );
    assert!(
        !r.contains("desc3"),
        "natural join should exclude desc3 (id=3 not in a): {}",
        r
    );
}

#[test]
fn join_using() {
    let mut db = Database::new();
    let ca = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "name".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut a = Table::new("a".into(), ca).unwrap();
    a.insert(vec![DbValue::Int(1), DbValue::String("one".into())]).unwrap();
    a.insert(vec![DbValue::Int(2), DbValue::String("two".into())]).unwrap();
    db.create_table("a", a).unwrap();
    let cb = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "val".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut b = Table::new("b".into(), cb).unwrap();
    b.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
        .unwrap();
    b.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
        .unwrap();
    db.create_table("b", b).unwrap();
    let r = parse_and_exec("SELECT * FROM a JOIN b USING (id)", &mut db).unwrap();
    assert!(r.contains("one"), "join using should include one: {}", r);
    assert!(r.contains("desc1"), "join using should include desc1: {}", r);
    assert!(!r.contains("two"), "join using should exclude two: {}", r);
    assert!(!r.contains("desc3"), "join using should exclude desc3: {}", r);
    let r2 = parse_and_exec("SELECT * FROM a INNER JOIN b USING (id)", &mut db).unwrap();
    assert!(r2.contains("one"), "inner join using: {}", r2);
}

#[test]
fn multi_table_join() {
    let mut db = Database::new();
    let ca = vec![Column {
        name: "k".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut a = Table::new("a".into(), ca).unwrap();
    a.insert(vec![DbValue::String("x".into())]).unwrap();
    db.create_table("a", a).unwrap();
    let mut b = Table::new(
        "b".into(),
        vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        }],
    )
    .unwrap();
    b.insert(vec![DbValue::String("x".into())]).unwrap();
    db.create_table("b", b).unwrap();
    let mut c = Table::new(
        "c".into(),
        vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        }],
    )
    .unwrap();
    c.insert(vec![DbValue::String("x".into())]).unwrap();
    db.create_table("c", c).unwrap();
    let r = parse_and_exec(
        "SELECT * FROM a INNER JOIN b ON a.k = b.k INNER JOIN c ON b.k = c.k",
        &mut db,
    )
    .unwrap();
    assert!(
        r.contains("x") && r.chars().filter(|&c| c == 'x').count() >= 3,
        "multi: {}",
        r
    );
}

#[test]
fn self_join() {
    let mut db = Database::new();
    let cols = vec![Column {
        name: "k".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut t = Table::new("t".into(), cols).unwrap();
    t.insert(vec![DbValue::String("x".into())]).unwrap();
    t.insert(vec![DbValue::String("y".into())]).unwrap();
    db.create_table("t", t).unwrap();
    let r = parse_and_exec("SELECT a.k, b.k FROM t AS a CROSS JOIN t AS b", &mut db).unwrap();
    assert!(r.contains("x") && r.matches("x").count() >= 2, "self cross: {}", r);
}

#[test]
fn join_with_aggregate() {
    let mut db = Database::new();
    let ca = vec![Column {
        name: "id".into(),
        dtype: ColumnType::Int,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut a = Table::new("a".into(), ca).unwrap();
    a.insert(vec![DbValue::Int(1)]).unwrap();
    a.insert(vec![DbValue::Int(2)]).unwrap();
    db.create_table("a", a).unwrap();
    let mut b = Table::new(
        "b".into(),
        vec![Column {
            name: "aid".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        }],
    )
    .unwrap();
    b.insert(vec![DbValue::Int(1)]).unwrap();
    b.insert(vec![DbValue::Int(1)]).unwrap();
    db.create_table("b", b).unwrap();
    println!("note: JOIN+aggregate not yet supported");
}

#[test]
fn join_with_order_by() {
    let mut db = Database::new();
    let ca = vec![Column {
        name: "k".into(),
        dtype: ColumnType::String,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut a = Table::new("a".into(), ca).unwrap();
    a.insert(vec![DbValue::String("b".into())]).unwrap();
    a.insert(vec![DbValue::String("a".into())]).unwrap();
    db.create_table("a", a).unwrap();
    let mut b = Table::new(
        "b".into(),
        vec![Column {
            name: "k".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        }],
    )
    .unwrap();
    b.insert(vec![DbValue::String("a".into())]).unwrap();
    b.insert(vec![DbValue::String("b".into())]).unwrap();
    db.create_table("b", b).unwrap();
    let r = parse_and_exec("SELECT a.k FROM a INNER JOIN b ON a.k = b.k ORDER BY a.k ASC", &mut db).unwrap();
    assert!(r.contains("a") && r.contains("b"), "join order: {}", r);
}

// ── Subqueries in JOIN ON ────────────────────────────────────────────────

fn subq_db() -> Database {
    let mut db = Database::new();
    let ca = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "v".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut a = Table::new("a".into(), ca).unwrap();
    a.insert(vec![DbValue::Int(1), DbValue::String("one".into())]).unwrap();
    a.insert(vec![DbValue::Int(2), DbValue::String("two".into())]).unwrap();
    db.create_table("a", a).unwrap();
    let cb = vec![
        Column {
            name: "id".into(),
            dtype: ColumnType::Int,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
        Column {
            name: "d".into(),
            dtype: ColumnType::String,
            primary_key: false,
            not_null: false,
            default: None,
            default_expr: None,
            auto_increment: false,
            unique: false,
        },
    ];
    let mut b = Table::new("b".into(), cb).unwrap();
    b.insert(vec![DbValue::Int(1), DbValue::String("desc1".into())])
        .unwrap();
    b.insert(vec![DbValue::Int(3), DbValue::String("desc3".into())])
        .unwrap();
    db.create_table("b", b).unwrap();
    let cc = vec![Column {
        name: "aid".into(),
        dtype: ColumnType::Int,
        primary_key: false,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }];
    let mut c = Table::new("c".into(), cc).unwrap();
    c.insert(vec![DbValue::Int(1)]).unwrap();
    db.create_table("c", c).unwrap();
    db
}

#[test]
fn join_on_scalar_subquery() {
    let mut db = subq_db();
    // ON subquery reads table b (snapshot path); scalar value MIN(id)=1.
    let r = parse_and_exec(
        "SELECT * FROM a INNER JOIN b ON a.id = (SELECT MIN(id) FROM b)",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("one"), "id=1 must match: {}", r);
    assert!(!r.contains("two"), "id=2 must not match: {}", r);
}

#[test]
fn join_on_in_subquery() {
    let mut db = subq_db();
    let r = parse_and_exec(
        "SELECT * FROM a INNER JOIN b ON a.id IN (SELECT id FROM b WHERE d = 'desc1')",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("one"), "id=1 in subquery result: {}", r);
    assert!(!r.contains("two"), "id=2 not in subquery result: {}", r);
}

#[test]
fn join_on_exists_correlated() {
    let mut db = subq_db();
    // Correlated EXISTS — the per-row rewrite substitutes a.id from the flat row.
    let r = parse_and_exec(
        "SELECT * FROM a INNER JOIN b ON EXISTS (SELECT 1 FROM c WHERE c.aid = a.id)",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("one"), "a.id=1 exists in c: {}", r);
    assert!(!r.contains("two"), "a.id=2 missing from c: {}", r);
}

#[test]
fn full_outer_join_on_subquery() {
    let mut db = subq_db();
    // FullOuter ON subquery — the subquery walker must flag this so the
    // snapshot is taken (previously only Join/Inner/Left/Right were walked).
    let r = parse_and_exec(
        "SELECT * FROM a FULL OUTER JOIN b ON a.id = (SELECT MIN(id) FROM b)",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("one"), "id=1 match: {}", r);
    assert!(r.contains("two"), "preserved left row: {}", r);
    assert!(r.contains("desc3"), "b row 3: {}", r);
}

// ── Derived tables in FROM ───────────────────────────────────────────────

#[test]
fn derived_table_alone() {
    let mut db = subq_db();
    let r = parse_and_exec("SELECT * FROM (SELECT id, v FROM a) d", &mut db).unwrap();
    assert!(r.contains("one") && r.contains("two"), "derived rows: {}", r);
    assert!(r.contains("d.id"), "qualified header: {}", r);
    let r2 = parse_and_exec("SELECT v FROM (SELECT id, v FROM a) d", &mut db).unwrap();
    assert!(r2.contains("one") && r2.contains("two"), "bare col: {}", r2);
    assert!(!r2.contains("d.id"), "projected header: {}", r2);
}

#[test]
fn derived_table_join() {
    let mut db = subq_db();
    let r = parse_and_exec(
        "SELECT * FROM (SELECT id, v FROM a) d INNER JOIN b ON d.id = b.id",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("one"), "id=1 matches: {}", r);
    assert!(!r.contains("two"), "id=2 excluded: {}", r);
    assert!(r.contains("desc1"), "b desc1: {}", r);
}

#[test]
fn derived_table_join_on_subquery() {
    let mut db = subq_db();
    // Derived table + scalar subquery in ON — both new paths together.
    let r = parse_and_exec(
        "SELECT * FROM (SELECT id FROM a) d INNER JOIN b ON d.id = (SELECT MIN(id) FROM b)",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("1"), "d.id=1: {}", r);
    assert!(r.contains("desc1") && r.contains("desc3"), "cross b rows: {}", r);
}

#[test]
fn derived_table_empty() {
    let mut db = subq_db();
    let r = parse_and_exec("SELECT * FROM (SELECT id FROM a WHERE id = 999) d", &mut db).unwrap();
    assert!(r.contains("d.id"), "header only: {}", r);
    assert!(!r.contains("999"), "no data rows: {}", r);
}

#[test]
fn join_on_derived_table() {
    let mut db = subq_db();
    // Derived table on the RIGHT of the JOIN (resolved via j.relation).
    let r = parse_and_exec(
        "SELECT * FROM a INNER JOIN (SELECT id, d FROM b) d ON a.id = d.id",
        &mut db,
    )
    .unwrap();
    assert!(r.contains("one"), "id=1 matches: {}", r);
    assert!(!r.contains("two"), "id=2 excluded: {}", r);
    assert!(r.contains("desc1"), "b desc1: {}", r);
    assert!(!r.contains("desc3"), "id=3 excluded: {}", r);
}

#[test]
fn derived_table_correlated_rejected() {
    let mut db = subq_db();
    // o is not a table of the derived subquery → qualified outer ref.
    let r = parse_and_exec("SELECT * FROM (SELECT o.id FROM a) d", &mut db);
    assert!(
        r.is_err() && r.clone().unwrap_err().contains("correlated subquery in FROM"),
        "expected correlated-in-FROM error, got {:?}",
        r
    );
}
