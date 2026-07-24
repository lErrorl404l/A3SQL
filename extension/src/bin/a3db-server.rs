// a3db-server — standalone database server for a3db
// cargo run --bin a3db-server -- --port 33307

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

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
                eprintln!("  --db, -d <PATH>     Persist database to file");
                return;
            }
            _ => {
                eprintln!("Unknown option: {}", args[i]);
                std::process::exit(1);
            }
        }
        i += 1;
    }

    if let Some(ref path) = db_path {
        match std::fs::read(path) {
            Ok(_) => {
                let r = a3db::dispatch(&format!("load {}", path), &[]);
                eprintln!("Loaded from {}: {}", path, r);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("Starting fresh (no db at {})", path);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    let addr = format!("{}:{}", bind, port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Bind failed: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!("a3db-server v{} on {}", env!("CARGO_PKG_VERSION"), addr);
    if let Some(ref p) = db_path {
        eprintln!("  persist: {}", p);
    }

    listener.set_nonblocking(true).ok();
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(|| handle(s));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(_) => break,
        }
    }
}

fn handle(mut stream: TcpStream) {
    stream.set_read_timeout(Some(Duration::from_secs(300))).ok();
    let mut r = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        match r.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        match t {
            "PING" => {
                let _ = writeln!(stream, "[0,\"OK\",\"PONG\"]");
            }
            "QUIT" => break,
            _ => {
                let _ = writeln!(stream, "{}", a3db::dispatch(t, &[]));
            }
        }
    }
}
