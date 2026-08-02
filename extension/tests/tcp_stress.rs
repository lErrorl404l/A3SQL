// a3sql — TCP concurrency stress test
//
// Proves the multi-client TCP server (thread-per-client, all statements
// serialized on the global DB Mutex) is safe under concurrent load:
//
//   - N clients × M ops each: INSERT own-prefix rows + UPDATE a shared counter
//   - Per-client `SELECT COUNT(*) WHERE client_id = 'c{i}'` == M
//     (no lost writes, no cross-client corruption)
//   - Shared counter reaches exactly N*M via `UPDATE t SET n = n + 1`
//     (single statement → read-modify-write happens inside the mutex, so it
//     IS atomic — unlike a client-side SELECT-then-UPDATE round trip)
//   - Zero error envelopes, clean QUIT, server alive and responsive at the end
//
// Plus a connection-churn variant: many short-lived connections hammering the
// thread-per-client spawn/teardown path.
//
// This lives in its own integration-test binary so the process-local
// CONFIG/CREDENTIALS statics (auth defaults, $A3SQL_CONFIG) are isolated from
// the other test binaries, mirroring auth_default.rs.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

const N_CLIENTS: usize = 8;
const OPS_PER_CLIENT: usize = 250; // N*M = 2000 statements ≥ 2000 required
const CHURN_CONNECTIONS: usize = 64;

// ── Test server with TCP auth disabled ──────────────────────────────────────

/// Server under test. TCP auth is fail-closed by default, so this test
/// explicitly opts out via `listener_require_auth = false` in a config file
/// pointed to by `$A3SQL_CONFIG` — the same mechanism the a3sql-server binary
/// uses. CONFIG is a process-wide LazyLock: the env var must be set BEFORE any
/// a3sql call in this process, so it is set in `start()` before the server
/// thread touches `listener_auth_required()` on the first connection.
struct TestServer {
    port: u16,
    config_path: std::path::PathBuf,
}

impl TestServer {
    fn start() -> Self {
        let config_path = std::env::temp_dir().join(format!("a3sql_tcp_stress_{}.toml", std::process::id()));
        std::fs::write(&config_path, "listener_require_auth = false\n").unwrap();
        // SAFETY: called at test start before any thread exists and before any
        // other a3sql call; no concurrent env access can occur (the std
        // contract for set_var is: no other thread may read/modify the env
        // concurrently, which holds here).
        unsafe { std::env::set_var("A3SQL_CONFIG", &config_path) };

        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        std::thread::spawn(move || {
            let _ = a3sql::start_server("127.0.0.1", port, None);
        });
        // Let the listener thread accept (same pacing as auth_default.rs).
        std::thread::sleep(Duration::from_millis(400));
        TestServer { port, config_path }
    }

    fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        // Drop the process-global listener clone; the accept loop keeps its own
        // clone alive until the process exits, so this only stops new state.
        let _ = a3sql::dispatch("stop", &[]);
        let _ = std::fs::remove_file(&self.config_path);
    }
}

// ── Line-based protocol client ──────────────────────────────────────────────

struct TcpClient {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl TcpClient {
    fn connect(addr: &str) -> Self {
        // Retry briefly — the accept loop may still be binding.
        let mut stream = None;
        for _ in 0..50 {
            match TcpStream::connect(addr) {
                Ok(s) => {
                    stream = Some(s);
                    break;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        let stream = stream.unwrap_or_else(|| panic!("cannot connect to test server at {}", addr));
        // A hung server must fail the test, not hang it.
        stream.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
        // Nagle/delayed-ACK on loopback inflates each request/response round
        // trip to ~80ms with tiny line-protocol messages; the real a3sql
        // clients (SQF / external tools) disable it too. Without this the
        // stress test would take ~40s instead of <1s for the same statements.
        stream.set_nodelay(true).unwrap();
        let writer = stream.try_clone().unwrap();
        TcpClient {
            reader: BufReader::new(stream),
            writer,
        }
    }

    /// Send one line, read the one response line. Every non-empty statement
    /// produces exactly one envelope line, so request/response is strictly
    /// sequential per connection.
    fn cmd(&mut self, line: &str) -> String {
        self.writer.write_all(line.as_bytes()).unwrap();
        self.writer.write_all(b"\n").unwrap();
        self.writer.flush().unwrap();
        let mut resp = String::new();
        let n = self.reader.read_line(&mut resp).unwrap();
        assert!(n > 0, "server closed connection on: {}", line);
        resp.trim().to_string()
    }

    fn quit(mut self) {
        let _ = self.writer.write_all(b"QUIT\n");
    }
}

// ── Assertions ─────────────────────────────────────────────────────────────

/// Fail if the response is an error envelope — the "zero errors" invariant.
fn expect_ok(what: &str, resp: &str) {
    assert!(resp.starts_with("[0,\"OK\","), "{} got error envelope: {}", what, resp);
}

/// Extract the single scalar cell of the last row from a SELECT payload like
/// `[0,"OK",[["count(*)"],[128]]]` or `[0,"OK",[["n"],[2000]]]`.
fn last_cell_int(resp: &str) -> i64 {
    let body = resp.strip_prefix("[0,\"OK\",").expect("not an OK envelope");
    let start = body.rfind('[').expect("no row array in response");
    let rest = &body[start + 1..];
    let end = rest.find(']').expect("unterminated row array");
    rest[..end]
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("cell {:?} is not an int", &rest[..end]))
}

// ── Workloads ──────────────────────────────────────────────────────────────

/// One client: M inserts of unique own-prefix rows + M counter increments,
/// then counts its own rows back. Returns on success; panics on any error
/// envelope or count mismatch (re-propagated by the test via join).
fn worker(id: usize, addr: &str, ops: usize) {
    let mut c = TcpClient::connect(addr);
    let grp = format!("c{}", id);
    for i in 0..ops {
        let ins = c.cmd(&format!(
            "INSERT INTO stress_rows VALUES ('c{}_r{}', 'c{}', {})",
            id, i, id, i
        ));
        expect_ok(&format!("client {} insert", id), &ins);
        let upd = c.cmd("UPDATE stress_counter SET n = n + 1 WHERE id = 'total'");
        expect_ok(&format!("client {} counter update", id), &upd);
    }
    // No lost writes: every row this client inserted is present.
    let cnt = c.cmd(&format!("SELECT COUNT(*) FROM stress_rows WHERE client_id = '{}'", grp));
    expect_ok(&format!("client {} count", id), &cnt);
    assert_eq!(last_cell_int(&cnt), ops as i64, "client {} lost rows: {}", id, cnt);
    c.quit();
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
#[cfg_attr(miri, ignore)] // real TCP sockets are blocked by miri's isolation
fn concurrent_clients_serialize_statements() {
    let srv = TestServer::start();
    let addr = srv.addr();

    // Setup through the real TCP path: shared tables + seeded counter row.
    let mut setup = TcpClient::connect(&addr);
    expect_ok(
        "create stress_rows",
        &setup.cmd("CREATE TABLE stress_rows (id STRING PRIMARY KEY, client_id STRING, v INT)"),
    );
    expect_ok(
        "create stress_counter",
        &setup.cmd("CREATE TABLE stress_counter (id STRING PRIMARY KEY, n INT)"),
    );
    expect_ok(
        "seed counter",
        &setup.cmd("INSERT INTO stress_counter VALUES ('total', 0)"),
    );
    setup.quit();

    // N concurrent clients, each doing OPS_PER_CLIENT inserts + increments.
    let handles: Vec<_> = (0..N_CLIENTS)
        .map(|id| {
            let addr = addr.clone();
            std::thread::spawn(move || worker(id, &addr, OPS_PER_CLIENT))
        })
        .collect();
    for h in handles {
        h.join().expect("client thread panicked");
    }

    // Global invariants, read back over a fresh connection.
    let mut check = TcpClient::connect(&addr);
    let total_rows = check.cmd("SELECT COUNT(*) FROM stress_rows");
    expect_ok("total rows", &total_rows);
    assert_eq!(
        last_cell_int(&total_rows),
        (N_CLIENTS * OPS_PER_CLIENT) as i64,
        "row count drifted under concurrency: {}",
        total_rows
    );
    // The atomic counter: `n = n + 1` inside a single statement under the
    // mutex must lose no update — exactly N*M increments applied.
    let counter = check.cmd("SELECT n FROM stress_counter WHERE id = 'total'");
    expect_ok("counter read", &counter);
    assert_eq!(
        last_cell_int(&counter),
        (N_CLIENTS * OPS_PER_CLIENT) as i64,
        "shared counter lost updates: {}",
        counter
    );

    // Server survived the whole run and still answers.
    expect_ok("final ping", &check.cmd("PING"));
    check.quit();
}

#[test]
#[cfg_attr(miri, ignore)] // real TCP sockets are blocked by miri's isolation
fn connection_churn_does_not_break_server() {
    let srv = TestServer::start();
    let addr = srv.addr();

    let mut setup = TcpClient::connect(&addr);
    expect_ok(
        "create churn_rows",
        &setup.cmd("CREATE TABLE churn_rows (id STRING PRIMARY KEY, v INT)"),
    );
    setup.quit();

    // Rapid connect → one statement → QUIT, from many threads at once:
    // exercises thread-per-client spawn + teardown under contention.
    let handles: Vec<_> = (0..CHURN_CONNECTIONS)
        .map(|i| {
            let addr = addr.clone();
            std::thread::spawn(move || {
                let mut c = TcpClient::connect(&addr);
                let r = c.cmd(&format!("INSERT INTO churn_rows VALUES ('x{}', {})", i, i));
                expect_ok("churn insert", &r);
                c.quit();
            })
        })
        .collect();
    for h in handles {
        h.join().expect("churn client thread panicked");
    }

    // Every churn connection's write landed; the server is still healthy.
    let mut check = TcpClient::connect(&addr);
    let cnt = check.cmd("SELECT COUNT(*) FROM churn_rows");
    expect_ok("churn count", &cnt);
    assert_eq!(
        last_cell_int(&cnt),
        CHURN_CONNECTIONS as i64,
        "churn connection lost a write: {}",
        cnt
    );
    check.quit();
}
