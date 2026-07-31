// a3sql — TCP server for external query access

//! TCP server — standalone and in-game TCP query interface.
//! Supports LOGIN auth, multi-client, and optional file persistence.
//!
//! # Security boundary — auth bypass note
//! The TCP LOGIN auth prevents _external_ network access, but SQF running in
//! the same game client can bypass it entirely via `RVExtensionArgs` which
//! has full DB access. This is an accepted design constraint in the Arma
//! threat model: SQF already owns the process and its memory.

use crate::dispatch;
use crate::engine::error::{error_response, ErrorCode};
use crate::ffi::{CREDENTIALS, LISTENER};

/// Serve a single TCP client connection.
/// Reads lines, handles LOGIN/auth, dispatches SQL, responds.
/// Used by both the in-game TCP listener and the standalone server.
fn serve_client(stream: std::net::TcpStream) {
    use std::io::{BufRead, BufReader, Write};
    let mut stream = stream;
    let mut reader = match stream.try_clone() {
        Ok(c) => BufReader::new(c),
        Err(_) => return,
    };

    // Snapshot credentials once per connection to avoid TOCTOU between
    // the has-auth check and login comparison (credentials are read via
    // RwLock in the FFI layer and could change between calls).
    let (expected_user, expected_pass) = CREDENTIALS.lock().unwrap().clone();

    // Auth is mandatory when credentials are set OR the operator forced it
    // via `listener_require_auth = true` in a3sql.toml (shared-host posture:
    // reject anonymous local connections even with no credentials configured).
    let has_auth =
        !expected_user.is_empty() || !expected_pass.is_empty() || crate::config::CONFIG.listener_auth_required();
    let mut authenticated = !has_auth;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "QUIT" || trimmed == "EXIT" {
            break;
        }
        if !authenticated {
            if let Some(rest) = trimmed.strip_prefix("LOGIN ") {
                if expected_user.is_empty() && expected_pass.is_empty() {
                    // listener_require_auth is set but no credentials are
                    // configured — LOGIN can never succeed. Say so plainly.
                    let _ = writeln!(
                        stream,
                        "[-1,\"ERR_AUTH\",\"No credentials configured — set listener_user/password in CBA settings\"]"
                    );
                    let _ = stream.flush();
                    break;
                }
                let parts: Vec<&str> = rest.splitn(2, ' ').collect();
                if parts.len() >= 2 && parts[0] == expected_user && parts[1] == expected_pass {
                    let _ = writeln!(stream, "[0,\"OK\",\"Authenticated\"]");
                    authenticated = true;
                } else {
                    let _ = writeln!(stream, "[-1,\"ERR_AUTH\",\"Invalid credentials\"]");
                    let _ = stream.flush();
                    break;
                }
            } else {
                let _ = writeln!(stream, "[-1,\"ERR_AUTH\",\"LOGIN <user> <pass> required\"]");
                let _ = stream.flush();
                break;
            }
            continue;
        }
        // Panic barrier: a panic in dispatch (e.g. an engine bug like integer
        // div-by-zero in SQF_EVAL) must not escape the per-statement loop.
        // The DB lock is taken inside the closure so its guard is released
        // during the unwind; the poisoned-tolerant lock on later iterations
        // keeps the connection usable. The fixed response leaks no panic
        // message or backtrace to the client.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut db = crate::ffi::DB.lock().unwrap_or_else(|e| e.into_inner());
            dispatch::dispatch_inner(&mut db, trimmed, &[])
        }))
        .unwrap_or_else(|_| error_response(ErrorCode::Internal, "Command failed"));
        let _ = writeln!(stream, "{}", result);
    }
}

/// Start a TCP server on `bind:port`. Each client gets a thread.
/// Pass `db_path` for persistence (loads on start, saves on writes).
/// This is the shared entry point used by both the extension's `listen` command
/// and the standalone `a3sql-server` binary.
pub fn start_server(bind: &str, port: u16, db_path: Option<&str>) -> Result<String, String> {
    let addr = format!("{}:{}", bind, port);

    if let Some(path) = db_path {
        // Load existing database if file exists
        let mut db = crate::ffi::DB.lock().unwrap_or_else(|e| e.into_inner());
        let r = dispatch::dispatch_inner(&mut db, &format!("load {}", path), &[]);
        eprintln!("[a3sql-server] Loaded from {}: {}", path, r);
    }

    let listener = try_bind(&addr).map_err(|e| format!("Bind failed: {}", e))?;

    // Register auto-save on SIGTERM for persistence
    if let Some(path) = db_path {
        let path = path.to_string();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let mut db = crate::ffi::DB.lock().unwrap_or_else(|e| e.into_inner());
            let r = dispatch::dispatch_inner(&mut db, &format!("save {}", path), &[]);
            if r.contains("ERR") {
                eprintln!("[a3sql-server] auto-save: {}", r);
            }
        });
    }

    *LISTENER.lock().unwrap() = Some(listener.try_clone().map_err(|e| e.to_string())?);

    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => break,
            };
            std::thread::spawn(|| serve_client(stream));
        }
    });

    Ok(addr)
}

fn try_bind(addr: &str) -> Result<std::net::TcpListener, String> {
    let mut last_err = String::new();
    for i in 0..6 {
        match std::net::TcpListener::bind(addr) {
            Ok(l) => return Ok(l),
            Err(e) => {
                last_err = format!("Bind failed: {}", e);
                if i < 5 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
    Err(last_err)
}

#[cfg(test)]
mod tests {
    use super::serve_client;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Drive a real `serve_client` connection with one line per statement.
    /// Half-closes the write side after sending (so the server loop sees EOF),
    /// then collects every response line until the server closes the
    /// connection. `join().unwrap()` re-propagates any panic from the server
    /// thread into the test — so a crashing dispatch fails the test with the
    /// original panic message.
    fn serve_lines(lines: &[&str]) -> Vec<String> {
        use std::net::Shutdown;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            serve_client(stream);
        });
        let client = std::net::TcpStream::connect(addr).unwrap();
        let mut writer = client.try_clone().unwrap();
        for line in lines {
            writeln!(writer, "{}", line).unwrap();
        }
        writer.flush().unwrap();
        client.shutdown(Shutdown::Write).unwrap(); // server loop reads EOF
        drop(writer);

        let mut reader = BufReader::new(client);
        let mut responses = Vec::new();
        let mut buf = String::new();
        while reader.read_line(&mut buf).unwrap() > 0 {
            responses.push(buf.trim().to_string());
            buf.clear();
        }
        server.join().unwrap();
        responses
    }

    fn is_envelope(resp: &str) -> bool {
        resp.starts_with("[0,\"OK\",") || resp.starts_with("[-1,\"")
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real TCP sockets are blocked by miri's isolation
    fn adversarial_inputs_return_envelopes_and_connection_survives() {
        let mut cases = vec![
            "SELECT admin_xor_key FROM auth_keys",
            "SELECT 'abc",
            "'",
            "''''''",
            "SELECT * FROM no_such_table",
            "SELECT ((((((((1))))))))",
            "NULL",
            "SELECT SQF_EVAL('1/0')",
            "SELECT SQF_EVAL('%0')",
        ];
        let big = format!("SELECT {}", "1".repeat(100_000));
        cases.push(big.as_str());

        let responses = serve_lines(&cases);
        assert_eq!(
            responses.len(),
            cases.len(),
            "server must answer every line without dying; got: {:?}",
            responses
        );
        for (input, resp) in cases.iter().zip(&responses) {
            assert!(
                is_envelope(resp),
                "input {:?} produced non-envelope response: {}",
                input,
                resp
            );
        }
        assert_eq!(
            responses[7], "[-1,\"ERR_INTERNAL\",\"Command failed\"]",
            "SELECT SQF_EVAL('1/0') must map to a clean internal error, not a crash"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real TCP sockets are blocked by miri's isolation
    fn empty_line_is_skipped_without_response() {
        let responses = serve_lines(&["", "PING"]);
        assert_eq!(responses, vec!["[0,\"OK\",\"PONG\"]"]);
    }
}
