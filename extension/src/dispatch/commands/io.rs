use std::path::Path;

use crate::config::CONFIG;
use crate::engine;
use crate::engine::error::{ErrorCode, error_response, ok_response};

// ── Path validation ─────────────────────────────────────────────────────

/// Validate and resolve `filename` against the configured `data_dir`.
/// Rejects absolute paths and paths containing `..` or `~`.
/// Creates the data directory if it does not exist.
/// Returns an error response string on failure.
/// Swap a path's extension, e.g. `save.bin` + `tmp` -> `save.bin.tmp`.
fn with_ext_suffix(path: &std::path::Path, suffix: &str) -> std::path::PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(format!(".{suffix}"));
    std::path::PathBuf::from(os)
}

fn safe_data_path(filename: &str) -> Result<std::path::PathBuf, String> {
    // ponytail: global data_dir lock; per-account dirs if multi-tenant needed
    let p = Path::new(filename);
    // `has_root()` catches Windows root-relative paths like `/foo` or `\foo`
    // that `is_absolute()` misses (no drive prefix) — those still escape the
    // data dir when joined. On Unix the two are equivalent.
    if p.is_absolute() || p.has_root() {
        return Err(error_response(ErrorCode::Io, "Absolute paths are not allowed"));
    }
    for component in p.components() {
        let s = component.as_os_str().to_str().unwrap_or("");
        if s == ".." {
            return Err(error_response(ErrorCode::Io, "Path must not contain '..'"));
        }
        if s == "~" {
            return Err(error_response(ErrorCode::Io, "Path must not contain '~'"));
        }
    }
    let data_dir = CONFIG.data_dir().to_path_buf();
    let resolved = data_dir.join(filename);
    ensure_within_data_dir(&resolved, &data_dir)?;
    Ok(resolved)
}

/// Reject `p` (a path already lexically inside `data_dir`) if any component
/// resolves outside `data_dir` via a symlink.
///
/// Two checks, both on the canonical (symlink-resolved) filesystem:
/// - the parent directory must live inside `data_dir` — this catches a
///   symlinked *subdirectory* (e.g. `data_dir/evil -> /etc`) even when the
///   final file does not exist yet (the write would otherwise follow the
///   subdir symlink out of the sandbox);
/// - an *existing* final component must resolve inside `data_dir` — this
///   rejects a pre-placed symlink at the target path itself.
///
/// Missing parent dirs are created first so the check runs against the real
/// resolved directories, not the lexical ones. The atomic-save format and
/// checksum scheme are untouched.
fn ensure_within_data_dir(p: &std::path::Path, data_dir: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| error_response(ErrorCode::Io, &format!("Cannot create dir: {}", e)))?;
        let canon_parent = parent
            .canonicalize()
            .map_err(|_| error_response(ErrorCode::Io, "Cannot resolve path"))?;
        let canon_data = data_dir
            .canonicalize()
            .map_err(|_| error_response(ErrorCode::Io, "Cannot resolve data dir"))?;
        if !canon_parent.starts_with(&canon_data) {
            return Err(error_response(ErrorCode::Io, "Path escapes data directory"));
        }
    }
    if p.exists() {
        let canonical = p
            .canonicalize()
            .map_err(|_| error_response(ErrorCode::Io, "Cannot resolve path"))?;
        if !canonical.starts_with(data_dir.canonicalize().unwrap_or_else(|_| data_dir.to_path_buf())) {
            return Err(error_response(ErrorCode::Io, "Path escapes data directory"));
        }
    }
    Ok(())
}

// ── Import/Export handlers ──────────────────────────────────────────────

pub(crate) fn handle_export(db: &engine::Database, input: &str, _args: &[&str]) -> String {
    let parts: Vec<&str> = input.splitn(3, |c: char| c.is_whitespace()).collect();
    let format_str = parts.get(1).unwrap_or(&"json");
    let table_name = parts.get(2).map(|s| s.trim());

    let format: engine::serialize::Format = match format_str.parse() {
        Ok(f) => f,
        Err(e) => return error_response(ErrorCode::Exec, &e),
    };

    match format {
        engine::serialize::Format::Sql => {
            let sql = engine::serialize::export_sql(db);
            let encoded = ::serde_json::to_string(&sql).unwrap_or_else(|_| "\"\"".into());
            ok_response(&encoded)
        }
        engine::serialize::Format::Binary => {
            let bytes = engine::serialize::export_binary(db);
            let hex = engine::serialize::hex_encode(&bytes);
            ok_response(&format!("\"{}\"", hex))
        }
        engine::serialize::Format::Csv => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Usage: export <format> <table>"),
            };
            match db.get_table(name) {
                Ok(table) => {
                    let data = engine::serialize::export(format, table, db);
                    let encoded = ::serde_json::to_string(&data).unwrap_or_else(|_| "\"\"".into());
                    ok_response(&encoded)
                }
                Err(e) => error_response(ErrorCode::Table, &e),
            }
        }
        _ => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Usage: export <format> <table>"),
            };
            match db.get_table(name) {
                Ok(table) => {
                    let data = engine::serialize::export(format, table, db);
                    ok_response(&data)
                }
                Err(e) => error_response(ErrorCode::Table, &e),
            }
        }
    }
}

pub(crate) fn handle_import(db: &mut engine::Database, input: &str, args: &[&str]) -> String {
    let parts: Vec<&str> = input.splitn(3, |c: char| c.is_whitespace()).collect();
    let format_str = parts.get(1).unwrap_or(&"json");
    let table_name = parts.get(2).map(|s| s.trim()).unwrap_or("");

    if table_name.is_empty() {
        return error_response(ErrorCode::Exec, "Usage: import <format> <table>");
    }

    let format: engine::serialize::Format = match format_str.parse() {
        Ok(f) => f,
        Err(e) => return error_response(ErrorCode::Exec, &e),
    };

    let data = args.first().unwrap_or(&"");
    if data.is_empty() {
        return error_response(ErrorCode::Exec, "No data provided");
    }
    match engine::serialize::import(format, table_name, data, db) {
        Ok(()) => ok_response(&format!("\"Imported into '{}'\"", table_name)),
        Err(e) => error_response(ErrorCode::Exec, &e),
    }
}

pub(crate) fn handle_dump_sql(db: &engine::Database) -> String {
    let sql = engine::serialize::export_sql(db);
    let encoded = ::serde_json::to_string(&sql).unwrap_or_else(|_| format!("\"{}\"", ""));
    ok_response(&encoded)
}

/// Write `bytes` to `path` with O_NOFOLLOW on Unix: the final component must
/// be a regular file (or newly created), never a symlink — closing the
/// TOCTOU window between `ensure_within_data_dir` and the write (a local
/// attacker could plant `a3sql.bin.tmp -> /home/user/.ssh/authorized_keys`
/// in between). On non-Unix, falls back to std::fs::write.
#[cfg(unix)]
fn write_no_follow(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    f.write_all(bytes)
}

#[cfg(not(unix))]
fn write_no_follow(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

pub(crate) fn handle_save(db: &engine::Database, args: &[&str]) -> String {
    let filename = args.first().copied().unwrap_or("a3sql.bin");
    if filename.is_empty() {
        return error_response(ErrorCode::Io, "Filename must not be empty");
    }
    let path = match safe_data_path(filename) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let bytes = engine::serialize::export_binary(db);

    // Atomic save: write to a temp file in the same dir, then rename over
    // the target (rename is atomic on the same filesystem). A crash mid-write
    // leaves only the temp file — the last good save survives untouched.
    let tmp_path = with_ext_suffix(&path, "tmp");
    // A pre-placed symlink at the tmp path would be followed by the write,
    // overwriting whatever it points to (a local attacker on a shared host
    // could plant `a3sql.bin.tmp -> /home/user/.ssh/authorized_keys`).
    if let Err(e) = ensure_within_data_dir(&tmp_path, CONFIG.data_dir()) {
        return e;
    }
    if let Err(e) = write_no_follow(&tmp_path, &bytes) {
        return error_response(ErrorCode::Io, &format!("Save failed: {}", e));
    }
    // Keep the previous good save as .bak before replacing it.
    let bak_path = with_ext_suffix(&path, "bak");
    if path.exists() {
        let _ = std::fs::rename(&path, &bak_path);
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        let _ = std::fs::remove_file(&tmp_path);
        return error_response(ErrorCode::Io, &format!("Save failed: {}", e));
    }
    ok_response(&format!("\"Saved to '{}/{}'\"", CONFIG.data_dir().display(), filename))
}

pub(crate) fn handle_load(db: &mut engine::Database, args: &[&str]) -> String {
    let filename = args.first().copied().unwrap_or("a3sql.bin");
    if filename.is_empty() {
        return error_response(ErrorCode::Io, "Filename must not be empty");
    }
    let path = match safe_data_path(filename) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let bak_path = with_ext_suffix(&path, "bak");
    let bak_name = bak_path.file_name().and_then(|f| f.to_str()).unwrap_or("").to_string();

    // Load is all-or-nothing: parse into a fresh Database and swap it in only
    // on success, so a failed load never destroys the current in-memory state.
    // The .bak fallback also fires when the main save is *missing* — a crash
    // between the two renames in handle_save leaves the main file absent
    // (already renamed to .bak) with the new save stranded in .tmp.
    let mut fresh = engine::Database::new();
    let main_result: Result<(), (ErrorCode, String)> = match std::fs::read(&path) {
        Ok(bytes) => engine::serialize::import_binary(&bytes, &mut fresh)
            .map_err(|e| (ErrorCode::Exec, format!("Load failed: {}", e))),
        Err(e) => Err((ErrorCode::Io, format!("Read failed: {}", e))),
    };

    match main_result {
        Ok(()) => {
            *db = fresh;
            ok_response(&format!(
                "\"Loaded from '{}/{}'\"",
                CONFIG.data_dir().display(),
                filename
            ))
        }
        Err((code, main_err)) => {
            // Main save missing or corrupt — fall back to the last good .bak
            // (created by handle_save before each successful rename).
            fresh = engine::Database::new();
            let main_flaw = if code == ErrorCode::Io {
                "main save unreadable"
            } else {
                "main save corrupt"
            };
            match std::fs::read(&bak_path) {
                Ok(bak_bytes) => match engine::serialize::import_binary(&bak_bytes, &mut fresh) {
                    Ok(()) => {
                        *db = fresh;
                        ok_response(&format!(
                            "\"Loaded from backup '{}/{}' ({}: {})\"",
                            CONFIG.data_dir().display(),
                            bak_name,
                            main_flaw,
                            main_err
                        ))
                    }
                    Err(bak_e) => error_response(
                        ErrorCode::Exec,
                        &format!("Load failed (main: {}; backup: {})", main_err, bak_e),
                    ),
                },
                Err(_) => error_response(code, &main_err),
            }
        }
    }
}

// ── Export to file ─────────────────────────────────────────────────────

/// Write table data or SQL dump directly to a file on disk.
pub(crate) fn handle_export_to_file(db: &engine::Database, trimmed: &str, args: &[&str]) -> String {
    let parts: Vec<&str> = trimmed.splitn(4, |c: char| c.is_whitespace()).collect();
    let format_str = parts.get(1).copied().unwrap_or("json");
    let has_table = parts.len() > 2 && !parts[2].is_empty();
    let table_name = if has_table { parts.get(2) } else { None };
    let cmd_path = if has_table { parts.get(3) } else { parts.get(2) };
    let filename: String = match cmd_path.or_else(|| args.first()) {
        Some(p) => p.to_string(),
        None => {
            if format_str == "sql" {
                "a3sql_export.sql".into()
            } else if let Some(t) = table_name {
                format!("{}.{}", t, format_str)
            } else {
                "a3sql_export.txt".into()
            }
        }
    };

    if filename.is_empty() {
        return error_response(ErrorCode::Io, "Filename must not be empty");
    }

    let format: engine::serialize::Format = match format_str.parse() {
        Ok(f) => f,
        Err(e) => return error_response(ErrorCode::Exec, &e),
    };

    let data = match format {
        engine::serialize::Format::Sql => engine::serialize::export_sql(db),
        engine::serialize::Format::Binary => engine::serialize::hex_encode(&engine::serialize::export_binary(db)),
        _ => {
            let name = match table_name {
                Some(n) if !n.is_empty() => n,
                _ => return error_response(ErrorCode::Exec, "Table name required for json/csv export"),
            };
            let table = match db.get_table(name) {
                Ok(t) => t,
                Err(e) => return error_response(ErrorCode::Table, &e),
            };
            engine::serialize::export(format, table, db)
        }
    };

    let path = match safe_data_path(&filename) {
        Ok(p) => p,
        Err(e) => return e,
    };

    let path_display = path.to_string_lossy().to_string();
    match std::fs::write(&path, &data) {
        Ok(()) => ok_response(&format!("\"Exported to '{}'\"", path_display)),
        Err(e) => error_response(ErrorCode::Io, &format!("Write failed: {}", e)),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn err_msg(r: Result<std::path::PathBuf, String>) -> String {
        match r {
            Ok(_) => "OK".to_string(),
            Err(e) => e,
        }
    }

    fn err2(r: Result<(), String>) -> String {
        match r {
            Ok(()) => "OK".to_string(),
            Err(e) => e,
        }
    }

    /// Fresh, uniquely-named temp dir per test (isolated across runs and
    /// parallel tests; no dependence on the process-global CONFIG.data_dir).
    fn temp_data_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("a3sql_io_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A sibling of the test data dir — escaping here lands OUTSIDE it.
    fn temp_outside_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("a3sql_outside_{}_{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(err_msg(safe_data_path("/etc/passwd")).contains("Absolute paths"));
        #[cfg(windows)]
        assert!(err_msg(safe_data_path("C:\\windows\\system32\\evil.dll")).contains("Absolute paths"));
    }

    #[test]
    fn rejects_root_relative_paths() {
        // Windows treats `/foo` / `\foo` as root-relative (root, no drive
        // prefix) — `is_absolute()` alone misses these, so the data dir can
        // be escaped. `has_root()` catches them on both platforms.
        assert!(err_msg(safe_data_path("/tmp/escape.sql")).contains("Absolute paths"));
        #[cfg(windows)]
        assert!(err_msg(safe_data_path("\\tmp\\escape.sql")).contains("Absolute paths"));
    }

    #[test]
    fn rejects_parent_dir_and_tilde() {
        assert!(err_msg(safe_data_path("../evil.sql")).contains("'..'"));
        assert!(err_msg(safe_data_path("sub/../../evil.sql")).contains("'..'"));
        assert!(err_msg(safe_data_path("~/evil.sql")).contains("'~'"));
    }

    #[test]
    #[cfg_attr(miri, ignore)] // creates/removes the data dir - fs blocked by miri isolation
    fn accepts_relative_paths() {
        let r = safe_data_path("a3sql.bin");
        assert!(r.is_ok(), "expected OK, got {}", err_msg(r));
        let r = safe_data_path("subdir/a3sql.bin");
        assert!(r.is_ok(), "expected OK, got {}", err_msg(r));
        // cleanup side effects from the accepted cases
        let _ = std::fs::remove_dir_all(CONFIG.data_dir());
    }

    #[test]
    #[cfg(unix)] // symlink(2) semantics; Windows link creation needs privileges
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn rejects_final_symlink_escaping_data_dir() {
        let dir = temp_data_dir("final_link");
        let outside = temp_outside_dir("final_link");
        let target = outside.join("secret.txt");
        std::fs::write(&target, b"data").unwrap();
        std::os::unix::fs::symlink(&target, dir.join("evil.bin")).unwrap();

        let r = ensure_within_data_dir(&dir.join("evil.bin"), &dir);
        let msg = err2(r);
        assert!(
            msg.contains("escapes"),
            "pre-placed final symlink must be rejected, got: {}",
            msg
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    #[cfg(unix)]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn rejects_subdir_symlink_escaping_data_dir() {
        let dir = temp_data_dir("sub_link");
        let outside = temp_outside_dir("sub_link");
        std::os::unix::fs::symlink(&outside, dir.join("sub")).unwrap();

        // The final file does not exist yet — a write would follow the `sub`
        // symlink out of the data dir. The parent check must catch it.
        let r = ensure_within_data_dir(&dir.join("sub").join("new.bin"), &dir);
        let msg = err2(r);
        assert!(
            msg.contains("escapes"),
            "subdirectory symlink must be rejected, got: {}",
            msg
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    #[cfg(unix)]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn accepts_symlink_within_data_dir() {
        let dir = temp_data_dir("inner_link");
        let real = dir.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, dir.join("alias")).unwrap();

        let r = ensure_within_data_dir(&dir.join("alias").join("ok.bin"), &dir);
        assert!(r.is_ok(), "symlink inside data dir is fine: {}", err2(r));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn accepts_regular_new_file() {
        let dir = temp_data_dir("regular");
        let r = ensure_within_data_dir(&dir.join("fresh.bin"), &dir);
        assert!(r.is_ok(), "regular new file must be accepted: {}", err2(r));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Crash-recovery / durability (T4) ─────────────────────────────────
    // Exercises the real handle_save/handle_load against the configured data
    // dir (the existing test pattern), replaying the on-disk states each crash
    // window in the atomic-save leaves behind.

    use crate::engine::table::Table;
    use crate::engine::value::*;

    /// Two-column table ("items": id STRING PK, val INT) with 2 rows, plus an
    /// optional third row so v1/v2 saves are distinguishable.
    fn make_db(extra_row: bool) -> engine::Database {
        let mut db = engine::Database::new();
        let cols = vec![
            Column {
                name: "id".into(),
                dtype: ColumnType::String,
                primary_key: true,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            },
            Column {
                name: "val".into(),
                dtype: ColumnType::Int,
                primary_key: false,
                not_null: false,
                default: None,
                default_expr: None,
                auto_increment: false,
                unique: false,
            },
        ];
        let mut table = Table::new("items".into(), cols).unwrap();
        table
            .insert(vec![DbValue::String("a".into()), DbValue::Int(10)])
            .unwrap();
        table
            .insert(vec![DbValue::String("b".into()), DbValue::Int(20)])
            .unwrap();
        if extra_row {
            table
                .insert(vec![DbValue::String("c".into()), DbValue::Int(30)])
                .unwrap();
        }
        db.create_table("items", table).unwrap();
        db
    }

    fn row_count(db: &engine::Database) -> usize {
        db.get_table("items").unwrap().rows.len()
    }

    /// Unique filename per run/test so parallel tests and repeated runs never
    /// collide in the shared data dir.
    fn unique_file(tag: &str) -> String {
        format!("a3sql_crash_{}_{}.bin", std::process::id(), tag)
    }

    fn cleanup(tag: &str) {
        let f = unique_file(tag);
        for suffix in ["", ".bak", ".tmp"] {
            let _ = std::fs::remove_file(CONFIG.data_dir().join(format!("{f}{suffix}")));
        }
    }

    /// Independent FNV-1a 64-bit (standard offset basis + prime) used to
    /// verify the checksum trailer, not copied from the engine.
    fn fnv1a(data: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in data {
            hash ^= u64::from(*b);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    #[test]
    fn binary_format_checksum_and_corruption_detection() {
        let bytes = engine::serialize::export_binary(&make_db(false));
        // Header: 4-byte magic "A3SQ" + 1-byte format version.
        assert_eq!(&bytes[0..4], b"A3SQ", "magic");
        assert_eq!(bytes[4], 0x02, "format version");
        // Trailer: 8-byte FNV-1a over everything before it, stored LE.
        let payload_end = bytes.len() - 8;
        let stored = u64::from_le_bytes(bytes[payload_end..].try_into().unwrap());
        assert_eq!(
            stored,
            fnv1a(&bytes[..payload_end]),
            "trailer must be FNV-1a of payload"
        );
        // A clean save round-trips.
        let mut clean = engine::Database::new();
        engine::serialize::import_binary(&bytes, &mut clean).unwrap();
        assert_eq!(clean.get_table("items").unwrap().rows.len(), 2);
        // Any payload bit-flip must be rejected by the checksum.
        for offset in [5usize, 6, 12, payload_end - 1] {
            let mut bad = bytes.clone();
            bad[offset] ^= 0x01;
            let mut t = engine::Database::new();
            let e = engine::serialize::import_binary(&bad, &mut t).unwrap_err();
            assert!(
                e.contains("Checksum"),
                "bit-flip at byte {} must hit checksum, got: {}",
                offset,
                e
            );
        }
        // Truncation at any point must be rejected.
        for cut in [13usize, payload_end / 2, payload_end - 1] {
            let mut t = engine::Database::new();
            assert!(
                engine::serialize::import_binary(&bytes[..cut], &mut t).is_err(),
                "truncation to {} bytes must be detected",
                cut
            );
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn save_load_roundtrip_first_save_is_atomic() {
        let tag = "roundtrip";
        cleanup(tag);
        let filename = unique_file(tag);
        let dir = CONFIG.data_dir().to_path_buf();
        let main = dir.join(&filename);

        let r = handle_save(&make_db(false), &[filename.as_str()]);
        assert!(r.contains("\"OK\""), "save: {}", r);
        assert!(main.exists(), "main save must exist");
        assert!(!dir.join(format!("{filename}.tmp")).exists(), "no .tmp left after save");
        assert!(!dir.join(format!("{filename}.bak")).exists(), "no .bak on first save");

        let mut fresh = engine::Database::new();
        let r = handle_load(&mut fresh, &[filename.as_str()]);
        assert!(r.contains("\"OK\""), "load: {}", r);
        assert!(fresh.has_table("items"));
        assert_eq!(row_count(&fresh), 2);
        assert_eq!(fresh.get_table("items").unwrap().columns[0].name, "id");
        cleanup(tag);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn second_save_rotates_bak_corrupt_main_recovers() {
        let tag = "bak_rotate";
        cleanup(tag);
        let filename = unique_file(tag);
        let dir = CONFIG.data_dir().to_path_buf();
        let main = dir.join(&filename);
        let bak = dir.join(format!("{filename}.bak"));

        // v1 (2 rows), then v2 (3 rows) → main=v2, bak=v1.
        assert!(handle_save(&make_db(false), &[filename.as_str()]).contains("\"OK\""));
        assert!(handle_save(&make_db(true), &[filename.as_str()]).contains("\"OK\""));
        let v1_bytes = std::fs::read(&bak).unwrap();
        assert_eq!(
            v1_bytes,
            engine::serialize::export_binary(&make_db(false)),
            ".bak must hold the previous good save (v1)"
        );

        // Corrupt the main save inside the payload.
        let mut main_bytes = std::fs::read(&main).unwrap();
        main_bytes[8] ^= 0xff;
        std::fs::write(&main, &main_bytes).unwrap();

        // Load must fall back to .bak and restore v1.
        let mut fresh = engine::Database::new();
        let r = handle_load(&mut fresh, &[filename.as_str()]);
        assert!(r.contains("backup"), "expected backup fallback, got: {}", r);
        assert_eq!(row_count(&fresh), 2, "bak holds v1 (2 rows)");
        assert!(
            !fresh
                .get_table("items")
                .unwrap()
                .rows
                .iter()
                .any(|row| row[0] == DbValue::String("c".into())),
            "v2 row must not leak through the backup"
        );
        // .bak itself stays intact.
        assert_eq!(std::fs::read(&bak).unwrap(), v1_bytes);
        cleanup(tag);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn mid_write_crash_leaves_tmp_and_preserves_last_good() {
        let tag = "midwrite";
        cleanup(tag);
        let filename = unique_file(tag);
        let dir = CONFIG.data_dir().to_path_buf();
        let main = dir.join(&filename);
        let tmp = dir.join(format!("{filename}.tmp"));

        assert!(handle_save(&make_db(false), &[filename.as_str()]).contains("\"OK\"")); // v1 on disk
        let v1_bytes = std::fs::read(&main).unwrap();

        // Crash mid-write of v2: only half the new payload reaches .tmp.
        let v2 = engine::serialize::export_binary(&make_db(true));
        std::fs::write(&tmp, &v2[..v2.len() / 2]).unwrap();

        // The interrupted write must never have touched the main file.
        assert_eq!(
            std::fs::read(&main).unwrap(),
            v1_bytes,
            "main untouched by mid-write crash"
        );

        // Load recovers the last good save (v1); the stale .tmp is ignored.
        let mut fresh = engine::Database::new();
        let r = handle_load(&mut fresh, &[filename.as_str()]);
        assert!(r.contains("\"OK\""), "load after mid-write crash: {}", r);
        assert_eq!(row_count(&fresh), 2);

        // A subsequent save overwrites the stale .tmp and yields a valid file.
        assert!(handle_save(&make_db(true), &[filename.as_str()]).contains("\"OK\""));
        assert!(!tmp.exists(), "stale .tmp must be gone after the next save");
        let mut fresh2 = engine::Database::new();
        let r = handle_load(&mut fresh2, &[filename.as_str()]);
        assert!(r.contains("\"OK\""), "load after recovery save: {}", r);
        assert_eq!(row_count(&fresh2), 3, "v2 fully recovered");
        cleanup(tag);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn crash_between_renames_recovers_from_bak() {
        let tag = "rename_gap";
        cleanup(tag);
        let filename = unique_file(tag);
        let dir = CONFIG.data_dir().to_path_buf();
        let main = dir.join(&filename);
        let bak = dir.join(format!("{filename}.bak"));
        let tmp = dir.join(format!("{filename}.tmp"));

        assert!(handle_save(&make_db(false), &[filename.as_str()]).contains("\"OK\"")); // v1
        assert!(handle_save(&make_db(true), &[filename.as_str()]).contains("\"OK\"")); // v2 → main=v2, bak=v1

        // Replay the crash window inside handle_save: the first rename
        // (main→bak) completed, the second (tmp→main) never ran. The main file
        // is absent; only .bak plus a stranded .tmp remain.
        std::fs::write(&tmp, b"partial").unwrap();
        std::fs::rename(&main, &bak).unwrap();
        assert!(!main.exists(), "crash window leaves the main file absent");

        // Load must recover from .bak (the previous good save, v2: 3 rows).
        let mut fresh = engine::Database::new();
        let r = handle_load(&mut fresh, &[filename.as_str()]);
        assert!(
            r.contains("backup"),
            "expected backup fallback for missing main, got: {}",
            r
        );
        assert_eq!(row_count(&fresh), 3, ".bak holds the previous good save (v2)");
        cleanup(tag);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    fn failed_load_preserves_in_memory_state() {
        let tag = "state_keep";
        cleanup(tag);
        let filename = unique_file(tag);
        let dir = CONFIG.data_dir().to_path_buf();
        let main = dir.join(&filename);

        // In-memory DB with live data; save it once (no .bak on first save).
        let mut db = make_db(true);
        assert!(handle_save(&db, &[filename.as_str()]).contains("\"OK\""));
        assert_eq!(row_count(&db), 3);

        // Corrupt the main save → load fails → in-memory data must survive.
        let mut bytes = std::fs::read(&main).unwrap();
        bytes[7] ^= 0xff;
        std::fs::write(&main, &bytes).unwrap();
        let r = handle_load(&mut db, &[filename.as_str()]);
        assert!(!r.contains("\"OK\""), "corrupt load must fail: {}", r);
        assert_eq!(row_count(&db), 3, "failed load must not destroy in-memory data");
        assert!(db.has_table("items"));

        // Missing file → load fails → in-memory data must survive.
        let r = handle_load(&mut db, &["no_such_file_xyz.bin"]);
        assert!(!r.contains("\"OK\""), "missing-file load must fail: {}", r);
        assert_eq!(row_count(&db), 3, "missing-file load must not destroy in-memory data");
        cleanup(tag);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // fs blocked by miri isolation
    #[cfg(unix)]
    fn save_refuses_symlinked_tmp_no_follow() {
        // TOCTOU regression: a symlink planted at the .tmp path must NOT be
        // followed by the save write (write_no_follow uses O_NOFOLLOW).
        let tag = "no_follow";
        cleanup(tag);
        let filename = unique_file(tag);
        let dir = CONFIG.data_dir().to_path_buf();
        let tmp = dir.join(format!("{filename}.tmp"));
        let victim = dir.join(format!("{filename}.victim"));

        // Plant the symlink where handle_save will write .tmp.
        std::fs::write(&victim, b"sentinel").unwrap();
        std::fs::remove_file(&tmp).ok();
        std::os::unix::fs::symlink(&victim, &tmp).unwrap();

        // Save must fail cleanly (O_NOFOLLOW) and must NOT overwrite victim.
        let r = handle_save(&make_db(false), &[filename.as_str()]);
        assert!(
            !r.contains("\"OK\""),
            "save through a symlinked .tmp must fail, got: {}",
            r
        );
        assert_eq!(
            std::fs::read(&victim).unwrap(),
            b"sentinel",
            "victim must not be overwritten through the symlink"
        );
        cleanup(tag);
    }
}
