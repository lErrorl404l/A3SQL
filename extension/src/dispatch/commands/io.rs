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
    if let Err(e) = std::fs::write(&tmp_path, &bytes) {
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
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => return error_response(ErrorCode::Io, &format!("Read failed: {}", e)),
    };
    db.clear();
    match engine::serialize::import_binary(&bytes, db) {
        Ok(()) => ok_response(&format!(
            "\"Loaded from '{}/{}'\"",
            CONFIG.data_dir().display(),
            filename
        )),
        Err(e) => {
            // Main save corrupt/truncated — fall back to the last good .bak
            // (created by handle_save before each successful rename).
            let bak_path = with_ext_suffix(&path, "bak");
            match std::fs::read(&bak_path) {
                Ok(bak_bytes) => {
                    db.clear();
                    match engine::serialize::import_binary(&bak_bytes, db) {
                        Ok(()) => ok_response(&format!(
                            "\"Loaded from backup '{}/{}' (main save corrupt: {})\"",
                            CONFIG.data_dir().display(),
                            bak_path.file_name().and_then(|f| f.to_str()).unwrap_or(""),
                            e
                        )),
                        Err(bak_e) => error_response(
                            ErrorCode::Exec,
                            &format!("Load failed (main: {}; backup: {})", e, bak_e),
                        ),
                    }
                }
                Err(_) => error_response(ErrorCode::Exec, &format!("Load failed: {}", e)),
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
}
