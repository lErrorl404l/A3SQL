// a3sql-server — standalone database server for a3sql
//
// Thin CLI wrapper around a3sql::start_server(). Shares ALL features with
// the in-game extension: LOGIN auth, PING, DESCRIBE, multi-client TCP, etc.
//
// Usage:
//   a3sql-server                  # port 33306, localhost, in-memory
//   a3sql-server --port 33307     # custom port
//   a3sql-server --bind 0.0.0.0   # network-accessible
//   a3sql-server --db data.bin    # persistent (auto-save every 30s)
//   a3sql-server --interactive    # interactive REPL mode
//   a3sql-server --help           # options

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn main() {
    // Replace the default panic hook: panics recovered by the catch_unwind
    // barriers still trigger the hook, and the default "thread 'main'
    // panicked at ... / run with RUST_BACKTRACE" noise is not a clean
    // operator-facing signal. Keep the message + location for debugging.
    std::panic::set_hook(Box::new(|info| {
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic payload".to_string());
        let loc = info
            .location()
            .map(|l| format!(" at {}:{}", l.file(), l.line()))
            .unwrap_or_default();
        eprintln!("[a3sql] internal error: {}{}", msg, loc);
    }));

    let args: Vec<String> = std::env::args().collect();
    let mut port = 33306u16;
    let mut bind = "127.0.0.1".to_string();
    let mut db_path: Option<String> = None;
    let mut interactive = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                i += 1;
                port = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(33306);
            }
            "--bind" | "-b" => {
                i += 1;
                bind = args.get(i).cloned().unwrap_or("127.0.0.1".into());
            }
            "--db" | "-d" => {
                i += 1;
                db_path = args.get(i).cloned();
            }
            "--interactive" | "-i" => {
                interactive = true;
            }
            "--help" | "-h" => {
                eprintln!("a3sql-server [OPTIONS]");
                eprintln!("  --port, -p <PORT>   TCP port (default: 33306)");
                eprintln!("  --bind, -b <IP>     Bind address (default: 127.0.0.1)");
                eprintln!("  --db, -d <PATH>     Persist database to file (auto-save)");
                eprintln!("  --interactive, -i   Interactive REPL mode");
                return;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Register Ctrl+C handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
        eprintln!("\nShutting down...");
    })
    .expect("Error setting Ctrl+C handler");

    match a3sql::start_server(&bind, port, db_path.as_deref()) {
        Ok(addr) => {
            eprintln!("a3sql-server v{} on {}", env!("CARGO_PKG_VERSION"), addr);
            if let Some(ref p) = db_path {
                eprintln!("  persist: {} (auto-save every 30s)", p);
            }

            if interactive {
                repl(&running);
            } else {
                // Wait for shutdown signal
                while running.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }

            // Graceful shutdown
            graceful_shutdown(&db_path);
        }
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}

/// Interactive REPL — read SQL from stdin, dispatch, print results.
fn repl(running: &AtomicBool) {
    use std::io::Write;

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    eprintln!("Interactive REPL — type SQL or commands (:help for list)");

    loop {
        let _ = write!(stdout, "a3sql> ");
        let _ = stdout.flush();

        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => break,                                      // EOF
            Err(_) if !running.load(Ordering::Relaxed) => break, // Ctrl+C
            Err(_) => break,
            Ok(_) => {}
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // REPL meta-commands
        let lower = trimmed.to_lowercase();
        match trimmed {
            ":exit" | ":quit" | ":q" => break,
            ":help" | ":h" => {
                eprintln!("Commands:");
                eprintln!("  :exit, :quit, :q  Exit the REPL");
                eprintln!("  :help, :h         Show this help");
                eprintln!("  Any SQL statement or server command (PING, SAVE, LOAD, etc.)");
                eprintln!("  is executed directly via dispatch().");
                continue;
            }
            _ => {
                if lower == "exit" || lower == "quit" {
                    break;
                }
            }
        }

        // Panic barrier: a panic in dispatch must not abort the REPL. Respond
        // with a clean internal error and keep reading lines. The fixed
        // response leaks no panic message or backtrace to the console.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a3sql::dispatch(trimmed, &[])))
            .unwrap_or_else(|_| "[-1,\"ERR_INTERNAL\",\"Command failed\"]".to_string());
        println!("{}", result);
    }
}

/// Stop the TCP listener and persist database if configured.
fn graceful_shutdown(db_path: &Option<String>) {
    // Stop accepting new TCP connections
    a3sql::dispatch("stop", &[]);

    // Final save if persistence is enabled
    if let Some(ref path) = db_path {
        eprintln!("Saving database to {}...", path);
        let r = a3sql::dispatch(&format!("save {}", path), &[]);
        if r.contains("ERR") {
            eprintln!("Save failed: {}", r);
        } else {
            eprintln!("Database saved.");
        }
    }

    eprintln!("Goodbye.");
}
