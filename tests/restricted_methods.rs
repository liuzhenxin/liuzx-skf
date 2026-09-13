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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unrestricted_methods_degrade_when_the_library_is_missing() {
    // Regression: `LockDev`/`UnlockDev`/`Transmit`/`RSAVerify` used to unwrap the
    // library load, so a configured-but-missing library panicked the request task
    // and the dispatch-failure fallback emitted invalid JSON. The response must be
    // the documented `Load Lib Failed` error instead.
    let dir = std::env::temp_dir().join(format!("skf-restricted-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let config = dir.join("skf.yaml");
    std::fs::write(
        &config,
        format!(
            "default: GM3000\nvendor: {{}}\nGM3000:\n  {}: native/definitely-missing-lib\n",
            std::env::consts::OS
        ),
    )
    .expect("write config");

    let service = start_service_with_env(&[("SKF_CONFIG", config.to_str().expect("utf8"))]);
    let mut socket = connect(service.port()).await;
    let response = send_and_receive(
        &mut socket,
        &request("Transmit", json!(["GM3000", "dev-placeholder", "AAAA"]), 1),
    )
    .await;
    assert_eq!(response["error"], -5, "got {}", response);
    assert!(
        response["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Load Lib Failed"),
        "got {}",
        response
    );
}
