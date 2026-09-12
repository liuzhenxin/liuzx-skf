//! Restricted-method behaviour (REL-02).
//!
//! `IssueCertificate` and `Transmit` remain implemented and work by default. An
//! operator can make them return a documented error with `SKF_RESTRICT_LEGACY=1`
//! without changing the frozen v0.2.0 contract.

mod session_support;

use serde_json::json;

use session_support::{connect, request, send_and_receive, start_service, start_service_with_env};

const RESTRICTED_ERROR: i64 = -100;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restricted_methods_work_by_default() {
    let service = start_service();
    let mut socket = connect(service.port()).await;
    let response = send_and_receive(
        &mut socket,
        &request("Transmit", json!(["GM3000", "dev-placeholder", "AAAA"]), 1),
    )
    .await;

    assert_ne!(
        response["error"], RESTRICTED_ERROR,
        "restricted methods must not be blocked by default, got {}",
        response
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restricted_methods_are_rejected_when_opted_in() {
    let service = start_service_with_env(&[("SKF_RESTRICT_LEGACY", "1")]);

    let mut socket = connect(service.port()).await;
    let transmitted = send_and_receive(
        &mut socket,
        &request("Transmit", json!(["GM3000", "dev-placeholder", "AAAA"]), 1),
    )
    .await;
    assert_eq!(
        transmitted["error"], RESTRICTED_ERROR,
        "got {}",
        transmitted
    );
    assert!(transmitted["message"]
        .as_str()
        .unwrap_or_default()
        .contains("restricted"));

    let mut socket = connect(service.port()).await;
    let issued = send_and_receive(
        &mut socket,
        &request(
            "IssueCertificate",
            json!([
                "-----BEGIN CERTIFICATE REQUEST-----\nMIIB\n-----END CERTIFICATE REQUEST-----",
                false
            ]),
            2,
        ),
    )
    .await;
    assert_eq!(issued["error"], RESTRICTED_ERROR, "got {}", issued);
    assert!(issued["message"]
        .as_str()
        .unwrap_or_default()
        .contains("restricted"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn non_restricted_methods_are_unaffected() {
    let service = start_service_with_env(&[("SKF_RESTRICT_LEGACY", "1")]);
    let mut socket = connect(service.port()).await;
    let response = send_and_receive(&mut socket, &request("SetLanguage", json!(["EN"]), 1)).await;
    assert_eq!(response["error"], 0, "got {}", response);
}
