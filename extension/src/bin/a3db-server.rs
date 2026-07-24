// a3db-server — standalone database server for a3db
//
// Thin CLI wrapper around a3db::start_server(). Shares ALL features with
// the in-game extension: LOGIN auth, PING, DESCRIBE, multi-client TCP, etc.
//
// Usage:
//   a3db-server                  # port 33306, localhost, in-memory
//   a3db-server --port 33307     # custom port
//   a3db-server --bind 0.0.0.0   # network-accessible
//   a3db-server --db data.bin    # persistent (auto-save every 30s)
//   a3db-server --help           # options

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut port = 33306u16;
    let mut bind = "127.0.0.1".to_string();
    let mut db_path: Option<String> = None;

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
            "--help" | "-h" => {
                eprintln!("a3db-server [OPTIONS]");
                eprintln!("  --port, -p <PORT>   TCP port (default: 33306)");
                eprintln!("  --bind, -b <IP>     Bind address (default: 127.0.0.1)");
                eprintln!("  --db, -d <PATH>     Persist database to file (auto-save)");
                return;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    match a3db::start_server(&bind, port, db_path.as_deref()) {
        Ok(addr) => {
            eprintln!("a3db-server v{} on {}", env!("CARGO_PKG_VERSION"), addr);
            if let Some(ref p) = db_path {
                eprintln!("  persist: {} (auto-save every 30s)", p);
            }
            // Block forever — server runs on background threads
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}
