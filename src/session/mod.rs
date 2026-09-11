//! Session identity and lifecycle.
//!
//! # Why a session exists at all
//!
//! Before Phase 2 the service kept verified PINs in a process-wide map keyed by
//! `provider/device/application`. Any connection that verified a PIN therefore
//! authorized every other connection, and native handles were handed to clients as
//! raw pointer values that a client could forge. This module becomes the owner of
//! the state that fixes both: identity, authorization (plan 02-02), and the
//! resources a connection actually holds (plan 02-03).
//!
//! # Ownership model
//!
//! ```text
//! connection ──owns──► SessionGuard ──owns──► SessionState
//!                            └──holds──► Arc<SessionMarker>  ◄──Weak── SessionRegistry
//!```
//!
//! Two properties of this shape are deliberate and load-bearing:
//!
//! 1. **The registry never owns session state.** It stores `Weak<SessionMarker>`,
//!    a marker holding only the id. If it stored `Arc<SessionState>` it would keep
//!    sessions — and every native handle they own — alive past disconnect, which is
//!    exactly the leak class this phase removes.
//! 2. **`SessionState` is held by value, not behind an `Arc`.** It will contain
//!    `Box<dyn DeviceGuard>` values (plan 02-03), which are `Send` but not `Sync`,
//!    so an `Arc<SessionState>` would not be `Send` and the session could not be
//!    moved into a Tokio task. Making it `Send` again would need an
//!    `unsafe impl Sync`, which the crate forbids.
//!
//! # No interior mutability
//!
//! `Session::handle` receives `&mut self`, and one connection is one session, so
//! session state needs no `Mutex` or `RwLock`: exclusive access is already
//! guaranteed. Only the registry, which is shared across connections, takes a lock.

pub mod registry;

pub use registry::{SessionGuard, SessionRegistry};

/// A 128-bit session identifier, rendered as 32 lowercase hex characters.
///
/// Never sent to clients: a distributed identifier becomes a credential, and a
/// credential that survives reconnection would contradict the rule that
/// authorization is cleared when the connection closes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

impl SessionId {
    /// The identifier as a lowercase hex string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Raw entropy width, in bytes.
    pub fn len_bytes(&self) -> usize {
        16
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Generate a fresh session identifier from the OS CSPRNG.
///
/// Uses `OsRng` rather than `thread_rng`: a session id must be unguessable, since
/// anyone who can predict it can address another session's state.
pub fn generate_session_id() -> SessionId {
    use rand::RngCore;

    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);

    let mut hex = String::with_capacity(32);
    for byte in bytes {
        use std::fmt::Write as _;
        // Writing into a String cannot fail.
        let _ = write!(hex, "{:02x}", byte);
    }
    SessionId(hex)
}

/// Lightweight, `Send + Sync` identity handle for the registry.
///
/// Holds only the id, so the registry's `Weak` stays cheap and the registry never
/// reaches into session state.
#[derive(Debug)]
pub struct SessionMarker {
    id: SessionId,
}

impl SessionMarker {
    pub fn new(id: SessionId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &SessionId {
        &self.id
    }
}

/// State owned exclusively by one connection, held **by value**.
///
/// Nothing here needs interior mutability: `Session::handle` gives `&mut self`.
/// Authorization and the handle table are added by plans 02-02 and 02-03.
#[derive(Debug)]
pub struct SessionState {
    id: SessionId,
}

impl SessionState {
    /// Create state for a fresh session.
    pub fn new() -> Self {
        Self {
            id: generate_session_id(),
        }
    }

    pub fn id(&self) -> &SessionId {
        &self.id
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn session_ids_are_32_lowercase_hex_characters() {
        let id = generate_session_id();
        assert_eq!(id.as_str().len(), 32);
        assert_eq!(id.len_bytes(), 16);
        assert!(
            id.as_str()
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "id must be lowercase hex, got {}",
            id.as_str()
        );
    }

    #[test]
    fn session_ids_do_not_repeat() {
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(
                seen.insert(generate_session_id()),
                "OsRng produced a duplicate session id"
            );
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn session_state_exposes_its_id() {
        let state = SessionState::new();
        assert_eq!(state.id().as_str().len(), 32);
    }

    /// The session must be movable into a Tokio task. If session state is ever
    /// changed to sit behind an `Arc`, this stops compiling — which is the point:
    /// that change would require a new `unsafe impl Sync` to work.
    #[test]
    fn session_guard_is_send() {
        assert_send::<SessionGuard>();
    }
}
