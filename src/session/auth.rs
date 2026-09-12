//! Per-session authorization grants.
//!
//! # What this replaces
//!
//! Before Phase 2 the service cached verified PINs in a process-wide map keyed by
//! `provider/device/application`. Any connection that verified a PIN therefore
//! authorized every other connection, and the PIN itself stayed readable in memory
//! for the lifetime of the process.
//!
//! This module stores a **deadline** instead: authorization is a property of one
//! session, and the credential that produced it is not retained at all.
//!
//! # Why the PIN is not kept
//!
//! SESS-05 requires that no recoverable copy of the PIN survives verification.
//! Storing it — even with a "wipe on drop" helper — cannot be made reliable in
//! Rust: a `String` can be copied by the allocator, and a reallocation leaves the
//! old bytes behind. The only dependable answer is not to keep it, so [`Grant`]
//! holds nothing but an `Instant`.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Default authorization lifetime when `SKF_AUTH_TTL_SECONDS` is unset.
pub const DEFAULT_TTL_SECONDS: u64 = 600;

/// Environment variable that overrides [`DEFAULT_TTL_SECONDS`].
pub const TTL_ENV_VAR: &str = "SKF_AUTH_TTL_SECONDS";

/// Why an authorization check failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthRejection {
    /// The session never authorized this application, or it was cleared.
    NotAuthorized,
    /// The grant existed but its deadline has passed.
    Expired,
    /// An operation found the device missing, so its grants were dropped.
    DeviceUnavailable,
}

/// Identifies what was authorized.
///
/// Deliberately contains no credential: it is a description of the target, not of
/// the proof.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthKey {
    pub provider: String,
    pub device: String,
    pub application: String,
}

impl AuthKey {
    pub fn new(
        provider: impl Into<String>,
        device: impl Into<String>,
        application: impl Into<String>,
    ) -> Self {
        Self {
            provider: provider.into(),
            device: device.into(),
            application: application.into(),
        }
    }
}

/// One authorization grant.
///
/// Holds **only** a deadline. See the module docs for why the PIN is not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grant {
    pub expires_at: Instant,
}

/// Grants held by one session.
#[derive(Debug)]
pub struct AuthTable {
    grants: HashMap<AuthKey, Grant>,
    ttl: Duration,
}

impl AuthTable {
    pub fn new(ttl: Duration) -> Self {
        Self {
            grants: HashMap::new(),
            ttl,
        }
    }

    /// The configured lifetime, for diagnostics and tests.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Record a successful verification.
    pub fn grant(&mut self, key: AuthKey) {
        self.grants.insert(
            key,
            Grant {
                expires_at: Instant::now() + self.ttl,
            },
        );
    }

    /// Check an authorization, removing it if it has expired.
    ///
    /// Expiry *removes* the entry rather than merely reporting it, so a stale grant
    /// cannot be observed later by anything that inspects the table.
    pub fn check(&mut self, key: &AuthKey) -> Result<(), AuthRejection> {
        match self.grants.get(key).copied() {
            None => Err(AuthRejection::NotAuthorized),
            Some(grant) if grant.expires_at <= Instant::now() => {
                self.grants.remove(key);
                Err(AuthRejection::Expired)
            }
            Some(_) => Ok(()),
        }
    }

    /// Drop every grant for one device, returning how many were removed.
    pub fn invalidate_device(&mut self, provider: &str, device: &str) -> usize {
        let before = self.grants.len();
        self.grants
            .retain(|key, _| !(key.provider == provider && key.device == device));
        before - self.grants.len()
    }

    pub fn len(&self) -> usize {
        self.grants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }
}

/// Read the authorization lifetime from the environment.
///
/// Falls back to [`DEFAULT_TTL_SECONDS`] and warns when the value is missing,
/// unparseable, or zero. `0` is not "never expires": SESS-03 requires expiry, so an
/// unbounded grant is not representable and a zero is treated as a mistake.
pub fn ttl_from_env() -> Duration {
    match std::env::var(TTL_ENV_VAR) {
        Err(_) => Duration::from_secs(DEFAULT_TTL_SECONDS),
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(0) => {
                log::warn!(
                    "{}={} is not a valid lifetime (0 would mean never expire); using {}s",
                    TTL_ENV_VAR,
                    raw.trim(),
                    DEFAULT_TTL_SECONDS
                );
                Duration::from_secs(DEFAULT_TTL_SECONDS)
            }
            Ok(seconds) => Duration::from_secs(seconds),
            Err(_) => {
                log::warn!(
                    "{}='{}' is not a number; using {}s",
                    TTL_ENV_VAR,
                    raw,
                    DEFAULT_TTL_SECONDS
                );
                Duration::from_secs(DEFAULT_TTL_SECONDS)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> AuthKey {
        AuthKey::new("GM3000", "dev-a", "app")
    }

    #[test]
    fn an_authorized_application_is_accepted() {
        let mut table = AuthTable::new(Duration::from_secs(600));
        table.grant(key());
        assert_eq!(table.check(&key()), Ok(()));
    }

    #[test]
    fn an_unauthorized_application_is_rejected() {
        let mut table = AuthTable::new(Duration::from_secs(600));
        assert_eq!(table.check(&key()), Err(AuthRejection::NotAuthorized));
    }

    #[test]
    fn expiry_removes_the_grant() {
        let mut table = AuthTable::new(Duration::from_millis(50));
        table.grant(key());
        assert_eq!(table.len(), 1);

        std::thread::sleep(Duration::from_millis(80));

        assert_eq!(table.check(&key()), Err(AuthRejection::Expired));
        assert_eq!(
            table.len(),
            0,
            "an expired grant must be removed, not merely reported"
        );
    }

    #[test]
    fn invalidate_device_clears_only_that_device() {
        let mut table = AuthTable::new(Duration::from_secs(600));
        table.grant(AuthKey::new("GM3000", "dev-a", "app"));
        table.grant(AuthKey::new("GM3000", "dev-b", "app"));
        table.grant(AuthKey::new("Other", "dev-a", "app"));

        let removed = table.invalidate_device("GM3000", "dev-a");

        assert_eq!(removed, 1);
        assert_eq!(
            table.check(&AuthKey::new("GM3000", "dev-a", "app")),
            Err(AuthRejection::NotAuthorized)
        );
        assert_eq!(table.check(&AuthKey::new("GM3000", "dev-b", "app")), Ok(()));
        assert_eq!(
            table.check(&AuthKey::new("Other", "dev-a", "app")),
            Ok(()),
            "a different provider must not be affected"
        );
    }

    /// A grant must be a plain deadline with no heap-allocated credential in it.
    #[test]
    fn a_grant_holds_only_a_deadline() {
        assert!(
            std::mem::size_of::<Grant>() <= 32,
            "Grant must stay a small Copy value holding only an Instant, got {} bytes",
            std::mem::size_of::<Grant>()
        );
        fn assert_copy<T: Copy>() {}
        assert_copy::<Grant>();
    }

    #[test]
    fn ttl_defaults_when_the_variable_is_absent() {
        // The variable is not set in the test environment.
        std::env::remove_var(TTL_ENV_VAR);
        assert_eq!(
            ttl_from_env(),
            Duration::from_secs(DEFAULT_TTL_SECONDS),
            "an unset variable must fall back to the documented default"
        );
    }

    #[test]
    fn ttl_rejects_zero_and_garbage() {
        std::env::set_var(TTL_ENV_VAR, "0");
        assert_eq!(ttl_from_env(), Duration::from_secs(DEFAULT_TTL_SECONDS));

        std::env::set_var(TTL_ENV_VAR, "not-a-number");
        assert_eq!(ttl_from_env(), Duration::from_secs(DEFAULT_TTL_SECONDS));

        std::env::set_var(TTL_ENV_VAR, "30");
        assert_eq!(ttl_from_env(), Duration::from_secs(30));

        std::env::remove_var(TTL_ENV_VAR);
    }
}
