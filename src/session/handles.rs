//! Opaque, session-owned handles for native resources.
//!
//! # What this replaces
//!
//! Before Phase 2 a client received a device handle as the decimal string of a
//! native pointer and could send any integer back. `DisConnectDev` converted that
//! integer straight to a `DEVHANDLE` and handed it to the vendor library; passing a
//! fabricated value was observed to **kill the service process**. Digest handles
//! were no better: they were keyed by the JSON-RPC id in a process-wide map and
//! were never released if the client simply disconnected.
//!
//! Now a handle is a server-issued string with a type prefix — `dev-1`, `app-1`,
//! `cnt-1`, `hsh-1` — and the table holding it belongs to exactly one session.
//!
//! # Properties this type enforces
//!
//! * **Type safety.** A container id can never be used where a device id is
//!   expected; the prefix is checked on every lookup.
//! * **Session isolation.** The table is a field of `SessionState`, which one
//!   connection reaches through `&mut self`, so another session cannot name these
//!   handles at all.
//! * **Deterministic release.** Dropping the table drops every guard, and guards
//!   release their native handle in `Drop`. That is what makes release on
//!   disconnect — including an abrupt one — guaranteed rather than best-effort.
//! * **No native pointers.** Guards keep their own handles as integers, so nothing
//!   here names `HANDLE`, `DEVHANDLE`, or any other native type.
//!
//! # No lock
//!
//! One connection is one session is exclusive access, so the table needs no
//! interior mutability.

use std::collections::HashMap;

use crate::provider::{ApplicationGuard, ContainerGuard, DeviceGuard, DigestGuard};

/// Why a handle request failed.
///
/// Deliberately carries no value: an error must not echo the offending input or the
/// internal prefix back to the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleError {
    /// The prefix was right but no such handle exists in this session.
    Unknown,
    /// The handle names a different kind of resource.
    WrongKind,
}

/// The kinds of resource a session can own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Device,
    Application,
    Container,
    Digest,
}

impl ResourceKind {
    /// The handle prefix for this kind.
    pub fn prefix(self) -> &'static str {
        match self {
            ResourceKind::Device => "dev",
            ResourceKind::Application => "app",
            ResourceKind::Container => "cnt",
            ResourceKind::Digest => "hsh",
        }
    }

    /// The prefix of `handle`, or `None` when it has no recognisable prefix.
    fn prefix_of(handle: &str) -> Option<&str> {
        handle.split_once('-').map(|(prefix, _)| prefix)
    }

    /// Parse a prefix back into a kind.
    fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "dev" => Some(ResourceKind::Device),
            "app" => Some(ResourceKind::Application),
            "cnt" => Some(ResourceKind::Container),
            "hsh" => Some(ResourceKind::Digest),
            _ => None,
        }
    }
}

/// Every resource table is keyed by the opaque handle string.
type Table<T> = HashMap<String, T>;

/// Handles issued by one session.
///
/// Owned by value inside `SessionState`. Dropping it drops every guard, which
/// releases the underlying native resources.
#[derive(Default)]
pub struct HandleTable {
    next_id: u64,
    devices: Table<Box<dyn DeviceGuard>>,
    applications: Table<Box<dyn ApplicationGuard>>,
    containers: Table<Box<dyn ContainerGuard>>,
    digests: Table<Box<dyn DigestGuard>>,
}

/// Reports counts only.
///
/// Written by hand rather than derived for two reasons: the guard traits are not
/// `Debug`, and a handle string must never reach a log line.
impl std::fmt::Debug for HandleTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandleTable")
            .field("devices", &self.devices.len())
            .field("applications", &self.applications.len())
            .field("containers", &self.containers.len())
            .field("digests", &self.digests.len())
            .finish()
    }
}

impl HandleTable {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            devices: Table::new(),
            applications: Table::new(),
            containers: Table::new(),
            digests: Table::new(),
        }
    }

    /// Mint the next identifier for `kind`.
    ///
    /// The counter is shared across kinds, so no two handles in a session are ever
    /// equal even when their kinds differ.
    fn mint(&mut self, kind: ResourceKind) -> String {
        self.next_id += 1;
        format!("{}-{}", kind.prefix(), self.next_id)
    }

    /// Check a handle against an expected kind.
    ///
    /// Returns the handle unchanged when it is well formed for `expected`. A handle
    /// with no prefix, an unknown prefix, or a different prefix is `WrongKind`; a
    /// well-formed handle of the right kind that this session does not hold is
    /// `Unknown`.
    fn check(&self, handle: &str, expected: ResourceKind) -> Result<(), HandleError> {
        match ResourceKind::prefix_of(handle) {
            Some(prefix) => match ResourceKind::from_prefix(prefix) {
                Some(kind) if kind == expected => Ok(()),
                Some(_) => Err(HandleError::WrongKind),
                None => Err(HandleError::WrongKind),
            },
            // No separator at all: an old client sending the pointer's decimal
            // string, or simply garbage.
            None => Err(HandleError::WrongKind),
        }
    }

    // ---- devices ---------------------------------------------------------

    /// Take ownership of an open device and return its handle.
    pub fn issue_device(&mut self, guard: Box<dyn DeviceGuard>) -> String {
        let handle = self.mint(ResourceKind::Device);
        self.devices.insert(handle.clone(), guard);
        handle
    }

    /// Borrow a device by handle.
    pub fn device(&mut self, handle: &str) -> Result<&mut Box<dyn DeviceGuard>, HandleError> {
        self.check(handle, ResourceKind::Device)?;
        self.devices.get_mut(handle).ok_or(HandleError::Unknown)
    }

    /// Remove a device, returning ownership so the caller can drop it.
    pub fn remove_device(
        &mut self,
        handle: &str,
    ) -> Result<Box<dyn DeviceGuard>, HandleError> {
        self.check(handle, ResourceKind::Device)?;
        self.devices.remove(handle).ok_or(HandleError::Unknown)
    }

    // ---- applications ----------------------------------------------------

    pub fn issue_application(&mut self, guard: Box<dyn ApplicationGuard>) -> String {
        let handle = self.mint(ResourceKind::Application);
        self.applications.insert(handle.clone(), guard);
        handle
    }

    pub fn application(
        &mut self,
        handle: &str,
    ) -> Result<&mut Box<dyn ApplicationGuard>, HandleError> {
        self.check(handle, ResourceKind::Application)?;
        self.applications
            .get_mut(handle)
            .ok_or(HandleError::Unknown)
    }

    pub fn remove_application(
        &mut self,
        handle: &str,
    ) -> Result<Box<dyn ApplicationGuard>, HandleError> {
        self.check(handle, ResourceKind::Application)?;
        self.applications.remove(handle).ok_or(HandleError::Unknown)
    }

    // ---- containers ------------------------------------------------------

    pub fn issue_container(&mut self, guard: Box<dyn ContainerGuard>) -> String {
        let handle = self.mint(ResourceKind::Container);
        self.containers.insert(handle.clone(), guard);
        handle
    }

    pub fn container(
        &mut self,
        handle: &str,
    ) -> Result<&mut Box<dyn ContainerGuard>, HandleError> {
        self.check(handle, ResourceKind::Container)?;
        self.containers.get_mut(handle).ok_or(HandleError::Unknown)
    }

    pub fn remove_container(
        &mut self,
        handle: &str,
    ) -> Result<Box<dyn ContainerGuard>, HandleError> {
        self.check(handle, ResourceKind::Container)?;
        self.containers.remove(handle).ok_or(HandleError::Unknown)
    }

    // ---- digests ---------------------------------------------------------

    pub fn issue_digest(&mut self, guard: Box<dyn DigestGuard>) -> String {
        let handle = self.mint(ResourceKind::Digest);
        self.digests.insert(handle.clone(), guard);
        handle
    }

    pub fn digest(&mut self, handle: &str) -> Result<&mut Box<dyn DigestGuard>, HandleError> {
        self.check(handle, ResourceKind::Digest)?;
        self.digests.get_mut(handle).ok_or(HandleError::Unknown)
    }

    pub fn remove_digest(&mut self, handle: &str) -> Result<Box<dyn DigestGuard>, HandleError> {
        self.check(handle, ResourceKind::Digest)?;
        self.digests.remove(handle).ok_or(HandleError::Unknown)
    }

    /// Total number of resources held.
    pub fn len(&self) -> usize {
        self.devices.len() + self.applications.len() + self.containers.len() + self.digests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::fake::FakeSkfProvider;
    use crate::provider::SkfProvider;

    fn assert_send<T: Send>() {}

    /// Extract a rejection without requiring the `Ok` type to be `Debug`.
    ///
    /// Provider guards deliberately do not implement `Debug` — printing a guard
    /// would risk putting handle material in a log — so `unwrap_err()` cannot be
    /// used on the accessors.
    fn rejection<T>(result: Result<T, HandleError>) -> HandleError {
        match result {
            Ok(_) => panic!("expected a handle rejection, got a live resource"),
            Err(error) => error,
        }
    }

    /// A device guard from the fake provider, so these tests need no hardware.
    fn fake_device() -> (FakeSkfProvider, Box<dyn DeviceGuard>) {
        let provider = FakeSkfProvider::new("FAKE");
        let device = provider.open_device("dev-a").expect("open device");
        (provider, device)
    }

    #[test]
    fn device_handles_have_the_dev_prefix_and_increase() {
        let mut table = HandleTable::new();
        let (_p, first) = fake_device();
        let (_q, second) = fake_device();

        assert_eq!(table.issue_device(first), "dev-1");
        assert_eq!(table.issue_device(second), "dev-2");
    }

    #[test]
    fn an_unknown_but_well_formed_handle_is_unknown() {
        let mut table = HandleTable::new();
        assert_eq!(rejection(table.device("dev-99")), HandleError::Unknown);
    }

    #[test]
    fn using_a_container_handle_as_a_device_is_the_wrong_kind() {
        let mut table = HandleTable::new();
        assert_eq!(rejection(table.device("cnt-1")), HandleError::WrongKind);
        assert_eq!(rejection(table.digest("app-1")), HandleError::WrongKind);
        assert_eq!(
            rejection(table.application("hsh-1")),
            HandleError::WrongKind
        );
    }

    /// The regression that matters most: a legacy client sends back the pointer's
    /// decimal string, and a hostile one sends an integer. Neither may be treated as
    /// a handle.
    #[test]
    fn bare_numbers_and_garbage_are_rejected() {
        let mut table = HandleTable::new();
        for bad in ["1", "0", "140234567890123", "", "no-prefix", "xyz-1", "-", "dev"] {
            assert_eq!(
                rejection(table.device(bad)),
                HandleError::WrongKind,
                "input {:?} must not be accepted as a handle",
                bad
            );
        }
    }

    #[test]
    fn removing_a_device_makes_it_unknown() {
        let mut table = HandleTable::new();
        let (_p, device) = fake_device();
        let handle = table.issue_device(device);
        assert_eq!(table.len(), 1);

        let guard = table.remove_device(&handle).expect("remove");
        drop(guard);

        assert_eq!(table.len(), 0);
        assert_eq!(rejection(table.device(&handle)), HandleError::Unknown);
    }

    #[test]
    fn all_four_kinds_are_counted_and_typed() {
        let mut table = HandleTable::new();
        let (_p, device) = fake_device();
        let device_handle = table.issue_device(device);

        let app = table.device(&device_handle).expect("device").open_application("app").expect("app");
        let app_handle = table.issue_application(app);

        let container = table
            .application(&app_handle)
            .expect("app")
            .open_container("c")
            .expect("container");
        let container_handle = table.issue_container(container);

        let digest = table
            .device(&device_handle)
            .expect("device")
            .begin_digest(0x00000001, b"")
            .expect("digest");
        let digest_handle = table.issue_digest(digest);

        assert_eq!(table.len(), 4);
        assert!(device_handle.starts_with("dev-"));
        assert!(app_handle.starts_with("app-"));
        assert!(container_handle.starts_with("cnt-"));
        assert!(digest_handle.starts_with("hsh-"));

        // Every cross-kind lookup is refused.
        assert_eq!(rejection(table.device(&container_handle)), HandleError::WrongKind);
        assert_eq!(rejection(table.application(&digest_handle)), HandleError::WrongKind);
        assert_eq!(rejection(table.container(&device_handle)), HandleError::WrongKind);
        assert_eq!(rejection(table.digest(&app_handle)), HandleError::WrongKind);
    }

    /// Regression test for research finding V-1.
    ///
    /// Before Phase 2 digest handles lived in a process-global map that was only
    /// cleaned by DigestFinal and CloseHash. A client that called DigestInit and
    /// then disconnected left an open device handle behind for the life of the
    /// process. The handle table makes release a consequence of session teardown,
    /// so this asserts exactly that.
    #[test]
    fn digest_handle_is_released_when_the_session_ends() {
        let provider = FakeSkfProvider::new("FAKE");
        {
            let mut table = HandleTable::new();
            let device = provider.open_device("dev-a").expect("open device");
            let device_handle = table.issue_device(device);
            let digest = table
                .device(&device_handle)
                .expect("device")
                .begin_digest(0x00000001, b"")
                .expect("begin digest");
            table.issue_digest(digest);

            // Abandon it: no DigestFinal, no CloseHash — just end the session.
        }

        assert_eq!(
            provider.call_count(crate::provider::Operation::CloseDigest),
            1,
            "an abandoned digest must still be closed when the session ends"
        );
        assert_eq!(
            provider.call_count(crate::provider::Operation::CloseDevice),
            1,
            "its device must be released too, not left open for the process lifetime"
        );
    }

    /// Dropping the table must release every resource; that is what makes release
    /// on disconnect deterministic.
    #[test]
    fn dropping_the_table_releases_every_resource() {
        let provider = FakeSkfProvider::new("FAKE");
        {
            let mut table = HandleTable::new();
            let device = provider.open_device("dev-a").expect("open");
            let handle = table.issue_device(device);
            let digest = table
                .device(&handle)
                .expect("device")
                .begin_digest(0x00000001, b"")
                .expect("digest");
            table.issue_digest(digest);
            assert_eq!(table.len(), 2);
        }
        assert_eq!(
            provider.call_count(crate::provider::Operation::CloseDevice),
            1,
            "the device must be closed when the table dies"
        );
        assert_eq!(
            provider.call_count(crate::provider::Operation::CloseDigest),
            1,
            "the digest must be closed when the table dies"
        );
    }

    /// Review requirement: a rejection must not carry the offending value. The
    /// error is a fieldless enum, so there is nowhere for the input to hide.
    #[test]
    fn no_rejection_echoes_the_offending_value() {
        assert_eq!(
            std::mem::size_of::<HandleError>(),
            1,
            "HandleError must stay a fieldless enum so it cannot carry the input"
        );
        let mut table = HandleTable::new();
        let rejection = rejection(table.device("dev-SECRET-TOKEN"));
        // Nothing to inspect: the variant is the whole value.
        assert!(matches!(rejection, HandleError::Unknown));
    }

    /// End-to-end shape of the RES-01 fix at the table level: a legacy client sends
    /// the pointer's decimal string, a hostile client sends an integer, and neither
    /// reaches the vendor library.
    #[test]
    fn fabricated_integer_handle_is_rejected() {
        let mut table = HandleTable::new();
        for fabricated in ["1", "140234567890123", "0"] {
            assert_eq!(
                rejection(table.device(fabricated)),
                HandleError::WrongKind,
                "{:?} must not be treated as a handle",
                fabricated
            );
        }
    }

    #[test]
    fn handle_table_is_send() {
        assert_send::<HandleTable>();
    }
}
