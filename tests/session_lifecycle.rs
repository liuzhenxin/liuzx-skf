//! End-to-end session lifecycle: registration on connect, teardown on disconnect
//! (SESS-04, RES-03).
//!
//! # What this can prove without a token
//!
//! Handle release is only observable when a handle exists, and issuing one needs
//! a token. What is asserted here is therefore the lifecycle *contract* at the
//! wire level:
//!
//! * every connection is its own session, so a handle-shaped name does not carry
//!   across a reconnect;
//! * an abrupt disconnect does not wedge the service — a later connection is
//!   served normally.
//!
//! The token-dependent "abandon a digest and assert the device is released" case
//! is covered at the library level by
//! `session::handles::tests::digest_handle_is_released_when_the_session_ends` and
//! at the hardware level by `02-HUMAN-UAT.md`.

mod session_support;

use session_support::{connect, request, send_and_receive, start_service};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_session_is_torn_down_when_its_connection_closes() {
    let service = start_service();

    // First connection: rejected handle, then an abrupt close (no DisConnectDev).
    {
        let mut socket = connect(service.port()).await;
        let response = send_and_receive(
            &mut socket,
            &request("GenerateRandom", serde_json::json!(["dev-1", 8]), 1),
        )
        .await;
        assert_eq!(response["error"], -11);
        drop(socket);
    }

    // A new connection is a fresh session; the same handle-shaped name is still
    // unknown, which is what session ownership guarantees across a reconnect.
    let mut next = connect(service.port()).await;
    let response = send_and_receive(
        &mut next,
        &request("GenerateRandom", serde_json::json!(["dev-1", 8]), 2),
    )
    .await;
    assert_eq!(
        response["error"], -11,
        "a handle must not survive a reconnect, got {}",
        response
    );

    // The service survived the abrupt disconnect and still serves requests.
    let response = send_and_receive(
        &mut next,
        &request("SetLanguage", serde_json::json!(["CN"]), 3),
    )
    .await;
    assert_eq!(response["error"], 0, "the service must remain responsive");
}

/// Many short-lived connections in sequence must not accumulate sessions in a way
/// that blocks service. This is a smoke test that the registry does not keep dead
/// sessions alive (the library-level `registry_does_not_keep_sessions_alive` test
/// proves the ownership model).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_connect_disconnect_keeps_the_service_responsive() {
    let service = start_service();

    for id in 0..10 {
        let mut socket = connect(service.port()).await;
        let response = send_and_receive(
            &mut socket,
            &request("SetLanguage", serde_json::json!(["EN"]), id),
        )
        .await;
        assert_eq!(response["error"], 0);
        drop(socket);
    }

    let mut socket = connect(service.port()).await;
    let response = send_and_receive(
        &mut socket,
        &request("SetLanguage", serde_json::json!(["EN"]), 99),
    )
    .await;
    assert_eq!(response["error"], 0);
}
