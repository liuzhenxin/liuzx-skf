//! Protocol version handshake (REL-01).
//!
//! v0.2.0 clients omit `apiVersion`; such requests must keep working. A client
//! asking for a newer version gets a documented rejection rather than a silently
//! wrong response.

mod session_support;

use serde_json::json;

use session_support::{connect, request, send_and_receive, start_service};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn get_protocol_version_reports_service_version() {
    let service = start_service();
    let mut socket = connect(service.port()).await;
    let response =
        send_and_receive(&mut socket, &request("GetProtocolVersion", json!([]), 1)).await;

    assert_eq!(response["error"], 0, "got {}", response);
    assert_eq!(response["result"]["min"], 1);
    assert_eq!(response["result"]["current"], 1);
    assert_eq!(
        response["result"]["service"], "0.3.0",
        "the reported service version must be the crate version"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_version_absent_is_accepted() {
    let service = start_service();
    let mut socket = connect(service.port()).await;
    let response = send_and_receive(&mut socket, &request("SetLanguage", json!(["EN"]), 1)).await;
    assert_eq!(response["error"], 0, "a v0.2.0 client must keep working");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_version_current_is_accepted() {
    let service = start_service();
    let mut socket = connect(service.port()).await;
    let mut req = request("SetLanguage", json!(["EN"]), 1);
    req["apiVersion"] = json!(1);
    let response = send_and_receive(&mut socket, &req).await;
    assert_eq!(response["error"], 0, "got {}", response);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn api_version_too_new_is_rejected() {
    let service = start_service();
    let mut socket = connect(service.port()).await;
    let mut req = request("SetLanguage", json!(["EN"]), 1);
    req["apiVersion"] = json!(2);
    let response = send_and_receive(&mut socket, &req).await;

    assert_eq!(response["error"], -1, "got {}", response);
    assert!(
        response["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Unsupported apiVersion"),
        "the rejection must be documented, got {}",
        response
    );
}
