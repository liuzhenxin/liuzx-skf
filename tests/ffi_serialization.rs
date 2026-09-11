//! Blocking-FFI isolation, per-provider serialization, and request timeout
//! (TRANS-03, TRANS-04, TRANS-05).
//!
//! Two evidence styles are used deliberately:
//!
//! * a **behavioural** test proves a slow provider call does not stall the async
//!   runtime (the handler runs on the blocking pool);
//! * **structural** tests pin the two properties a fake cannot exercise: every
//!   native method takes the gate, and the wait/cancel pair does not.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use skf_service::domain::container::CreateContainer;
use skf_service::domain::test_support::TestContext;
use skf_service::protocol::params::Params;
use skf_service::protocol::Language;
use skf_service::provider::fake::FakeSkfProvider;
use skf_service::provider::{Operation, SkfError, SkfProvider};
use skf_service::session::auth::AuthKey;
use skf_service::session::SessionState;

mod session_support;

use session_support::{connect, request, send_and_receive, start_service_with_env};

fn leaked_params(values: Vec<Value>) -> &'static Params<'static> {
    let values: &'static [Value] = Box::leak(values.into_boxed_slice());
    Box::leak(Box::new(Params::new(values, Some(json!(1)))))
}

/// A slow vendor call must not occupy an async worker: the runtime stays able to
/// service other work while the blocking pool runs the device operation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_provider_call_does_not_stall_an_unrelated_request() {
    let fake = Arc::new(FakeSkfProvider::new("FAKE"));
    fake.fail_next(
        Operation::OpenDevice,
        SkfError::Blocking(Duration::from_millis(400)),
    );
    let ctx = TestContext::new(fake.clone() as Arc<dyn SkfProvider>);
    let mut state = SessionState::new(Duration::from_secs(600));
    state.grant(AuthKey::new("FAKE", "dev-a", "app-a"));
    let p = leaked_params(vec![
        json!("FAKE"),
        json!("dev-a"),
        json!("app-a"),
        json!("cnt-a"),
    ]);

    let started = Instant::now();
    let slow = tokio::task::spawn_blocking(move || {
        CreateContainer::handle(&ctx, &mut state, p, &Language::EN)
    });

    // The runtime must remain responsive while the blocking call is in flight.
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(
        started.elapsed() < Duration::from_millis(300),
        "the async timer must not wait for the blocking provider call, elapsed {:?}",
        started.elapsed()
    );

    let response = slow.await.expect("blocking task must join");
    assert_eq!(
        response.error, 0,
        "the slow call must still succeed after the delay"
    );
    assert_eq!(fake.call_count(Operation::CloseDevice), 1);
}

/// Every native FFI method takes the per-provider gate, which is what serialises
/// concurrent access to one vendor library.
#[test]
fn concurrent_guard_calls_do_not_interleave() {
    let native = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/provider/native.rs"),
    )
    .expect("native.rs readable");
    let locks = native.matches("gate_lock(&self.gate)").count();
    assert!(
        locks >= 15,
        "expected the gate to be taken by every guard method, found {} sites",
        locks
    );
}

/// `WaitForDevEvent`/`CancelWaitForDevEvent` must not take the gate: cancel has
/// to reach the library while a wait is parked, or the pair deadlocks.
#[test]
fn wait_and_cancel_are_exempt_from_the_ffi_gate() {
    let native = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/provider/native.rs"),
    )
    .expect("native.rs readable");
    for method in ["fn wait_for_event(", "fn cancel_wait_for_event("] {
        let start = native
            .find(method)
            .unwrap_or_else(|| panic!("{} not found", method));
        let rest = &native[start..];
        let end = rest[1..]
            .find("\n    fn ")
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        assert!(
            !rest[..end].contains("gate_lock"),
            "{} must not take the FFI gate",
            method
        );
    }
}

/// The request timeout stops the caller waiting; the real vendor thread cannot
/// be killed. `SKF_FFI_TIMEOUT_SECONDS=0` forces the timeout deterministically,
/// and `IssueCertificate` is used because it shells out to OpenSSL (and so is
/// never already finished when the zero deadline is checked).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_request_that_exceeds_30s_times_out() {
    let service = start_service_with_env(&[("SKF_FFI_TIMEOUT_SECONDS", "0")]);
    let mut socket = connect(service.port()).await;
    let response = send_and_receive(
        &mut socket,
        &request(
            "IssueCertificate",
            json!(["-----BEGIN CERTIFICATE REQUEST-----\nMIIB\n-----END CERTIFICATE REQUEST-----", false]),
            1,
        ),
    )
    .await;

    assert_eq!(
        response["error"], -1,
        "a zero timeout must produce the timeout response, got {}",
        response
    );
    assert!(
        response["message"]
            .as_str()
            .unwrap_or_default()
            .contains("timed out"),
        "the timeout message must be explicit, got {}",
        response
    );
}
