// Property tests: serialization round-trips losslessly.
//
// For each format (JSON, CSV, binary) we generate a random table — mixed column
// types, random values, occasional NULLs — export it, import into a fresh
// Database, and require the data to survive:
//   - JSON:   columns (name/dtype/PK flag) and rows must be byte-identical.
//   - binary: same, bit-exact (NaN included, though the generator stays finite
//             so the same table drives all three formats).
//   - CSV:    a text format — columns come back as STRING and cells as display
//             text, so rows are compared at the display-string level, plus
//             re-export of the imported table must reproduce the same CSV.
//
// Generator constraints (format limitations, documented in serialize/*):
//   - JSON: NaN and Infinity are not valid JSON numbers, and Strings-array
//     elements are written UNescaped by to_json_string — so array elements are
//     restricted to [a-z] and floats to finite.
//   - CSV:  import trims fields and splits on newlines — strings carry no
//     control chars and no leading/trailing whitespace.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;

use super::helpers::*;
use crate::engine::serialize::{export_binary, export_csv, export_json, import_binary, import_csv, import_json};

#[derive(Clone, Copy, Debug)]
enum Spec {
    Int,
    Float,
    Str,
    Bool,
    StrArr,
    FltArr,
}

fn col(name: &str, dtype: ColumnType, primary_key: bool) -> Column {
    Column {
        name: name.into(),
        dtype,
        primary_key,
        not_null: false,
        default: None,
        default_expr: None,
        auto_increment: false,
        unique: false,
    }
}

fn col_type(sp: Spec) -> ColumnType {
    match sp {
        Spec::Int => ColumnType::Int,
        Spec::Float => ColumnType::Float,
        Spec::Str => ColumnType::String,
        Spec::Bool => ColumnType::Bool,
        Spec::StrArr => ColumnType::Strings,
        Spec::FltArr => ColumnType::Floats,
    }
}

/// JSON is lossless only for floats that survive serde_json's own number
/// parser. serde_json's default (non-arbitrary_precision) parser is not
/// correctly rounded for every f64 — e.g. "123456789.12345679" parses one ULP
/// off (Rust's own `f64::from_str` gets it right; serde_json does not). The
/// engine's JSON path is therefore bit-exact exactly for the values that
/// round-trip through its export string + serde_json — this filter is that
/// bound, so the round-trip assertion below is honest, not lossy.
fn json_safe_float(f: f64) -> bool {
    // Scalar path (integral floats get a ".0" suffix) and array-element path
    // (plain Display) must both survive serde_json.
    let scalar = serde_json::from_str::<serde_json::Value>(&DbValue::Float(f).to_json_string())
        .ok()
        .and_then(|v| v.as_f64());
    let elem = serde_json::from_str::<serde_json::Value>(&f.to_string())
        .ok()
        .and_then(|v| v.as_f64());
    scalar == Some(f) && elem == Some(f)
}

fn finite_float() -> impl Strategy<Value = f64> {
    any::<f64>().prop_filter("finite + JSON-lossless", |f| f.is_finite() && json_safe_float(*f))
}

/// Printable, no control chars; round-trips JSON (escaping) and CSV (no
/// newlines, no leading/trailing whitespace that import trims).
fn safe_string() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            prop::char::range('a', 'z'),
            prop::char::range('0', '9'),
            Just(' '),
            Just('_'),
            Just('-'),
            Just('\''),
            Just('"'),
            Just('\\'),
            Just(','),
        ],
        0..12,
    )
    .prop_map(|v| v.into_iter().collect())
    .prop_filter("no leading/trailing space (CSV trims)", |s: &String| {
        !s.starts_with(' ') && !s.ends_with(' ')
    })
}

fn arr_elem() -> impl Strategy<Value = String> {
    // Strings-array elements are written unescaped by to_json_string — stay
    // JSON-safe (no quotes/backslashes).
    prop::collection::vec(prop::char::range('a', 'z'), 0..6).prop_map(|v| v.into_iter().collect())
}

fn cell_value(sp: Spec) -> BoxedStrategy<DbValue> {
    match sp {
        Spec::Int => any::<i64>().prop_map(DbValue::Int).boxed(),
        Spec::Float => finite_float().prop_map(DbValue::Float).boxed(),
        Spec::Str => safe_string().prop_map(DbValue::String).boxed(),
        Spec::Bool => any::<bool>().prop_map(DbValue::Bool).boxed(),
        Spec::StrArr => prop::collection::vec(arr_elem(), 0..3)
            .prop_map(DbValue::Strings)
            .boxed(),
        Spec::FltArr => prop::collection::vec(finite_float(), 0..3)
            .prop_map(DbValue::Floats)
            .boxed(),
    }
}

/// (column spec, rows) — rows are [id, v, w]: id is a unique Int PK, v and w
/// are drawn from the spec (10% NULL), exercising Null round-trip too.
fn random_table() -> impl Strategy<Value = (Spec, Vec<Vec<DbValue>>)> {
    let spec = prop_oneof![
        Just(Spec::Int),
        Just(Spec::Float),
        Just(Spec::Str),
        Just(Spec::Bool),
        Just(Spec::StrArr),
        Just(Spec::FltArr),
    ];
    (spec, 0..20usize).prop_flat_map(|(sp, nrows)| {
        let cell = prop_oneof![3 => cell_value(sp), 1 => Just(DbValue::Null)];
        let rows = prop::collection::vec((cell.clone(), cell), nrows).prop_map(|rows| {
            rows.into_iter()
                .enumerate()
                .map(|(i, (v, w))| vec![DbValue::Int(i as i64), v, w])
                .collect()
        });
        (Just(sp), rows)
    })
}

fn build_table(sp: Spec, rows: Vec<Vec<DbValue>>) -> Table {
    let cols = vec![
        col("id", ColumnType::Int, true),
        col("v", col_type(sp), false),
        col("w", col_type(sp), false),
    ];
    let mut t = Table::new("t".into(), cols).unwrap();
    for row in rows {
        t.insert(row).unwrap();
    }
    t
}

fn col_sig(t: &Table) -> Vec<(String, ColumnType, bool)> {
    t.columns
        .iter()
        .map(|c| (c.name.clone(), c.dtype.clone(), c.primary_key))
        .collect()
}

/// CSV cell display (mirrors csv.rs value_to_display).
fn display(v: &DbValue) -> String {
    match v {
        DbValue::Null => String::new(),
        DbValue::Bool(b) => b.to_string(),
        DbValue::Int(n) => n.to_string(),
        DbValue::Float(f) => f.to_string(),
        DbValue::String(s) => s.clone(),
        DbValue::Strings(a) => a.join(";"),
        DbValue::Floats(a) => a.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(";"),
    }
}

proptest! {
    #[test]
    fn serialize_json_roundtrip((sp, rows) in random_table()) {
        let orig = build_table(sp, rows);
        let json = export_json(&orig);
        let mut db2 = Database::new();
        import_json("t", &json, &mut db2).unwrap_or_else(|e| panic!("import: {e}\n{json}"));
        let imported = db2.get_table("t").unwrap();
        prop_assert_eq!(&orig.rows, &imported.rows, "rows diverged");
        prop_assert_eq!(col_sig(&orig), col_sig(imported), "schema diverged");
    }

    #[test]
    fn serialize_csv_roundtrip((sp, rows) in random_table()) {
        let orig = build_table(sp, rows);
        let csv = export_csv(&orig);
        let mut db2 = Database::new();
        import_csv("t", &csv, &mut db2).unwrap_or_else(|e| panic!("import: {e}\n{csv}"));
        let imported = db2.get_table("t").unwrap();
        prop_assert_eq!(orig.rows.len(), imported.rows.len(), "row count diverged");
        prop_assert_eq!(
            imported.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            orig.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            "column names diverged"
        );
        for (i, (orow, irow)) in orig.rows.iter().zip(&imported.rows).enumerate() {
            for (j, (ov, iv)) in orow.iter().zip(irow).enumerate() {
                prop_assert_eq!(
                    iv,
                    &DbValue::String(display(ov)),
                    "cell [{}][{}] diverged: {:?}",
                    i,
                    j,
                    ov
                );
            }
        }
        prop_assert_eq!(export_csv(imported), csv, "re-export diverged");
    }

    #[test]
    fn serialize_binary_roundtrip((sp, rows) in random_table()) {
        let orig = build_table(sp, rows);
        let mut db1 = Database::new();
        db1.create_table("t", orig.clone()).unwrap();
        let bytes = export_binary(&db1);
        let mut db2 = Database::new();
        import_binary(&bytes, &mut db2).unwrap_or_else(|e| panic!("import: {e}"));
        let imported = db2.get_table("t").unwrap();
        prop_assert_eq!(&orig.rows, &imported.rows, "rows diverged");
        prop_assert_eq!(col_sig(&orig), col_sig(imported), "schema diverged");
    }
}
