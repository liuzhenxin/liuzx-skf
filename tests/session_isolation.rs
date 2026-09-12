//! End-to-end session isolation (SESS-02) and session-owned handles (RES-05).
//!
//! # What this can prove without a token
//!
//! The machine has no GM3000 attached, so no operation can obtain a real native
//! handle and `CheckPIN` cannot succeed. What *can* be proven end to end is the
//! property RES-01/RES-05 exist for: **a client cannot name a resource it was not
//! issued**, on any connection.
//!
//! * a bare integer is rejected rather than converted to a native handle;
//! * a well-formed but never-issued handle string is rejected on a second
//!   connection, which is what session ownership means at the wire level.
//!
//! The adversarial "session A verifies, session B is refused" case needs a token
//! and is recorded in `02-HUMAN-UAT.md`.

mod session_support;

use session_support::{connect, request, send_and_receive, start_service};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sessions_are_isolated_and_never_accept_a_fabricated_handle() {
    let service = start_service();

    // Two connections at the same time: two independent sessions.
    let mut session_a = connect(service.port()).await;
    let mut session_b = connect(service.port()).await;

    // RES-01: a legacy client sends the pointer's decimal string. `GenerateRandom`
    // must not convert it to a device handle.
    let response = send_and_receive(
        &mut session_a,
        &request("GenerateRandom", serde_json::json!(["1", 8]), 1),
    )
    .await;
    assert_eq!(
        response["error"], -11,
        "a bare integer must be rejected as a handle, got {}",
        response
    );

    // RES-05: a handle-shaped string that this session was never issued is
    // unknown, not silently resolved.
    let response = send_and_receive(
        &mut session_b,
        &request("GenerateRandom", serde_json::json!(["dev-1", 8]), 2),
    )
    .await;
    assert_eq!(
        response["error"], -11,
        "a never-issued handle must not resolve, got {}",
        response
    );

    // RES-01 regression: `DisConnectDev` must not treat an integer as a handle.
    // This is the call path that was observed to kill the service process.
    let response = send_and_receive(
        &mut session_a,
        &request("DisConnectDev", serde_json::json!([1]), 3),
    )
    .await;
    assert_eq!(
        response["error"], -11,
        "DisConnectDev must reject a fabricated integer, got {}",
        response
    );

    // The rejection must not have taken the service down: a further request on the
    // same connection still answers.
    let response = send_and_receive(
        &mut session_b,
        &request("DisConnectDev", serde_json::json!([0]), 4),
    )
    .await;
    assert_eq!(response["error"], -11);
}

/// A handle issued by one connection must not be addressable from another. No
/// handle can be issued without a token, so this asserts the negative shape: the
/// same guessed handle is unknown on both connections, and neither connection's
/// state is visible to the other.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_guessed_handle_is_unknown_on_every_connection() {
    let service = start_service();
    let mut first = connect(service.port()).await;
    let mut second = connect(service.port()).await;

    for (socket, id) in [(&mut first, 1), (&mut second, 2)] {
        let response = send_and_receive(
            socket,
            &request("GenerateRandom", serde_json::json!(["hsh-1", 8]), id),
        )
        .await;
        assert_eq!(
            response["error"], -11,
            "a guessed digest handle must be unknown, got {}",
            response
        );
    }
}
