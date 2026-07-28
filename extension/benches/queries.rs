use criterion::{criterion_group, criterion_main, Criterion};

// a3sql benchmark — query throughput by operation type
// Run with: cargo bench
//
// Each group creates its own tables, warms up, benchmarks, then cleans up.

const ROW_COUNT: i64 = 1000;

fn setup_scan_table() {
    a3sql::dispatch(
        "CREATE TABLE bench_scan (k STRING PRIMARY KEY, v INT, name STRING)",
        &[],
    );
    for i in 0..ROW_COUNT {
        let sql = format!("INSERT INTO bench_scan VALUES ('k{}', {}, 'val{}')", i, i, i);
        a3sql::dispatch(&sql, &[]);
    }
}

fn bench_full_scan(c: &mut Criterion) {
    a3sql::dispatch("DROP TABLE IF EXISTS bench_scan", &[]);
    setup_scan_table();

    let mut group = c.benchmark_group("scan");
    group.sample_size(50);

    group.bench_function("select_all_1000", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_scan", &[]))
    });
    group.bench_function("select_where_int", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_scan WHERE v >= 500", &[]))
    });
    group.bench_function("select_where_string", |b| {
        b.iter(|| a3sql::dispatch("SELECT k FROM bench_scan WHERE name LIKE 'val%'", &[]))
    });
    group.bench_function("order_by_limit", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_scan ORDER BY v DESC LIMIT 10", &[]))
    });
    group.bench_function("order_by_offset", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_scan ORDER BY v ASC LIMIT 50 OFFSET 200", &[]))
    });

    group.finish();
    a3sql::dispatch("DROP TABLE bench_scan", &[]);
}

fn bench_aggregates(c: &mut Criterion) {
    a3sql::dispatch("CREATE TABLE bench_agg (k STRING PRIMARY KEY, grp STRING, v INT)", &[]);
    for i in 0..ROW_COUNT {
        let grp = if i % 3 == 0 {
            "a"
        } else if i % 3 == 1 {
            "b"
        } else {
            "c"
        };
        let sql = format!("INSERT INTO bench_agg VALUES ('k{}', '{}', {})", i, grp, i);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("aggregate");
    group.sample_size(50);
    group.bench_function("count_star", |b| {
        b.iter(|| a3sql::dispatch("SELECT COUNT(*) FROM bench_agg", &[]))
    });
    group.bench_function("sum_avg_min_max", |b| {
        b.iter(|| a3sql::dispatch("SELECT SUM(v), AVG(v), MIN(v), MAX(v) FROM bench_agg", &[]))
    });
    group.bench_function("group_by_count", |b| {
        b.iter(|| a3sql::dispatch("SELECT grp, COUNT(*) FROM bench_agg GROUP BY grp", &[]))
    });
    group.bench_function("group_by_sum", |b| {
        b.iter(|| a3sql::dispatch("SELECT grp, SUM(v) FROM bench_agg GROUP BY grp", &[]))
    });
    group.finish();
    a3sql::dispatch("DROP TABLE bench_agg", &[]);
}

fn bench_joins(c: &mut Criterion) {
    a3sql::dispatch("CREATE TABLE bench_jl (k STRING PRIMARY KEY, val INT)", &[]);
    a3sql::dispatch("CREATE TABLE bench_jr (k STRING PRIMARY KEY, label STRING)", &[]);
    for i in 0..200i64 {
        let sql = format!("INSERT INTO bench_jl VALUES ('k{}', {})", i, i);
        a3sql::dispatch(&sql, &[]);
        let label = format!("L{}", i);
        let sql = format!("INSERT INTO bench_jr VALUES ('k{}', '{}')", i, label);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("join");
    group.sample_size(50);
    group.bench_function("inner_join", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "SELECT * FROM bench_jl INNER JOIN bench_jr ON bench_jl.k = bench_jr.k",
                &[],
            )
        })
    });
    group.bench_function("left_join", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "SELECT * FROM bench_jl LEFT JOIN bench_jr ON bench_jl.k = bench_jr.k",
                &[],
            )
        })
    });
    group.bench_function("join_with_filter", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "SELECT * FROM bench_jl INNER JOIN bench_jr ON bench_jl.k = bench_jr.k WHERE bench_jl.val > 100",
                &[],
            )
        })
    });
    group.finish();
    a3sql::dispatch("DROP TABLE bench_jl", &[]);
    a3sql::dispatch("DROP TABLE bench_jr", &[]);
}

fn bench_index(c: &mut Criterion) {
    a3sql::dispatch("CREATE TABLE bench_idx (k STRING PRIMARY KEY, v INT, tag STRING)", &[]);
    a3sql::dispatch("CREATE INDEX bench_idx_v ON bench_idx (v)", &[]);
    for i in 0..ROW_COUNT {
        let tag = format!("tag_{}", i % 50);
        let sql = format!("INSERT INTO bench_idx VALUES ('k{}', {}, '{}')", i, i, tag);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("index");
    group.sample_size(50);
    group.bench_function("btree_equality", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_idx WHERE v = 42", &[]))
    });
    group.bench_function("btree_equality_first", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_idx WHERE v = 0", &[]))
    });
    group.bench_function("btree_equality_last", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_idx WHERE v = 999", &[]))
    });
    group.finish();
    a3sql::dispatch("DROP TABLE bench_idx", &[]);
}

fn bench_dml(c: &mut Criterion) {
    a3sql::dispatch("CREATE TABLE bench_dml (k STRING PRIMARY KEY, v INT, name STRING)", &[]);
    for i in 0..100i64 {
        let sql = format!("INSERT INTO bench_dml VALUES ('k{}', {}, 'init{}')", i, i, i);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("dml");
    group.sample_size(50);
    group.bench_function("update_all", |b| {
        b.iter(|| a3sql::dispatch("UPDATE bench_dml SET v = v + 1", &[]))
    });
    group.bench_function("update_where", |b| {
        b.iter(|| a3sql::dispatch("UPDATE bench_dml SET name = 'x' WHERE v > 50", &[]))
    });
    group.bench_function("delete_where", |b| {
        // Re-insert deleted rows for next iteration
        b.iter(|| {
            a3sql::dispatch("DELETE FROM bench_dml WHERE v < 10", &[]);
            for j in 0..10i64 {
                let sql = format!("INSERT INTO bench_dml VALUES ('rk{}', {}, 'reinsert{}')", j, j, j);
                a3sql::dispatch(&sql, &[]);
            }
        })
    });

    // Full-table INSERT scan
    group.bench_function("insert_multi", |b| {
        b.iter(|| {
            for j in 0..100i64 {
                let sql = format!("INSERT INTO bench_dml VALUES ('ik{}', {}, 'insert{}')", j, j, j);
                a3sql::dispatch(&sql, &[]);
            }
        })
    });
    group.finish();
    a3sql::dispatch("DROP TABLE bench_dml", &[]);
}

fn bench_fuzzy(c: &mut Criterion) {
    a3sql::dispatch("CREATE TABLE bench_fuzz (k STRING PRIMARY KEY, val STRING)", &[]);
    a3sql::dispatch("CREATE INDEX bench_fuzz_val ON bench_fuzz (val)", &[]);
    let items: Vec<String> = (0..100).map(|i| format!("rhs_m4a{}_scope_{}", i % 10, i)).collect();
    let items2: Vec<String> = (0..100)
        .map(|i| format!("hlc_rifle_m4{}_carry_{}", i % 10, i))
        .collect();
    for item in items.iter().chain(items2.iter()) {
        let sql = format!("INSERT INTO bench_fuzz VALUES ('{}', '{}')", item, item);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("fuzzy");
    group.sample_size(50);
    group.bench_function("trigram_fuzzy_lookup", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_fuzz WHERE val %% 'rhs_m4'", &[]))
    });
    group.bench_function("trigram_fuzzy_all", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench_fuzz WHERE val %% 'm4'", &[]))
    });
    group.finish();
    a3sql::dispatch("DROP TABLE bench_fuzz", &[]);
}

fn bench_cte(c: &mut Criterion) {
    a3sql::dispatch(
        "CREATE TABLE bench_cte (k STRING PRIMARY KEY, parent STRING, val INT)",
        &[],
    );
    // Create a simple parent-child hierarchy
    a3sql::dispatch("INSERT INTO bench_cte VALUES ('root', NULL, 0)", &[]);
    for i in 0..50i64 {
        let parent = if i < 10 { "root" } else { &format!("child_{}", i % 10) };
        let sql = format!("INSERT INTO bench_cte VALUES ('child_{}', '{}', {})", i, parent, i);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("cte");
    group.sample_size(50);
    group.bench_function("recursive_cte", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "WITH RECURSIVE tree AS ( \
                 SELECT k, parent, val FROM bench_cte WHERE k = 'root' \
                 UNION ALL \
                 SELECT c.k, c.parent, c.val FROM bench_cte c JOIN tree t ON c.parent = t.k \
                 ) SELECT * FROM tree",
                &[],
            )
        })
    });
    group.bench_function("non_recursive_cte", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "WITH high AS (SELECT * FROM bench_cte WHERE val > 25) SELECT COUNT(*) FROM high",
                &[],
            )
        })
    });
    group.finish();
    a3sql::dispatch("DROP TABLE bench_cte", &[]);
}

fn bench_window(c: &mut Criterion) {
    a3sql::dispatch(
        "CREATE TABLE bench_win (k STRING PRIMARY KEY, grp STRING, val INT)",
        &[],
    );
    for i in 0..100i64 {
        let grp = if i % 4 == 0 {
            "a"
        } else if i % 4 == 1 {
            "b"
        } else {
            "c"
        };
        let sql = format!("INSERT INTO bench_win VALUES ('k{}', '{}', {})", i, grp, i);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("window");
    group.sample_size(50);
    group.bench_function("row_number", |b| {
        b.iter(|| a3sql::dispatch("SELECT k, ROW_NUMBER() OVER (ORDER BY val) FROM bench_win", &[]))
    });
    group.bench_function("rank", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "SELECT k, RANK() OVER (PARTITION BY grp ORDER BY val) FROM bench_win",
                &[],
            )
        })
    });
    group.bench_function("sum_over", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "SELECT k, val, SUM(val) OVER (PARTITION BY grp ORDER BY val ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS running FROM bench_win",
                &[],
            )
        })
    });
    group.finish();
    a3sql::dispatch("DROP TABLE bench_win", &[]);
}

fn bench_patch_rules(c: &mut Criterion) {
    let create = "CREATE TABLE bench_patch (id INTEGER PRIMARY KEY, name STRING, active INT, priority INT, match_type STRING, match_value STRING, target_type STRING, property STRING, operator STRING, value STRING)";
    let select_active = "SELECT * FROM bench_patch WHERE active=1 ORDER BY priority LIMIT 100";

    a3sql::dispatch("DROP TABLE IF EXISTS bench_patch", &[]);
    a3sql::dispatch(create, &[]);
    for i in 0..1000i64 {
        let sql = format!("INSERT INTO bench_patch VALUES ({}, 'rule_{}', 1, {}, 'exact', 'M4A1', 'weapon', 'reloadTime', 'set', '{}')", i, i, i % 10, i);
        a3sql::dispatch(&sql, &[]);
    }

    let mut group = c.benchmark_group("patch");
    group.sample_size(50);

    group.bench_function("select_active_1000", |b| b.iter(|| a3sql::dispatch(select_active, &[])));
    group.bench_function("select_by_target_type", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "SELECT * FROM bench_patch WHERE target_type='weapon' AND active=1 ORDER BY priority LIMIT 50",
                &[],
            )
        })
    });
    group.bench_function("count_by_type", |b| {
        b.iter(|| {
            a3sql::dispatch(
                "SELECT target_type, COUNT(*) FROM bench_patch WHERE active=1 GROUP BY target_type",
                &[],
            )
        })
    });
    group.bench_function("insert_single", |b| {
        let mut i = 1000i64;
        b.iter(|| {
            let sql = format!("INSERT INTO bench_patch VALUES ({}, 'rule_{}', 1, 0, 'exact', 'target', 'weapon', 'prop', 'set', 'val')", i, i);
            a3sql::dispatch(&sql, &[]);
            i += 1;
        })
    });
    group.bench_function("insert_batch_50", |b| {
        b.iter(|| {
            let vals: Vec<String> = (0..50i64)
                .map(|j| format!("({}, 'batch_{}', 1, 0, 'exact', 't', 'w', 'p', 'set', 'v')", j, j))
                .collect();
            a3sql::dispatch(&format!("INSERT INTO bench_patch VALUES {}", vals.join(",")), &[]);
        })
    });
    group.bench_function("update_activate_all", |b| {
        b.iter(|| a3sql::dispatch("UPDATE bench_patch SET active=1", &[]))
    });
    group.bench_function("delete_all", |b| {
        b.iter(|| {
            a3sql::dispatch("DELETE FROM bench_patch", &[]);
            for i in 0..1000i64 {
                let sql = format!("INSERT INTO bench_patch VALUES ({}, 'rule_{}', 1, {}, 'exact', 'M4A1', 'weapon', 'reloadTime', 'set', '{}')", i, i, i % 10, i);
                a3sql::dispatch(&sql, &[]);
            }
        })
    });

    group.finish();
    a3sql::dispatch("DROP TABLE bench_patch", &[]);
}

criterion_group!(
    benches,
    bench_full_scan,
    bench_aggregates,
    bench_joins,
    bench_index,
    bench_dml,
    bench_fuzzy,
    bench_cte,
    bench_window,
    bench_patch_rules,
);
criterion_main!(benches);
