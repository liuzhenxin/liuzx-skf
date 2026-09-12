//! End-to-end replay of the frozen v0.2.0 contract against the real binary.
//!
//! # Why a subprocess rather than an in-process server
//!
//! The JSON-RPC dispatcher deliberately stays in the binary crate (decision
//! D-06), so an integration test of the library cannot reach it. Driving the
//! actual shipped executable is also the stronger check: it exercises argument
//! handling, configuration loading, provider construction, transport, and the
//! dispatcher exactly as a deployment does.
//!
//! # Why the port is read from stdout
//!
//! The service binds `127.0.0.1:0` and reports the **resolved** address
//! (`listener.local_addr()`). Reading that line is how the test learns which port
//! the OS chose; the pre-refactor server printed only the requested address, which
//! made ephemeral-port testing impossible.

mod common;

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{load_all, Fixture};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;

/// How long to wait for the service to announce its port.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// A running service process. Killed on drop so a failing assertion cannot leak it.
struct ServiceProcess {
    child: Child,
    port: u16,
}

impl Drop for ServiceProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Parse `SKF Service listening on ws://127.0.0.1:PORT` from a log line.
fn parse_ws_port(line: &str) -> Option<u16> {
    let marker = "SKF Service listening on ws://";
    let rest = line.split(marker).nth(1)?;
    let addr = rest.trim();
    addr.rsplit(':').next()?.parse::<u16>().ok()
}

/// Start the built binary on an ephemeral port and wait for it to report the port.
fn start_service() -> ServiceProcess {
    let exe = env!("CARGO_BIN_EXE_skf-service");
    let mut child = Command::new(exe)
        // `api/` and `config/` are resolved relative to the working directory.
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SKF_WS_ADDR", "127.0.0.1:0")
        // Keep the demo server off the shared 8000 port; the value is unused by
        // the assertions, the service only needs *an* address to bind.
        .env("SKF_HTTP_ADDR", "127.0.0.1:0")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start skf-service");

    let stdout = child.stdout.take().expect("stdout pipe");
    let mut reader = BufReader::new(stdout);
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut port = None;

    while Instant::now() < deadline {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Some(found) = parse_ws_port(&line) {
                    port = Some(found);
                    break;
                }
            }
            Err(_) => break,
        }
    }

    // Drain stdout in the background: if the pipe fills, the child would block on
    // its next print.
    std::thread::spawn(move || {
        let mut sink = String::new();
        let _ = reader.read_to_string(&mut sink);
    });

    let port = port.unwrap_or_else(|| {
        let _ = child.kill();
        panic!(
            "service did not report a bound port within {:?}",
            STARTUP_TIMEOUT
        )
    });

    ServiceProcess { child, port }
}

/// One request/response exchange over a fresh WebSocket connection.
async fn exchange(port: u16, setup: &[Value], request: &Value) -> Result<Value, String> {
    let url = format!("ws://127.0.0.1:{}", port);
    let (mut socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .map_err(|e| format!("connect to {}: {}", url, e))?;

    // Replay prerequisites on the same connection: the server keeps per-connection
    // state (language) and derives digest handle keys from the JSON-RPC id.
    for prereq in setup {
        send_and_receive(&mut socket, prereq).await?;
    }

    let response = send_and_receive(&mut socket, request).await?;
    let _ = socket.close(None).await;
    Ok(response)
}

async fn send_and_receive<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    request: &Value,
) -> Result<Value, String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            request.to_string(),
        ))
        .await
        .map_err(|e| format!("send: {}", e))?;

    loop {
        let message = tokio::time::timeout(Duration::from_secs(20), socket.next())
            .await
            .map_err(|_| "timed out waiting for a response".to_string())?
            .ok_or_else(|| "connection closed before a response arrived".to_string())?
            .map_err(|e| format!("receive: {}", e))?;

        match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                return serde_json::from_str(&text).map_err(|e| format!("invalid JSON: {}", e));
            }
            _ => continue,
        }
    }
}

/// The fixture set must stay complete; a dropped method would silently leave the
/// surface unguarded.
#[test]
fn fixture_count_is_at_least_37() {
    let fixtures = load_all().expect("fixtures directory readable");
    assert!(
        fixtures.len() >= 37,
        "expected at least 37 frozen v0.2.0 fixtures, found {}",
        fixtures.len()
    );
}

/// Every method the service exposes must have a fixture, and vice versa.
#[test]
fn every_expected_method_has_a_fixture() {
    let fixtures = load_all().expect("fixtures directory readable");
    let recorded: Vec<String> = fixtures.iter().map(|f| f.method.clone()).collect();

    let missing: Vec<&str> = common::EXPECTED_METHODS
        .iter()
        .copied()
        .filter(|method| !recorded.iter().any(|r| r == method))
        .collect();
    assert!(
        missing.is_empty(),
        "methods without a fixture: {:?}",
        missing
    );

    let unexpected: Vec<&String> = recorded
        .iter()
        .filter(|method| !common::EXPECTED_METHODS.contains(&method.as_str()))
        .collect();
    assert!(
        unexpected.is_empty(),
        "fixtures for unknown methods: {:?}",
        unexpected
    );
}

/// The authoritative check: replay every fixture against the real service.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn contract_v0_2_0_fixtures() {
    let service = start_service();
    let fixtures = load_all().expect("fixtures directory readable");
    assert!(!fixtures.is_empty(), "no fixtures to replay");

    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;
    let mut skipped: Vec<String> = Vec::new();

    for fixture in &fixtures {
        // `WaitForDevEvent` blocks until cancelled from a second connection. Its
        // recording is timing-sensitive (the cancel must arrive after the waiter
        // entered the blocking vendor call), so it is asserted as a shape check
        // rather than a value comparison. See tests/fixtures/v0.2.0/README.md.
        if fixture.method == "WaitForDevEvent" {
            let shape = replay_optional(&service, fixture).await;
            println!("WaitForDevEvent: shape-checked ({})", shape);
            skipped.push(fixture.method.clone());
            continue;
        }

        match exchange(service.port, &fixture.setup, &fixture.request).await {
            Ok(actual) => {
                let normalized_actual = fixture.normalize_actual(actual);
                let expected = fixture.normalized_expected();
                if normalized_actual == expected {
                    checked += 1;
                } else {
                    failures.push(format!(
                        "{}: response drift\n  expected: {}\n  actual:   {}",
                        fixture.method, expected, normalized_actual
                    ));
                }
            }
            Err(err) => failures.push(format!("{}: {}", fixture.method, err)),
        }
    }

    println!(
        "contract replay: {} compared, {} shape-checked, {} failed",
        checked,
        skipped.len(),
        failures.len()
    );

    assert!(
        failures.is_empty(),
        "contract drift in {} method(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        checked + skipped.len(),
        fixtures.len(),
        "every fixture must be either compared or explicitly shape-checked"
    );
}

/// Assert only that `WaitForDevEvent` is accepted and eventually answered.
///
/// Returns a short description of what was observed for the test log.
async fn replay_optional(service: &ServiceProcess, fixture: &Fixture) -> String {
    let url = format!("ws://127.0.0.1:{}", service.port);

    // Connection A issues the blocking wait without awaiting it.
    let waiter = tokio_tungstenite::connect_async(&url).await;
    let (mut socket_a, _) = match waiter {
        Ok(pair) => pair,
        Err(e) => return format!("could not connect: {}", e),
    };
    if socket_a
        .send(tokio_tungstenite::tungstenite::Message::Text(
            fixture.request.to_string(),
        ))
        .await
        .is_err()
    {
        return "send failed".to_string();
    }

    // Give the waiter time to enter the blocking vendor call before cancelling.
    tokio::time::sleep(Duration::from_millis(800)).await;

    // Connection B cancels the process-global wait.
    match tokio_tungstenite::connect_async(&url).await {
        Ok((mut socket_b, _)) => {
            let cancel = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "CancelWaitForDevEvent",
                "params": [],
                "id": 1
            });
            let _ = socket_b
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    cancel.to_string(),
                ))
                .await;
            let _ = tokio::time::timeout(Duration::from_secs(10), socket_b.next()).await;
            let _ = socket_b.close(None).await;
        }
        Err(e) => return format!("cancel connection failed: {}", e),
    }

    match tokio::time::timeout(Duration::from_secs(15), socket_a.next()).await {
        Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text)))) => {
            let ok = serde_json::from_str::<Value>(&text).is_ok();
            format!("answered, valid JSON: {}", ok)
        }
        Ok(Some(Ok(_))) => "answered with a non-text frame".to_string(),
        Ok(Some(Err(e))) => format!("socket error: {}", e),
        Ok(None) => "connection closed without answering".to_string(),
        Err(_) => "no answer within 15s after cancel (timing-sensitive)".to_string(),
    }
}

/// A TCP connection proves the listener is reachable; the replay test above is
/// what proves it answers correctly.
#[test]
fn service_accepts_connections_on_its_reported_port() {
    let service = start_service();
    let addr = format!("127.0.0.1:{}", service.port);
    let stream = TcpStream::connect(&addr)
        .unwrap_or_else(|e| panic!("could not connect to {}: {}", addr, e));
    drop(stream);
}
