//! Liveness tracking for sessions, without ownership.
//!
//! The registry answers one question — "which sessions are alive right now?" — and
//! deliberately does nothing else. In particular it cannot read or modify session
//! state, because that would require it to hold something stronger than a `Weak`
//! and would re-introduce the shared-state defect this phase removes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use super::{SessionId, SessionMarker, SessionState};

/// Tracks live sessions **without owning them**.
///
/// Entries are `Weak`, so a session's lifetime is controlled by the connection
/// that created it. A registry holding `Arc` would keep session state — and every
/// native handle it owns — alive after disconnect, which is precisely the leak
/// this phase exists to remove.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    sessions: Mutex<HashMap<SessionId, Weak<SessionMarker>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Register a session and return its id.
    ///
    /// The caller keeps the `Arc<SessionMarker>`; the registry holds only a `Weak`.
    pub fn register(self: &Arc<Self>, marker: Arc<SessionMarker>) -> SessionId {
        let id = marker.id().clone();
        self.sessions
            .lock()
            .expect("session registry mutex")
            .insert(id.clone(), Arc::downgrade(&marker));
        id
    }

    /// Remove a session by id. Called from [`SessionGuard`]'s `Drop`.
    fn deregister(&self, id: &SessionId) {
        self.sessions
            .lock()
            .expect("session registry mutex")
            .remove(id);
    }

    /// Number of live sessions, pruning entries whose session has gone away.
    ///
    /// This is the only read the registry offers. There is deliberately no lookup
    /// by id: the whole point of the session model is that a request can only reach
    /// its own state.
    pub fn session_count(&self) -> usize {
        let mut sessions = self.sessions.lock().expect("session registry mutex");
        sessions.retain(|_, weak| weak.strong_count() > 0);
        sessions.len()
    }
}

/// Owns one session for as long as the connection lives.
///
/// Dropping the guard deregisters from the registry and releases every native
/// handle the session owns (once the handle table lands in plan 02-03), which is
/// what makes release-on-disconnect deterministic.
///
/// `state` is held **by value**: session state contains `Send`-but-`!Sync` provider
/// guards, so an `Arc` around it would make the whole guard non-`Send` and the
/// session could not be moved into a Tokio task.
#[derive(Debug)]
pub struct SessionGuard {
    registry: Arc<SessionRegistry>,
    marker: Arc<SessionMarker>,
    state: SessionState,
}

impl SessionGuard {
    /// Register a fresh session and take ownership of its state.
    pub fn create(registry: &Arc<SessionRegistry>, state: SessionState) -> Self {
        let marker = Arc::new(SessionMarker::new(state.id().clone()));
        let _ = registry.register(Arc::clone(&marker));
        Self {
            registry: Arc::clone(registry),
            marker,
            state,
        }
    }

    pub fn id(&self) -> &SessionId {
        self.marker.id()
    }

    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut SessionState {
        &mut self.state
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        self.registry.deregister(self.marker.id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    fn new_registry() -> Arc<SessionRegistry> {
        Arc::new(SessionRegistry::new())
    }

    #[test]
    fn registering_a_session_makes_it_counted() {
        let registry = new_registry();
        let _guard = SessionGuard::create(&registry, SessionState::new());
        assert_eq!(registry.session_count(), 1);
    }

    /// Regression test for the leak class this phase removes: if the registry held
    /// a strong reference, the session would outlive its connection.
    #[test]
    fn dropping_the_guard_deregisters_the_session() {
        let registry = new_registry();
        {
            let _guard = SessionGuard::create(&registry, SessionState::new());
            assert_eq!(registry.session_count(), 1);
        }
        assert_eq!(
            registry.session_count(),
            0,
            "a dropped connection must not leave a session registered"
        );
    }

    #[test]
    fn registry_does_not_keep_sessions_alive() {
        let registry = new_registry();
        let marker = Arc::new(SessionMarker::new(super::super::generate_session_id()));
        registry.register(Arc::clone(&marker));

        assert_eq!(registry.session_count(), 1);
        assert_eq!(
            Arc::strong_count(&marker),
            1,
            "the registry must hold only a Weak, not a second strong reference"
        );

        drop(marker);
        assert_eq!(
            registry.session_count(),
            0,
            "the session disappears as soon as its owner lets go"
        );
    }

    #[test]
    fn several_sessions_are_counted_independently() {
        let registry = new_registry();
        let first = SessionGuard::create(&registry, SessionState::new());
        let second = SessionGuard::create(&registry, SessionState::new());
        let third = SessionGuard::create(&registry, SessionState::new());
        assert_eq!(registry.session_count(), 3);

        drop(second);
        assert_eq!(registry.session_count(), 2);
        drop(first);
        drop(third);
        assert_eq!(registry.session_count(), 0);
    }

    #[test]
    fn guard_exposes_its_id_and_state() {
        let registry = new_registry();
        let guard = SessionGuard::create(&registry, SessionState::new());
        assert_eq!(guard.id().as_str().len(), 32);
        assert_eq!(guard.id(), guard.state().id());
    }

    /// The guard must be movable into a Tokio task; see the note on `SessionGuard`.
    #[test]
    fn session_guard_is_send() {
        assert_send::<SessionGuard>();
    }
}
