//! End-to-end transport bounds (TRANS-01, TRANS-02).
//!
//! These drive the real binary over a socket, so they prove the limits hold at
//! the transport seam rather than only in a helper. None of them needs a token:
//! an over-limit frame and an over-limit payload are refused before any device is
//! touched, and the connection cap is purely a socket-level property.

mod session_support;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use session_support::{connect, request, send_and_receive, start_service};

/// Larger than `MAX_WS_MESSAGE_BYTES` (1 MiB), and larger than the client's own
/// default frame limit? No — the client default is 16 MiB, so the client can send
/// it; the server must reject it.
const OVERSIZED_MESSAGE_BYTES: usize = 2 * 1024 * 1024;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_over_limit_frame_is_rejected_and_the_service_survives() {
    let service = start_service();

    let mut socket = connect(service.port()).await;
    // A syntactically valid request padded past the server's message limit.
    let oversized = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"SetLanguage\",\"params\":[\"{}\"],\"id\":1}}",
        "A".repeat(OVERSIZED_MESSAGE_BYTES)
    );
    let _ = socket.send(Message::Text(oversized)).await;

    // The server must close the conversation rather than serve it. Any of
    // None / Err / a Close frame is acceptable; a normal response is not.
    let outcome = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
    if let Ok(Some(Ok(Message::Text(text)))) = outcome {
        panic!(
            "an over-limit frame must not be served, got {} bytes",
            text.len()
        )
    }
    drop(socket);

    // The rejection must not have taken the service down.
    let mut next = connect(service.port()).await;
    let response = send_and_receive(&mut next, &request("SetLanguage", json!(["EN"]), 2)).await;
    assert_eq!(
        response["error"], 0,
        "the service must remain responsive after rejecting a frame"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_payload_over_256kib_is_rejected() {
    let service = start_service();

    // 400_000 raw bytes encode to ~533_000 characters: past the 256 KiB decoded
    // limit, comfortably inside the 1 MiB frame limit.
    use base64::Engine as _;
    let big_payload = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 400_000]);

    let mut socket = connect(service.port()).await;
    let response = send_and_receive(
        &mut socket,
        &request(
            "ImportCertificate",
            json!([
                "GM3000",
                "dev-placeholder",
                "app-placeholder",
                "cnt-placeholder",
                true,
                big_payload
            ]),
            3,
        ),
    )
    .await;

    assert_eq!(
        response["error"], -2,
        "an over-limit payload must be a -2 rejection, got {}",
        response
    );
    let message = response["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("Payload too large"),
        "the message must name the limit, got {:?}",
        message
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connections_beyond_64_are_refused() {
    let service = start_service();

    // Hold 64 connections open. `connect` waits for the WebSocket handshake, so
    // each successful call means the server admitted the connection and holds a
    // permit for it.
    let mut held = Vec::new();
    for id in 0..64 {
        let mut socket = connect(service.port()).await;
        // Prove the connection is live and served.
        let response =
            send_and_receive(&mut socket, &request("SetLanguage", json!(["EN"]), id)).await;
        assert_eq!(response["error"], 0);
        held.push(socket);
    }

    // The 65th must be refused before the handshake completes.
    let url = format!("ws://127.0.0.1:{}", service.port());
    let overflow = tokio_tungstenite::connect_async(&url).await;
    assert!(
        overflow.is_err(),
        "the 65th concurrent connection must be refused, got {:?}",
        overflow.map(|_| "a live connection")
    );

    // Freeing one slot admits a new connection.
    held.pop();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut fresh = connect(service.port()).await;
    let response = send_and_receive(&mut fresh, &request("SetLanguage", json!(["EN"]), 99)).await;
    assert_eq!(response["error"], 0, "a freed slot must be reusable");
}
