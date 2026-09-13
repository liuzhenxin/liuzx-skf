//! Structured, non-sensitive authorization audit events (AUD-01, AUD-02).
//!
//! Every authorization decision is recorded here as one machine-parseable JSON
//! line. The events are emitted from the single decision point — the
//! [`crate::session::SessionState`] authorization methods — so no call site can
//! forget to audit.
//!
//! # What is never recorded
//!
//! The event variants can only carry provider, device, and application names, a
//! rejection reason, and a count. There is no variant that accepts a PIN, key
//! material, a decrypted payload, or a bearer token, so AUD-02 holds by
//! construction; `logging::sanitize` strips control characters so a
//! name cannot forge a second log line.

use serde_json::json;

use crate::logging::sanitize;
use crate::session::auth::AuthRejection;

/// Why an authorization was denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditReason {
    /// No grant exists for the target.
    NotAuthorized,
    /// The grant existed but its deadline passed.
    Expired,
    /// The device was found missing and its grants were dropped.
    DeviceUnavailable,
}

impl AuditReason {
    /// The stable, non-sensitive wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAuthorized => "not_authorized",
            Self::Expired => "expired",
            Self::DeviceUnavailable => "device_unavailable",
        }
    }
}

/// One authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    /// A successful PIN verification created a grant.
    AuthorizationGranted {
        provider: String,
        device: String,
        application: String,
    },
    /// An operation was refused because the session held no grant.
    AuthorizationDenied {
        provider: String,
        device: String,
        application: String,
        reason: AuditReason,
    },
    /// An operation was refused because the grant had expired.
    AuthorizationExpired {
        provider: String,
        device: String,
        application: String,
    },
    /// An operation found the device missing and cleared its grants.
    DeviceUnavailable {
        provider: String,
        device: String,
        cleared: usize,
    },
}

impl AuditEvent {
    /// Build a `authorization.granted` event.
    pub fn granted(provider: &str, device: &str, application: &str) -> Self {
        Self::AuthorizationGranted {
            provider: provider.to_string(),
            device: device.to_string(),
            application: application.to_string(),
        }
    }

    /// Build an `authorization.denied` event.
    pub fn denied(provider: &str, device: &str, application: &str, reason: AuditReason) -> Self {
        Self::AuthorizationDenied {
            provider: provider.to_string(),
            device: device.to_string(),
            application: application.to_string(),
            reason,
        }
    }

    /// Build an `authorization.expired` event.
    pub fn expired(provider: &str, device: &str, application: &str) -> Self {
        Self::AuthorizationExpired {
            provider: provider.to_string(),
            device: device.to_string(),
            application: application.to_string(),
        }
    }

    /// Build a `device.unavailable` event.
    pub fn device_unavailable(provider: &str, device: &str, cleared: usize) -> Self {
        Self::DeviceUnavailable {
            provider: provider.to_string(),
            device: device.to_string(),
            cleared,
        }
    }

    /// The stable event name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::AuthorizationGranted { .. } => "authorization.granted",
            Self::AuthorizationDenied { .. } => "authorization.denied",
            Self::AuthorizationExpired { .. } => "authorization.expired",
            Self::DeviceUnavailable { .. } => "device.unavailable",
        }
    }

    /// Render one JSON object. All string fields are sanitized.
    pub fn to_json(&self) -> String {
        let value = match self {
            Self::AuthorizationGranted {
                provider,
                device,
                application,
            } => json!({
                "audit": true,
                "event": self.name(),
                "provider": sanitize(provider),
                "device": sanitize(device),
                "application": sanitize(application),
            }),
            Self::AuthorizationDenied {
                provider,
                device,
                application,
                reason,
            } => json!({
                "audit": true,
                "event": self.name(),
                "provider": sanitize(provider),
                "device": sanitize(device),
                "application": sanitize(application),
                "reason": reason.as_str(),
            }),
            Self::AuthorizationExpired {
                provider,
                device,
                application,
            } => json!({
                "audit": true,
                "event": self.name(),
                "provider": sanitize(provider),
                "device": sanitize(device),
                "application": sanitize(application),
            }),
            Self::DeviceUnavailable {
                provider,
                device,
                cleared,
            } => json!({
                "audit": true,
                "event": self.name(),
                "provider": sanitize(provider),
                "device": sanitize(device),
                "cleared": cleared,
            }),
        };
        value.to_string()
    }
}

/// Emit one audit record through the process logger.
///
/// Deliberately the only place that touches `log`: the message is the JSON object,
/// so both the console logger and the rotating file logger capture it unchanged.
pub fn emit(event: &AuditEvent) {
    log::info!("{}", event.to_json());
}

/// Map an [`AuthRejection`] to an audit reason.
pub fn reason_for(rejection: AuthRejection) -> AuditReason {
    match rejection {
        AuthRejection::NotAuthorized => AuditReason::NotAuthorized,
        AuthRejection::Expired => AuditReason::Expired,
        AuthRejection::DeviceUnavailable => AuditReason::DeviceUnavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granted_event_has_the_expected_json() {
        let event = AuditEvent::granted("GM3000", "dev-a", "app");
        let value: serde_json::Value = serde_json::from_str(&event.to_json()).expect("valid json");
        assert_eq!(value["audit"], json!(true));
        assert_eq!(value["event"], json!("authorization.granted"));
        assert_eq!(value["provider"], json!("GM3000"));
        assert_eq!(value["device"], json!("dev-a"));
        assert_eq!(value["application"], json!("app"));
    }

    #[test]
    fn denied_event_carries_the_reason() {
        let event = AuditEvent::denied("GM3000", "dev-a", "app", AuditReason::NotAuthorized);
        let value: serde_json::Value = serde_json::from_str(&event.to_json()).expect("valid json");
        assert_eq!(value["event"], json!("authorization.denied"));
        assert_eq!(value["reason"], json!("not_authorized"));
    }

    #[test]
    fn expired_and_device_unavailable_names() {
        assert_eq!(
            AuditEvent::expired("p", "d", "a").name(),
            "authorization.expired"
        );
        let event = AuditEvent::device_unavailable("GM3000", "dev-a", 3);
        assert_eq!(event.name(), "device.unavailable");
        let value: serde_json::Value = serde_json::from_str(&event.to_json()).expect("json");
        assert_eq!(value["cleared"], json!(3));
    }

    #[test]
    fn audit_json_has_no_secret_fields() {
        let events = [
            AuditEvent::granted("GM3000", "dev-a", "app"),
            AuditEvent::denied("GM3000", "dev-a", "app", AuditReason::Expired),
            AuditEvent::expired("GM3000", "dev-a", "app"),
            AuditEvent::device_unavailable("GM3000", "dev-a", 1),
        ];
        for event in events {
            let json = event.to_json();
            // The marker itself and the structural keys are the only vocabulary.
            for forbidden in ["pin", "private_key", "payload", "token", "secret"] {
                assert!(
                    !json.to_ascii_lowercase().contains(forbidden),
                    "audit event leaked '{forbidden}': {json}"
                );
            }
        }
    }

    #[test]
    fn reason_for_maps_each_rejection() {
        assert_eq!(
            reason_for(AuthRejection::NotAuthorized),
            AuditReason::NotAuthorized
        );
        assert_eq!(reason_for(AuthRejection::Expired), AuditReason::Expired);
        assert_eq!(
            reason_for(AuthRejection::DeviceUnavailable),
            AuditReason::DeviceUnavailable
        );
    }

    #[test]
    fn control_characters_cannot_forge_a_second_record() {
        let event = AuditEvent::granted("GM3000", "dev\n{\"audit\":true}", "app");
        let json = event.to_json();
        assert!(!json.contains('\n'), "sanitize must strip newlines: {json}");
    }
}
