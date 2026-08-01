// Fail-closed TCP auth default — regression proof for the S2 hardening wave.
//
// With no a3sql.toml (Config::default()) and no credentials configured, the
// TCP listener must refuse every connection with the clear "No credentials
// configured" message. It must never fall open to anonymous access.
//
// This lives in its own integration-test binary so CREDENTIALS is process-
// local here: tests in other binaries that call `set_credentials` (gaps.rs,
// test_saveload.rs) cannot race this one.

use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};

#[test]
#[cfg_attr(miri, ignore)] // real TCP sockets are blocked by miri's isolation
fn listener_refuses_without_credentials_by_default() {
    // A fresh process: CREDENTIALS is empty and no a3sql.toml exists in the
    // test CWD, so CONFIG.listener_auth_required() defaults to true.
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let addr = format!("127.0.0.1:{}", port);
    std::thread::spawn(move || {
        let _ = a3sql::start_server("127.0.0.1", port, None);
    });
    std::thread::sleep(std::time::Duration::from_millis(400));

    // A non-LOGIN first line is refused with the auth-required message.
    if let Ok(mut stream) = TcpStream::connect(&addr) {
        writeln!(stream, "SELECT 1").ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut resp = String::new();
        std::io::BufReader::new(&mut stream).read_line(&mut resp).ok();
        assert!(resp.contains("LOGIN"), "auth required by default: {}", resp);
    }

    // Even a LOGIN attempt cannot succeed with no credentials configured —
    // the server says so plainly instead of accepting or hanging.
    if let Ok(mut stream) = TcpStream::connect(&addr) {
        writeln!(stream, "LOGIN admin secret").ok();
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut resp = String::new();
        std::io::BufReader::new(&mut stream).read_line(&mut resp).ok();
        assert!(
            resp.contains("No credentials configured"),
            "fail-closed message expected: {}",
            resp
        );
    }
}
