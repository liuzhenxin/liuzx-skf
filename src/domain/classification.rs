//! Read-only vs destructive classification for every JSON-RPC method.
//!
//! # Why this exists
//!
//! TRANS-07 requires destructive operations — deleting a container, importing a
//! key or certificate, locking a device, setting a label — to be distinguishable
//! from read-only ones. This module is the single source of truth for that
//! distinction: code, tests and `docs/OPERATION-CLASSIFICATION.md` all derive from
//! [`CLASSIFIED_METHODS`].
//!
//! # This phase classifies only
//!
//! Nothing here is consulted on a request path. A later phase may use the
//! classification to gate authorization, audit logging or operator confirmation;
//! applying it now would change the frozen v0.2.0 behaviour.
//!
//! # Classifying a new method
//!
//! Ask whether the operation can change persisted token/device state, consume a
//! PIN retry, create or import key material, open/close a device, application, or
//! container resource, or change a lock. If any is true it is
//! [`OperationClass::Destructive`]; a pure query or stateless computation is
//! [`OperationClass::ReadOnly`]. Streaming digest handles are session-scoped
//! in-memory state released with the session, so the digest lifecycle is
//! ReadOnly. `tests/classification_invariants.rs` fails if a method is added
//! without a class.

/// How an operation affects the device and its stored state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationClass {
    /// A query or stateless computation. It does not change persisted state.
    ReadOnly,
    /// It changes device/token state, consumes a retry, moves key material,
    /// owns a native resource, or changes a lock.
    Destructive,
}

impl OperationClass {
    /// Stable spelling used by tests and the generated documentation table.
    pub fn as_str(self) -> &'static str {
        match self {
            OperationClass::ReadOnly => "ReadOnly",
            OperationClass::Destructive => "Destructive",
        }
    }
}

/// Every method the service exposes, with its class.
///
/// Kept in sync with `tests/common::EXPECTED_METHODS` by an invariant test.
pub const CLASSIFIED_METHODS: &[(&str, OperationClass)] = &[
    ("SetLanguage", OperationClass::ReadOnly),
    ("WaitForDevEvent", OperationClass::ReadOnly),
    ("EnumProvider", OperationClass::ReadOnly),
    ("EnumDevice", OperationClass::ReadOnly),
    ("ConnectDev", OperationClass::Destructive),
    ("EnumApplication", OperationClass::ReadOnly),
    ("EnumContainer", OperationClass::ReadOnly),
    ("DeleteContainer", OperationClass::Destructive),
    ("IssueCertificate", OperationClass::Destructive),
    ("ImportCertificate", OperationClass::Destructive),
    ("SignData", OperationClass::ReadOnly),
    ("DisConnectDev", OperationClass::Destructive),
    ("FindCertificates", OperationClass::ReadOnly),
    ("GenerateRandom", OperationClass::ReadOnly),
    ("Digest", OperationClass::ReadOnly),
    ("CheckPIN", OperationClass::Destructive),
    ("CreatePKCS10", OperationClass::Destructive),
    ("EncryptData", OperationClass::ReadOnly),
    ("DecryptData", OperationClass::ReadOnly),
    ("GetDevInfo", OperationClass::ReadOnly),
    ("GetDevState", OperationClass::ReadOnly),
    ("SetLabel", OperationClass::Destructive),
    ("ECCVerify", OperationClass::ReadOnly),
    ("CreateContainer", OperationClass::Destructive),
    ("GetContainerType", OperationClass::ReadOnly),
    ("RSASignData", OperationClass::ReadOnly),
    ("LockDev", OperationClass::Destructive),
    ("UnlockDev", OperationClass::Destructive),
    ("Transmit", OperationClass::Destructive),
    ("CancelWaitForDevEvent", OperationClass::ReadOnly),
    ("GenECCKeyPair", OperationClass::Destructive),
    ("GenRSAKeyPair", OperationClass::Destructive),
    ("RSAVerify", OperationClass::ReadOnly),
    ("DigestInit", OperationClass::ReadOnly),
    ("DigestUpdate", OperationClass::ReadOnly),
    ("DigestFinal", OperationClass::ReadOnly),
    ("CloseHash", OperationClass::ReadOnly),
];

/// Methods that are present but may be restricted for safety in a deployment.
///
/// They remain implemented and work by default; an operator can disable them with
/// [`RESTRICT_ENV`] without changing the frozen contract.
pub const RESTRICTED_METHODS: &[&str] = &["IssueCertificate", "Transmit"];

/// Environment variable that turns restricted methods into documented errors.
pub const RESTRICT_ENV: &str = "SKF_RESTRICT_LEGACY";

/// Error code returned for a restricted method when restrictions are enabled.
pub const RESTRICTED_ERROR: i32 = -100;

/// Whether `method` is in the restricted set.
pub fn is_restricted(method: &str) -> bool {
    RESTRICTED_METHODS.contains(&method)
}

/// Build the documented rejection for a restricted method, when enabled.
///
/// Pure so it can be unit-tested without touching the environment: `enabled` is
/// the operator's decision, injected by the caller.
pub fn restricted_rejection(
    method: &str,
    enabled: bool,
    id: Option<serde_json::Value>,
) -> Option<crate::protocol::RpcResponse> {
    if !enabled || !is_restricted(method) {
        return None;
    }
    Some(crate::protocol::RpcResponse::err(
        RESTRICTED_ERROR,
        format!(
            "Method {} is restricted in this deployment; see RELEASE-NOTES.md",
            method
        ),
        id,
    ))
}

/// The class of `method`, or `None` when it is not classified.
pub fn classify(method: &str) -> Option<OperationClass> {
    CLASSIFIED_METHODS
        .iter()
        .find(|(name, _)| *name == method)
        .map(|(_, class)| *class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifying_a_known_method_returns_its_class() {
        assert_eq!(
            classify("DeleteContainer"),
            Some(OperationClass::Destructive)
        );
        assert_eq!(classify("EnumDevice"), Some(OperationClass::ReadOnly));
        assert_eq!(classify("SetLabel"), Some(OperationClass::Destructive));
    }

    #[test]
    fn an_unknown_method_returns_none() {
        assert_eq!(classify("NotAMethod"), None);
    }

    #[test]
    fn the_restricted_set_is_documented_and_small() {
        assert!(is_restricted("IssueCertificate"));
        assert!(is_restricted("Transmit"));
        assert!(!is_restricted("SignData"));
        assert_eq!(RESTRICTED_METHODS.len(), 2);
    }

    #[test]
    fn restricted_rejection_is_inert_unless_enabled() {
        assert!(restricted_rejection("Transmit", false, None).is_none());
        assert!(restricted_rejection("SignData", true, None).is_none());
        let rejection = restricted_rejection("Transmit", true, None).expect("rejected");
        assert_eq!(rejection.error, RESTRICTED_ERROR);
        assert!(rejection
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("restricted"));
    }
}
