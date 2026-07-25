use criterion::{criterion_group, criterion_main, Criterion};

// a3sql benchmark — query throughput
// Run with: cargo bench

static QUERIES: &[&str] = &[
    "CREATE TABLE bench (k STRING PRIMARY KEY, v INT, name STRING)",
    "INSERT INTO bench VALUES ('a', 10, 'alpha')",
    "INSERT INTO bench VALUES ('b', 20, 'beta')",
    "INSERT INTO bench VALUES ('c', 30, 'gamma')",
    "SELECT * FROM bench",
    "SELECT * FROM bench WHERE v > 15",
    "SELECT COUNT(*), SUM(v), AVG(v) FROM bench",
    "SELECT v > 15 AS high, COUNT(*) FROM bench GROUP BY high",
    "UPDATE bench SET v = 99 WHERE k = 'a'",
    "DELETE FROM bench WHERE k = 'c'",
];

fn bench_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispatch");
    group.sample_size(100);

    // Warmup — create tables
    for q in QUERIES {
        a3sql::dispatch(q, &[]);
    }

    group.bench_function("select_all", |b| b.iter(|| a3sql::dispatch("SELECT * FROM bench", &[])));

    group.bench_function("select_filtered", |b| {
        b.iter(|| a3sql::dispatch("SELECT * FROM bench WHERE v > 15", &[]))
    });

    group.bench_function("aggregate", |b| {
        b.iter(|| a3sql::dispatch("SELECT COUNT(*), SUM(v), AVG(v) FROM bench", &[]))
    });

    group.bench_function("group_by", |b| {
        b.iter(|| a3sql::dispatch("SELECT v > 15 AS high, COUNT(*) FROM bench GROUP BY high", &[]))
    });

    // Cleanup
    a3sql::dispatch("DROP TABLE bench", &[]);
}

criterion_group!(benches, bench_dispatch);
criterion_main!(benches);
