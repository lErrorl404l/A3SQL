// a3sql — proptest-based fuzz harness for the SQL engine
//
// cargo-fuzz is not installed in this environment, so this uses proptest
// (already a dev-dependency) to feed random and mutated SQL through the full
// parse → execute path and assert the engine NEVER panics, hangs, or violates
// envelope invariants — always returning a clean error or a valid result.
//
// Isolation: each iteration uses a FRESH `Database::new()` via the
// `dispatch_inner` API (the same code path the public `dispatch` wraps, minus
// the global-DB lock), so fuzz runs never mutate the global `ffi::DB` state.
//
// Invariants asserted per input:
//   1. `dispatch_inner` never panics (checked via `catch_unwind`).
//   2. The response is a well-formed envelope: non-empty, starts with `[`,
//      and parses as a JSON array whose status word is "OK" or "ERR_*".
//   3. A CREATE → INSERT → SELECT sequence on a fuzzed-but-valid schema keeps
//      the DB consistent: SELECT COUNT(*) reflects the rows inserted.
//   4. Weird-but-valid inputs (empty string, NUL bytes, unicode, control
//      chars, very long strings, whitespace, comments, unterminated quotes,
//      backticks, reserved keywords as identifiers) never crash.

#![cfg(test)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::dispatch::dispatch_inner;
use crate::engine::Database;

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

// ── Seed corpus — real query shapes from the deep harnesses + smoke tests ──

const CORPUS: &[&str] = &[
    // deep_armaos / deep_arsenal / deep_cba / deep_lambs / deep_tsp
    "CREATE TABLE users (uid INT PRIMARY KEY, name STRING, role STRING, home STRING)",
    "CREATE TABLE files (path STRING PRIMARY KEY, owner INT, size INT, mtime INT)",
    "CREATE TABLE calendar (date STRING, event STRING, PRIMARY KEY (date, event))",
    "CREATE TABLE processes (pid INT PRIMARY KEY, name STRING, user INT, cpu REAL, mem INT)",
    "CREATE TABLE journal (id INT PRIMARY KEY, ts INT, action STRING, detail STRING)",
    "CREATE TABLE loadouts (id STRING PRIMARY KEY, class_name STRING, display_name STRING, items STRING, weight REAL, updated INT)",
    "CREATE TABLE settings (ns TEXT, setting_key TEXT, value TEXT, priority INT, PRIMARY KEY (ns, setting_key))",
    "CREATE TABLE lambs_settings (module TEXT, setting TEXT, value REAL, PRIMARY KEY (module, setting))",
    "CREATE TABLE chvd (k STRING PRIMARY KEY, v TEXT)",
    "INSERT INTO users VALUES (0,'root','admin','/root'),(1,'matt','user','/home/matt'),(2,'sgt','operator','/home/sgt'),(3,'smith','user','/home/smith')",
    "INSERT INTO journal VALUES (200, 20000, 'op_0', 'x'), (201, 20060, 'op_1', 'y')",
    "INSERT INTO killhouse_scores VALUES ('smith', 'mout', 60.1, 33, 24)",
    "INSERT OR REPLACE INTO settings VALUES ('client', 'cba_setting_5', '5', 99)",
    "INSERT OR REPLACE INTO lambs_settings VALUES ('danger', 'param_5', 1.5)",
    "INSERT INTO loadouts VALUES ('L6', 'B_Recon_F', 'Recon', '[\"arifle_MX_F\"]', 41.0, 600)",
    "UPDATE loadouts SET display_name='Renamed', weight=40.0 WHERE id='L3'",
    "UPDATE settings SET value='NEW', priority=9 WHERE ns='' AND setting_key='cba_setting_7'",
    "DELETE FROM loadouts WHERE id='L5'",
    "DELETE FROM settings WHERE ns='server'",
    "SELECT name, role FROM users WHERE uid=2",
    "SELECT home FROM users WHERE name='matt'",
    "SELECT COUNT(*) FROM files WHERE owner=1",
    "SELECT COUNT(*) FROM files WHERE size > 1049",
    "SELECT size FROM files WHERE path='/usr/share/app42.bin'",
    "SELECT COUNT(*) FROM files WHERE path LIKE '/usr/share/%'",
    "SELECT event FROM calendar WHERE date='2026-06-01'",
    "SELECT COUNT(*) FROM calendar WHERE event LIKE '%x%'",
    "SELECT pid FROM processes WHERE name='proc_7'",
    "SELECT COUNT(*) FROM processes WHERE cpu > 30.0",
    "SELECT SUM(mem) FROM processes",
    "SELECT COUNT(*) FROM journal WHERE ts > 10000",
    "SELECT COUNT(*) FROM journal WHERE action='op_2'",
    "SELECT u.name, COUNT(f.path) FROM users u LEFT JOIN files f ON f.owner=u.uid GROUP BY u.name ORDER BY u.name",
    "SELECT u.role, COUNT(*) FROM processes p JOIN users u ON u.uid=p.user GROUP BY u.role ORDER BY u.role",
    "SELECT u.name, SUM(f.size) FROM users u JOIN files f ON f.owner=u.uid GROUP BY u.name ORDER BY u.name",
    "SELECT path FROM files WHERE owner = (SELECT uid FROM users WHERE name='sgt') LIMIT 1",
    "SELECT COUNT(*) FROM files WHERE owner IN (SELECT uid FROM users WHERE role='user')",
    "SELECT event FROM calendar WHERE date='2026-01-01' AND event='new_year'",
    "SELECT display_name FROM loadouts ORDER BY updated",
    "SELECT items FROM loadouts WHERE id='L4'",
    "SELECT id FROM loadouts WHERE weight < 45.0 ORDER BY id",
    "SELECT v FROM chvd WHERE k='CHVD_maxView'",
    "SELECT v FROM legacy_store WHERE k = 'saved_loadouts'",
    "SELECT COUNT(*) FROM settings WHERE ns='' AND setting_key LIKE 'cba_setting_4%'",
    "SELECT COUNT(*) FROM settings WHERE priority > 40 AND ns=''",
    "SELECT MAX(value) FROM lambs_settings WHERE module='wp'",
    "SELECT MIN(time_s) FROM killhouse_scores WHERE player='matt'",
    "SELECT course, MIN(time_s) FROM killhouse_scores GROUP BY course ORDER BY course",
    "SELECT setting FROM tsp_settings WHERE kind='CHECKBOX'",
    "SELECT setting, value FROM tsp_settings WHERE kind='SLIDER' AND module='tsp_cba_core_chvd'",
    "SELECT 1",
    "SELECT 1 FROM server_commands LIMIT 1",
    "SELECT * FROM patch_rules WHERE active = 1 ORDER BY priority DESC, id ASC LIMIT 50 OFFSET 0",
    "SELECT count(*) as c FROM server_commands WHERE status = 'pending'",
    "SELECT rank, COUNT(*) AS count FROM players GROUP BY rank ORDER BY count DESC",
    "INSERT INTO session_stats (player_name, kills, deaths, assists, score) VALUES ('PlayerOne', 0, 0, 0, 0)",
    "ALTER TABLE patch_rules ADD COLUMN group_name TEXT DEFAULT ''",
    "INSERT INTO patch_presets (name, data) VALUES ('preset1', '[[\"fixAmmo\",10,1]]')",
    "UPDATE patch_presets SET data = '[[\"fixAmmo\",20,0]]' WHERE name = 'preset1'",
];

/// Weird-but-valid inputs that must never crash (invariant 4).
const WEIRD_INPUTS: &[&str] = &[
    "",
    " ",
    "   ",
    "\t\n",
    "\u{0}",
    "SELECT 1;",
    ";",
    ";;",
    "-- comment",
    "/* block */",
    "SELECT 'unterminated",
    "SELECT `backtick`",
    "SELECT \"double\"",
    "SELECT 'a' 'b'",
    "SELECT 1; DROP TABLE t;",
    "SELECT 0x00",
    "SELECT '\u{0}'",
    "SELECT 'é\u{1f600}中'",
    "SELECT '\u{7f}'",
    "SELECT 1\t2",
    "SELECT select FROM from",
    "CREATE TABLE select (x INT PRIMARY KEY)",
    "SELECT * FROM t WHERE a = 1 AND b = 2",
    "SELECT 'long' || 'string'",
];

/// Keywords likely to appear in engine code paths (used by the token soup).
const KEYWORDS: &[&str] = &[
    "SELECT",
    "INSERT",
    "INTO",
    "VALUES",
    "CREATE",
    "TABLE",
    "DROP",
    "IF",
    "EXISTS",
    "FROM",
    "WHERE",
    "UPDATE",
    "SET",
    "DELETE",
    "ALTER",
    "ADD",
    "COLUMN",
    "PRIMARY",
    "KEY",
    "INT",
    "INTEGER",
    "STRING",
    "TEXT",
    "REAL",
    "FLOAT",
    "BOOLEAN",
    "NULL",
    "NOT",
    "AND",
    "OR",
    "ORDER",
    "BY",
    "GROUP",
    "LIMIT",
    "OFFSET",
    "JOIN",
    "LEFT",
    "RIGHT",
    "INNER",
    "ON",
    "AS",
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "LIKE",
    "BETWEEN",
    "IN",
    "IS",
    "DISTINCT",
    "ALL",
    "UNION",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "CAST",
    "DEFAULT",
    "UNIQUE",
    "AUTOINCREMENT",
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "RELEASE",
    "TO",
    "WITH",
    "EXPLAIN",
    "VACUUM",
    "REINDEX",
    "OR",
    "REPLACE",
    "CASCADE",
    "DESC",
    "ASC",
];

/// Custom commands handled by dispatch BEFORE SQL parsing. Fuzzed inputs that
/// accidentally match these would trigger side effects (TCP listener threads,
/// network connects, file I/O) — skip them so the fuzzer stays on the SQL path.
fn is_custom_command(input: &str) -> bool {
    let t = input.trim().to_lowercase();
    t.starts_with("ping")
        || t.starts_with("reset")
        || t.starts_with("version")
        || t.starts_with("save")
        || t.starts_with("load")
        || t.starts_with("export")
        || t.starts_with("import")
        || t.starts_with("listen")
        || t.starts_with("connect")
        || t.starts_with("plugin_dir")
        || t.starts_with("register_function")
        || t.starts_with("set_credentials")
        || t.starts_with("live_patch")
        || t.starts_with("cursor")
        || t.starts_with("prepare")
        || t.starts_with("reindex")
        || t.starts_with("stop")
        || t.starts_with("dump_sql")
        || t.starts_with("describe")
        || t.starts_with("show")
        || t.starts_with("plugins")
        || t.starts_with("disconnect")
}

// ── Strategies ─────────────────────────────────────────────────────────────

/// Random SQL-ish token soup: keywords, identifiers, numbers, quoted strings,
/// operators, whitespace, comments, backticks, NUL/unicode/control bytes.
fn sqlish() -> impl Strategy<Value = String> {
    let keyword = prop::sample::select(KEYWORDS).prop_map(|s: &str| s.to_string());
    let ident =
        prop::collection::vec(prop::char::range('a', 'z'), 0..16).prop_map(|v| v.into_iter().collect::<String>());
    let number = prop::num::i64::ANY.prop_map(|n| n.to_string());
    let float = prop::num::f64::ANY.prop_map(|n| n.to_string());
    let quoted = (0..4u32, prop::collection::vec(any::<char>(), 0..8)).prop_map(|(q, v)| {
        let body: String = v.into_iter().collect();
        match q {
            0 => format!("'{}'", body),
            1 => format!("\"{}\"", body),
            2 => format!("`{}`", body),
            _ => format!("'{}", body), // unterminated
        }
    });
    let op = prop::sample::select(&[
        "=",
        "==",
        "!=",
        "<>",
        "<",
        ">",
        "<=",
        ">=",
        "+",
        "-",
        "*",
        "/",
        "%",
        "||",
        "&",
        "|",
        "~",
        "!",
        "(",
        ")",
        ",",
        ";",
        ".",
        "[",
        "]",
        "{",
        "}",
        "?",
        ":",
        "--",
        "/*",
        "*/",
        "#",
        "\\",
        "\u{0}",
        "\u{1}",
        "\u{7f}",
        "\u{4f60}",
        "\u{1f600}",
        "\n",
        "\t",
        " ",
    ])
    .prop_map(|s: &str| s.to_string());
    prop::collection::vec(
        prop_oneof![
            4 => keyword,
            3 => ident,
            2 => number,
            2 => float,
            3 => quoted,
            2 => op,
        ],
        0..20,
    )
    .prop_map(|tokens| tokens.join(" "))
}

/// Safe SQL identifier (lowercase + digits, never a reserved word — "fz" prefix).
fn fz_ident() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::char::range('a', 'z'), 1..8)
        .prop_map(|v| format!("fz_{}", v.into_iter().collect::<String>()))
}

/// Fuzzed-but-valid CREATE → INSERT → SELECT lifecycle.
/// Returns (create_sql, insert_sql, count_sql, expected_rows).
fn lifecycle() -> impl Strategy<Value = (String, String, String, usize)> {
    // Values: i64 for id (unique via enumerate), safe string, fixed-point float
    // (avoid scientific notation that sqlparser may reject).
    let val = (
        prop::num::i64::ANY,
        prop::collection::vec(
            prop_oneof![
                prop::char::range('a', 'z'),
                prop::char::range('0', '9'),
                Just(' '),
                Just('_'),
                Just('-'),
            ],
            0..12,
        )
        .prop_map(|v| v.into_iter().collect::<String>()),
        (-1_000_000i64..1_000_000i64).prop_map(|n| n as f64 / 100.0),
    );
    (fz_ident(), prop::collection::vec(val, 1..10)).prop_map(|(table, rows)| {
        let create = format!("CREATE TABLE {table} (id INT PRIMARY KEY, a STRING, b REAL)");
        let values: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, (_, a, b))| format!("({i}, '{a}', {b})"))
            .collect();
        let insert = format!("INSERT INTO {table} VALUES {}", values.join(", "));
        let count = format!("SELECT COUNT(*) FROM {table}");
        (create, insert, count, rows.len())
    })
}

/// Corpus seed mutated with 0–3 random byte insertions.
fn mutated_corpus() -> impl Strategy<Value = String> {
    (prop::sample::select(CORPUS), 0..4u32).prop_flat_map(|(seed, n)| {
        let junk = prop::collection::vec(
            prop::sample::select(&['x', '0', '\u{0}', '\'', '"', ';', ' ', '\n', '\u{7f}', '\u{4f60}']),
            n as usize,
        );
        junk.prop_map(move |junk| {
            let mut s: Vec<char> = seed.chars().collect();
            for c in junk {
                if s.is_empty() {
                    s.push('x');
                    continue;
                }
                let pos = (c as usize) % s.len();
                s.insert(pos, c);
            }
            s.into_iter().collect()
        })
    })
}

/// The main fuzz input: token soup, mutated corpus, and raw byte soup
/// (lossy UTF-8 conversion exercises invalid-UTF-8 inputs like cargo-fuzz).
fn fuzz_input() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => sqlish(),
        2 => mutated_corpus(),
        1 => prop::collection::vec(any::<u8>(), 0..64)
            .prop_map(|v| String::from_utf8_lossy(&v).into_owned()),
    ]
}

// ── Fuzz driver ─────────────────────────────────────────────────────────────

/// Run a sequence of statements against ONE fresh Database (isolation:
/// per-iteration, not per-statement). Returns each response in order, or
/// Err(panic_message) on the first panic.
fn run_sequence<'a>(stmts: impl IntoIterator<Item = &'a str>) -> Result<Vec<String>, String> {
    let mut db = Database::new();
    let mut out = Vec::new();
    for s in stmts {
        let res = catch_unwind(AssertUnwindSafe(|| dispatch_inner(&mut db, s, &[])));
        match res {
            Ok(resp) => out.push(resp),
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "<non-string panic payload>".to_string());
                return Err(format!("{msg} (on statement {s:?})"));
            }
        }
    }
    Ok(out)
}

/// Run one input through the full parse → execute path on a fresh Database.
/// Returns Err(panic_message) if the engine panicked.
fn run_dispatch(input: &str) -> Result<String, String> {
    run_sequence(std::iter::once(input)).map(|mut v| v.pop().expect("one statement"))
}

/// Assert the envelope shape: non-empty, starts with '[', status word OK/ERR_*.
fn check_envelope(input: &str, resp: &str) -> Result<(), TestCaseError> {
    prop_assert!(!resp.is_empty(), "empty response for input: {:?}", input);
    prop_assert!(
        resp.starts_with('['),
        "response does not start with '[' for input {:?}: {:?}",
        input,
        resp
    );
    if let Ok(v) = serde_json::from_str::<Vec<serde_json::Value>>(resp) {
        prop_assert!(v.len() >= 2, "envelope too short for input {:?}: {:?}", input, resp);
        let status = v[1].as_str().unwrap_or("");
        prop_assert!(
            status == "OK" || status.starts_with("ERR_"),
            "bad status {:?} for input {:?}: {:?}",
            status,
            input,
            resp
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 20_000, timeout: 60, .. ProptestConfig::default() })]

    /// Invariant 1+2: dispatch never panics and returns a well-formed envelope.
    #[test]
    fn dispatch_never_panics_any_input(input in fuzz_input()) {
        if is_custom_command(&input) {
            return Ok(()); // skip custom-command surface (side effects)
        }
        match run_dispatch(&input) {
            Ok(resp) => check_envelope(&input, &resp)?,
            Err(panic_msg) => {
                panic!("ENGINE PANICKED on input {:?}\npanic payload: {}", input, panic_msg)
            }
        }
    }

    /// Invariant 3: CREATE + INSERT + SELECT on a fuzzed-but-valid schema stays consistent.
    #[test]
    fn create_insert_select_consistent((create, insert, count, expected) in lifecycle()) {
        let responses = run_sequence([create.as_str(), insert.as_str(), count.as_str()])
            .map_err(|m| TestCaseError::fail(format!("sequence panicked: {m}")))?;
        let [create_resp, insert_resp, count_resp] = responses
            .try_into()
            .map_err(|_| TestCaseError::fail("expected 3 responses"))?;
        prop_assert!(create_resp.starts_with("[0,\"OK\""), "CREATE failed: {} -> {}", create, create_resp);
        prop_assert!(insert_resp.starts_with("[0,\"OK\""), "INSERT failed: {} -> {}", insert, insert_resp);
        prop_assert!(count_resp.starts_with("[0,\"OK\""), "COUNT failed: {} -> {}", count, count_resp);
        // Parse [[header],[row]] payload and check the count value matches.
        let payload: Vec<serde_json::Value> = serde_json::from_str(&count_resp)
            .map_err(|e| TestCaseError::fail(format!("COUNT envelope not JSON: {} ({})", count_resp, e)))?;
        let data = payload.get(2).and_then(|d| d.as_array()).cloned().unwrap_or_default();
        // data[0] is the header; data[1] is the first data row.
        let got = data.get(1).and_then(|row| row.get(0)).and_then(|v| v.as_i64());
        prop_assert_eq!(got, Some(expected as i64), "COUNT mismatch: {} -> {}", count, count_resp);
    }
}

/// Invariant 4: the fixed weird-input list never panics and always returns an envelope.
#[test]
fn weird_inputs_never_crash() {
    let mut inputs: Vec<String> = WEIRD_INPUTS.iter().map(|s| s.to_string()).collect();
    inputs.push("SELECT ".to_owned() + &"x".repeat(50_000)); // very long input
    for input in inputs {
        match run_dispatch(&input) {
            Ok(resp) => {
                assert!(!resp.is_empty(), "empty response for input: {:?}", input);
                assert!(resp.starts_with('['), "bad envelope for input {:?}: {:?}", input, resp);
            }
            Err(panic_msg) => {
                panic!(
                    "ENGINE PANICKED on weird input {:?}\npanic payload: {}",
                    input, panic_msg
                )
            }
        }
    }
}

/// The seed corpus itself runs clean (fast regression check).
#[test]
fn corpus_runs_clean() {
    for &sql in CORPUS {
        match run_dispatch(sql) {
            Ok(resp) => {
                assert!(
                    resp.starts_with('['),
                    "bad envelope for corpus seed {:?}: {:?}",
                    sql,
                    resp
                );
            }
            Err(panic_msg) => {
                panic!("ENGINE PANICKED on corpus seed {:?}\npanic payload: {}", sql, panic_msg)
            }
        }
    }
}
