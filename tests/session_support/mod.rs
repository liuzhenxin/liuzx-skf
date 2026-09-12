#![allow(dead_code)]

//! Shared harness for the session isolation and lifecycle integration tests.
//!
//! These tests drive the **real binary** over a WebSocket, for the same reason
//! `contract_fixtures.rs` does: the JSON-RPC dispatcher lives in the binary crate,
//! so only a subprocess exercises it end to end. They are deliberately modest
//! because the machine has no token attached: every operation that would need a
//! native handle takes its rejection path, and the token-dependent behaviour is
//! recorded in `02-HUMAN-UAT.md` instead.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;

/// How long to wait for the service to announce its port.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

/// A running service process. Killed on drop so a failing assertion cannot leak it.
pub struct ServiceProcess {
    child: Child,
    port: u16,
}

impl ServiceProcess {
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ServiceProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn parse_ws_port(line: &str) -> Option<u16> {
    let marker = "SKF Service listening on ws://";
    let rest = line.split(marker).nth(1)?;
    rest.trim().rsplit(':').next()?.parse::<u16>().ok()
}

/// Start the built binary on an ephemeral port and wait for it to report the port.
pub fn start_service() -> ServiceProcess {
    start_service_with_env(&[])
}

/// Start the service with extra environment overrides.
///
/// Used by tests that need to shorten a bound (for example
/// `SKF_FFI_TIMEOUT_SECONDS`) without a rebuild.
pub fn start_service_with_env(extra_env: &[(&str, &str)]) -> ServiceProcess {
    let exe = env!("CARGO_BIN_EXE_skf-service");
    let mut command = Command::new(exe);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SKF_WS_ADDR", "127.0.0.1:0")
        .env("SKF_HTTP_ADDR", "127.0.0.1:0");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let mut child = command
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

/// Open a WebSocket connection to the service.
pub async fn connect(
    port: u16,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://127.0.0.1:{}", port);
    let (socket, _) = tokio_tungstenite::connect_async(&url)
        .await
        .unwrap_or_else(|e| panic!("connect to {}: {}", url, e));
    socket
}

/// Send one JSON-RPC request and read one response.
pub async fn send_and_receive<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    request: &Value,
) -> Value
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(Message::Text(request.to_string()))
        .await
        .expect("send");

    loop {
        let message = tokio::time::timeout(Duration::from_secs(20), socket.next())
            .await
            .expect("timed out waiting for a response")
            .expect("connection closed before a response arrived")
            .expect("receive");
        if let Message::Text(text) = message {
            return serde_json::from_str(&text).expect("valid JSON");
        }
    }
}

/// Build a JSON-RPC request with an integer id.
pub fn request(method: &str, params: Value, id: i64) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id,
    })
}
