//! Authorization audit events (Phase 9: AUD-01, AUD-02).
//!
//! Every authorization decision funnels through `SessionState`, so the audit can
//! be proven without a USB token by installing a capturing `log::Log` and driving
//! that single decision point. This exercises the real `audit::emit` path — the
//! same one the dispatcher uses — rather than a mock.
//!
//! (A subprocess/stderr variant cannot reach the decision point without hardware:
//! every handler opens the device before it checks authorization, so an absent
//! token fails earlier.)

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use skf_service::session::auth::AuthKey;
use skf_service::session::SessionState;

/// Audit lines captured by [`CaptureLogger`].
static RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static INSTALL: OnceLock<()> = OnceLock::new();
/// Serialises the two tests in this binary so they do not clear each other's records.
static TEST_LOCK: Mutex<()> = Mutex::new(());

struct CaptureLogger;

impl log::Log for CaptureLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if record.level() <= log::Level::Info {
            RECORDS
                .lock()
                .expect("records")
                .push(format!("{}", record.args()));
        }
    }

    fn flush(&self) {}
}

/// Install the capturing logger exactly once for this test process.
fn install_logger() {
    INSTALL.get_or_init(|| {
        // A logger can be set only once; ignore an already-installed env_logger.
        let _ = log::set_boxed_logger(Box::new(CaptureLogger));
        log::set_max_level(log::LevelFilter::Info);
    });
}

/// Drain the captured audit records.
fn take_audit_records() -> Vec<String> {
    let mut guard = RECORDS.lock().expect("records");
    let audit: Vec<String> = guard
        .iter()
        .filter(|line| line.contains("\"audit\":true"))
        .cloned()
        .collect();
    guard.clear();
    audit
}

fn has(records: &[String], needle: &str) -> bool {
    records.iter().any(|line| line.contains(needle))
}

#[test]
fn every_authorization_decision_is_audited() {
    let _serial = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_logger();
    let mut state = SessionState::new(Duration::from_millis(20));
    let key = AuthKey::new("GM3000", "dev-a", "app");

    // granted
    state.grant(key.clone());
    let records = take_audit_records();
    assert!(
        has(&records, "\"event\":\"authorization.granted\""),
        "grant must be audited: {records:?}"
    );

    // denied (target never authorized)
    let other = AuthKey::new("GM3000", "dev-b", "app");
    assert!(state.authorize(&other).is_err());
    let records = take_audit_records();
    assert!(
        has(&records, "\"event\":\"authorization.denied\"")
            && has(&records, "\"reason\":\"not_authorized\""),
        "denial must be audited with its reason: {records:?}"
    );

    // expired
    std::thread::sleep(Duration::from_millis(40));
    assert!(state.authorize(&key).is_err());
    let records = take_audit_records();
    assert!(
        has(&records, "\"event\":\"authorization.expired\""),
        "expiry must be audited: {records:?}"
    );

    // device unavailable
    state.grant(AuthKey::new("GM3000", "dev-c", "app"));
    let _ = take_audit_records();
    assert_eq!(state.invalidate_device("GM3000", "dev-c"), 1);
    let records = take_audit_records();
    assert!(
        has(&records, "\"event\":\"device.unavailable\"") && has(&records, "\"cleared\":1"),
        "device removal must be audited: {records:?}"
    );
}

#[test]
fn audit_records_never_carry_secrets() {
    let _serial = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    install_logger();
    let mut state = SessionState::new(Duration::from_secs(60));
    state.grant(AuthKey::new("GM3000", "dev-a", "app"));
    let records = take_audit_records();
    assert!(!records.is_empty(), "expected at least one audit record");
    for line in records {
        let lower = line.to_ascii_lowercase();
        for forbidden in ["pin", "private_key", "payload", "token", "bearer"] {
            assert!(
                !lower.contains(forbidden),
                "audit record leaked '{forbidden}': {line}"
            );
        }
        assert!(
            !line.contains('\n'),
            "audit record must be one line: {line}"
        );
    }
}
