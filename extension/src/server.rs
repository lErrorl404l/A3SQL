// a3sql — TCP server for external query access

//! TCP server — standalone and in-game TCP query interface.
//! Supports LOGIN auth, multi-client, and optional file persistence.

use crate::dispatch;
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

    let has_auth = !expected_user.is_empty() || !expected_pass.is_empty();
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
        let mut db = crate::ffi::DB.lock().unwrap();
        let result = dispatch::dispatch_inner(&mut db, trimmed, &[]);
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
        let mut db = crate::ffi::DB.lock().unwrap();
        let r = dispatch::dispatch_inner(&mut db, &format!("load {}", path), &[]);
        eprintln!("[a3sql-server] Loaded from {}: {}", path, r);
    }

    let listener = try_bind(&addr).map_err(|e| format!("Bind failed: {}", e))?;

    // Register auto-save on SIGTERM for persistence
    if let Some(path) = db_path {
        let path = path.to_string();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let mut db = crate::ffi::DB.lock().unwrap();
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
