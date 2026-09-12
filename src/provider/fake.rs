//! Deterministic in-memory [`SkfProvider`] for tests.
//!
//! Two capabilities make this useful rather than decorative:
//!
//! * **Composable failure injection.** [`FakeSkfProvider::fail_next`] queues a
//!   failure for one operation. A fake that can only succeed leaves the failure
//!   paths — which are where cleanup bugs live — untested.
//! * **Call recording.** Every operation and every guard drop is appended to a
//!   call log, so tests can assert that resources were actually released. Phase 2
//!   relies on this to prove deterministic release.
//!
//! The call log records only [`Operation`] values, never parameters, so no PIN or
//! key material can leak into a test artefact.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use super::{
    AppHandle, ApplicationGuard, ContainerGuard, ContainerHandle, DeviceGuard, DeviceHandle,
    DeviceInfo, DigestGuard, DigestHandle, EccPublicKey, Operation, PinOutcome, ProviderError,
    ProviderResult, RsaPublicKey, SkfError, SkfProvider,
};

/// First synthetic handle value. Chosen well above any plausible device index so
/// a synthetic handle can never be mistaken for a real one in a log.
const FIRST_HANDLE: u64 = 1000;

/// Records a device event for [`SkfProvider::wait_for_event`].
#[derive(Debug, Clone)]
pub struct FakeEvent {
    pub device_name: String,
    pub event_code: u32,
}

/// Shared, interior-mutable state behind every fake guard.
struct FakeInner {
    alias: String,
    failures: Mutex<HashMap<Operation, VecDeque<SkfError>>>,
    calls: Mutex<Vec<Operation>>,
    next_handle: AtomicU64,
    devices: Mutex<Vec<String>>,
    /// Device state code reported by [`SkfProvider::device_state`].
    device_state: Mutex<u32>,
    /// Container names reported by the application guard.
    containers: Mutex<Vec<String>>,
    /// Digest output produced by [`DigestGuard::finalize`].
    digest_output: Mutex<Vec<u8>>,
    /// Event returned by [`SkfProvider::wait_for_event`], when one is armed.
    pending_event: Mutex<Option<FakeEvent>>,
}

impl FakeInner {
    fn record(&self, op: Operation) {
        self.calls.lock().expect("call log").push(op);
    }

    /// Record the operation, then consume any injected failure for it.
    fn begin(&self, op: Operation) -> Result<(), ProviderError> {
        self.record(op);
        let injected = {
            let mut failures = self.failures.lock().expect("failure queue");
            failures.get_mut(&op).and_then(VecDeque::pop_front)
        };
        match injected {
            None => Ok(()),
            Some(SkfError::Blocking(delay)) => {
                // Deliberately succeed after the delay: the point of this failure
                // kind is to model a slow-but-working token for Phase 3.
                std::thread::sleep(delay);
                Ok(())
            }
            Some(err) => Err(ProviderError::Injected(err)),
        }
    }

    fn next_handle(&self) -> u64 {
        self.next_handle.fetch_add(1, Ordering::Relaxed)
    }
}

/// A fake provider.
pub struct FakeSkfProvider {
    inner: std::sync::Arc<FakeInner>,
}

impl FakeSkfProvider {
    /// A provider with no devices and defaults for every other field.
    pub fn new(alias: &str) -> Self {
        Self::with_devices(alias, Vec::new())
    }

    /// A provider that reports `devices` from [`SkfProvider::enum_devices`].
    pub fn with_devices(alias: &str, devices: Vec<String>) -> Self {
        Self {
            inner: std::sync::Arc::new(FakeInner {
                alias: alias.to_string(),
                failures: Mutex::new(HashMap::new()),
                calls: Mutex::new(Vec::new()),
                next_handle: AtomicU64::new(FIRST_HANDLE),
                devices: Mutex::new(devices),
                device_state: Mutex::new(0),
                containers: Mutex::new(Vec::new()),
                digest_output: Mutex::new(vec![0xAB; 32]),
                pending_event: Mutex::new(None),
            }),
        }
    }

    /// Queue a failure to be returned by the next call to `op`.
    ///
    /// Failures are consumed in FIFO order, so a test can express "the second
    /// PIN verification fails".
    pub fn fail_next(&self, op: Operation, err: SkfError) {
        self.inner
            .failures
            .lock()
            .expect("failure queue")
            .entry(op)
            .or_default()
            .push_back(err);
    }

    /// The recorded call sequence, in order.
    pub fn calls(&self) -> Vec<Operation> {
        self.inner.calls.lock().expect("call log").clone()
    }

    /// How many times `op` was called.
    pub fn call_count(&self, op: Operation) -> usize {
        self.calls().into_iter().filter(|c| *c == op).count()
    }

    /// Drop all queued failures.
    pub fn clear_failures(&self) {
        self.inner.failures.lock().expect("failure queue").clear();
    }

    /// Set the state code returned by [`SkfProvider::device_state`].
    pub fn set_device_state(&self, state: u32) {
        *self.inner.device_state.lock().expect("device state") = state;
    }

    /// Set the container names reported by the application guard.
    pub fn set_containers(&self, containers: Vec<String>) {
        *self.inner.containers.lock().expect("containers") = containers;
    }

    /// Set the bytes returned by [`DigestGuard::finalize`].
    pub fn set_digest_output(&self, output: Vec<u8>) {
        *self.inner.digest_output.lock().expect("digest output") = output;
    }

    /// Arm a device event for the next [`SkfProvider::wait_for_event`].
    pub fn arm_event(&self, device_name: &str, event_code: u32) {
        *self.inner.pending_event.lock().expect("pending event") = Some(FakeEvent {
            device_name: device_name.to_string(),
            event_code,
        });
    }
}

impl SkfProvider for FakeSkfProvider {
    fn alias(&self) -> &str {
        &self.inner.alias
    }

    fn enum_devices(&self, _present_only: bool) -> ProviderResult<Vec<String>> {
        self.inner.begin(Operation::EnumDevices)?;
        Ok(self.inner.devices.lock().expect("devices").clone())
    }

    fn device_state(&self, _name: &str) -> ProviderResult<u32> {
        self.inner.begin(Operation::DeviceState)?;
        Ok(*self.inner.device_state.lock().expect("device state"))
    }

    fn wait_for_event(&self, _buf_len: usize) -> ProviderResult<(String, u32)> {
        self.inner.begin(Operation::WaitForEvent)?;
        let event = self
            .inner
            .pending_event
            .lock()
            .expect("pending event")
            .take();
        match event {
            Some(e) => Ok((e.device_name, e.event_code)),
            // Mirrors the vendor behaviour with no device attached: a non-zero code
            // rather than a hang.
            None => Err(ProviderError::Native {
                code: 0x0A00_0022,
                context: "WaitForDevEvent",
            }),
        }
    }

    fn cancel_wait_for_event(&self) -> ProviderResult<()> {
        self.inner.begin(Operation::CancelWaitForEvent)
    }

    fn open_device(&self, name: &str) -> ProviderResult<Box<dyn DeviceGuard>> {
        self.inner.begin(Operation::OpenDevice)?;
        Ok(Box::new(FakeDevice {
            inner: std::sync::Arc::clone(&self.inner),
            name: name.to_string(),
            exposed: DeviceHandle(self.inner.next_handle()),
        }))
    }
}

/// Fake device guard. Dropping it records [`Operation::CloseDevice`].
pub struct FakeDevice {
    inner: std::sync::Arc<FakeInner>,
    name: String,
    exposed: DeviceHandle,
}

impl Drop for FakeDevice {
    fn drop(&mut self) {
        self.inner.record(Operation::CloseDevice);
    }
}

impl DeviceGuard for FakeDevice {
    fn handle(&self) -> DeviceHandle {
        self.exposed
    }

    fn state(&self) -> ProviderResult<u32> {
        self.inner.begin(Operation::DeviceState)?;
        Ok(*self.inner.device_state.lock().expect("device state"))
    }

    fn lock(&self, _timeout: Duration) -> ProviderResult<()> {
        self.inner.begin(Operation::LockDevice)
    }

    fn unlock(&self) -> ProviderResult<()> {
        self.inner.begin(Operation::UnlockDevice)
    }

    fn transmit(&self, command: &[u8]) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::Transmit)?;
        // Echo the command so tests can assert the round trip without a token.
        Ok(command.to_vec())
    }

    fn random(&self, len: usize) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::Random)?;
        Ok(vec![0x5A; len])
    }

    fn info(&self) -> ProviderResult<DeviceInfo> {
        self.inner.begin(Operation::DeviceInfo)?;
        Ok(DeviceInfo {
            manufacturer: "Fake Manufacturer".into(),
            issuer: "Fake Issuer".into(),
            label: self.name.clone(),
            serial_number: "FAKE0001".into(),
            total_space: 65536,
            free_space: 32768,
            max_ecc_buffer_size: 256,
            max_buffer_size: 4096,
        })
    }

    fn set_label(&self, _label: &str) -> ProviderResult<()> {
        self.inner.begin(Operation::SetLabel)
    }

    fn begin_digest(&self, _alg_id: u32, _id: &[u8]) -> ProviderResult<Box<dyn DigestGuard>> {
        self.inner.begin(Operation::DigestBegin)?;
        Ok(Box::new(FakeDigest {
            inner: std::sync::Arc::clone(&self.inner),
            exposed: DigestHandle(self.inner.next_handle()),
        }))
    }

    fn begin_digest_with_key(
        &self,
        alg_id: u32,
        id: &[u8],
        _public_key: &EccPublicKey,
    ) -> ProviderResult<Box<dyn DigestGuard>> {
        // The key only changes the SM2 `Z` mixing, which the fake does not model;
        // the call is still recorded so a release test can observe it.
        self.begin_digest(alg_id, id)
    }

    fn open_application(&self, name: &str) -> ProviderResult<Box<dyn ApplicationGuard>> {
        self.inner.begin(Operation::OpenApplication)?;
        let _ = name;
        Ok(Box::new(FakeApplication {
            inner: std::sync::Arc::clone(&self.inner),
            exposed: AppHandle(self.inner.next_handle()),
        }))
    }
}

/// Fake application guard. Dropping it records [`Operation::CloseApplication`].
pub struct FakeApplication {
    inner: std::sync::Arc<FakeInner>,
    exposed: AppHandle,
}

impl Drop for FakeApplication {
    fn drop(&mut self) {
        self.inner.record(Operation::CloseApplication);
    }
}

impl ApplicationGuard for FakeApplication {
    fn handle(&self) -> AppHandle {
        self.exposed
    }

    fn enum_containers(&self) -> ProviderResult<Vec<String>> {
        self.inner.begin(Operation::EnumContainers)?;
        Ok(self.inner.containers.lock().expect("containers").clone())
    }

    fn verify_pin(&self, _pin: &str) -> ProviderResult<PinOutcome> {
        // A wrong PIN is a business result, never an Err.
        match self.inner.begin(Operation::VerifyPin) {
            Ok(()) => Ok(PinOutcome {
                success: true,
                retry_count: 3,
                code: 0,
            }),
            Err(ProviderError::Injected(SkfError::PinIncorrect)) => Ok(PinOutcome {
                success: false,
                retry_count: 2,
                code: 0x0A00_002F,
            }),
            Err(ProviderError::Injected(SkfError::PinLocked)) => Ok(PinOutcome {
                success: false,
                retry_count: 0,
                code: 0x0A00_0030,
            }),
            Err(other) => Err(other),
        }
    }

    fn open_container(&self, _name: &str) -> ProviderResult<Box<dyn ContainerGuard>> {
        self.inner.begin(Operation::OpenContainer)?;
        Ok(Box::new(FakeContainer {
            inner: std::sync::Arc::clone(&self.inner),
            exposed: ContainerHandle(self.inner.next_handle()),
        }))
    }

    fn delete_container(&self, _name: &str) -> ProviderResult<()> {
        self.inner.begin(Operation::DeleteContainer)
    }

    fn create_container(&self, _name: &str) -> ProviderResult<Box<dyn ContainerGuard>> {
        self.inner.begin(Operation::CreateContainer)?;
        Ok(Box::new(FakeContainer {
            inner: std::sync::Arc::clone(&self.inner),
            exposed: ContainerHandle(self.inner.next_handle()),
        }))
    }
}

/// Fake container guard. Dropping it records [`Operation::CloseContainer`].
pub struct FakeContainer {
    inner: std::sync::Arc<FakeInner>,
    exposed: ContainerHandle,
}

impl Drop for FakeContainer {
    fn drop(&mut self) {
        self.inner.record(Operation::CloseContainer);
    }
}

impl ContainerGuard for FakeContainer {
    fn handle(&self) -> ContainerHandle {
        self.exposed
    }

    fn container_type(&self) -> ProviderResult<u32> {
        self.inner.begin(Operation::ContainerType)?;
        Ok(1)
    }

    fn export_certificate(&self, _sign_flag: bool) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::ExportCertificate)?;
        Ok(vec![0x30, 0x00])
    }

    fn import_certificate(&self, _sign_flag: bool, _cert: &[u8]) -> ProviderResult<()> {
        self.inner.begin(Operation::ImportCertificate)
    }

    fn sign_ecc(&self, _digest: &[u8]) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::SignEcc)?;
        Ok(vec![0x11; 128])
    }

    fn sign_rsa(&self, _data: &[u8]) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::SignRsa)?;
        Ok(vec![0x22; 256])
    }

    fn gen_ecc_key_pair(&self, _alg_id: u32) -> ProviderResult<EccPublicKey> {
        self.inner.begin(Operation::GenEccKeyPair)?;
        Ok(EccPublicKey {
            bit_len: 256,
            x: vec![0x11; 64],
            y: vec![0x22; 64],
        })
    }

    fn gen_rsa_key_pair(&self, bits: u32) -> ProviderResult<RsaPublicKey> {
        self.inner.begin(Operation::GenRsaKeyPair)?;
        Ok(RsaPublicKey {
            alg_id: 0,
            bit_len: bits,
            modulus: vec![0x33; 256],
            exponent: vec![0x01, 0x00, 0x01, 0x00],
        })
    }

    fn set_symm_key(&self, _alg_id: u32, _key: &[u8]) -> ProviderResult<()> {
        self.inner.begin(Operation::SetSymmKey)
    }

    fn encrypt(
        &self,
        _alg_id: u32,
        _iv: &[u8],
        _padding: u32,
        data: &[u8],
    ) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::EncryptData)?;
        // Prefix marker so tests can prove the ciphertext is not the plaintext.
        let mut out = vec![0xEE];
        out.extend_from_slice(data);
        Ok(out)
    }

    fn decrypt(
        &self,
        _alg_id: u32,
        _iv: &[u8],
        _padding: u32,
        data: &[u8],
    ) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::DecryptData)?;
        if data.first() == Some(&0xEE) {
            Ok(data[1..].to_vec())
        } else {
            Ok(data.to_vec())
        }
    }
}

/// Fake digest guard. Dropping it records [`Operation::CloseDigest`].
pub struct FakeDigest {
    inner: std::sync::Arc<FakeInner>,
    exposed: DigestHandle,
}

impl Drop for FakeDigest {
    fn drop(&mut self) {
        self.inner.record(Operation::CloseDigest);
    }
}

impl DigestGuard for FakeDigest {
    fn handle(&self) -> DigestHandle {
        self.exposed
    }

    fn update(&self, _data: &[u8]) -> ProviderResult<()> {
        self.inner.begin(Operation::DigestUpdate)
    }

    fn finalize(&self) -> ProviderResult<Vec<u8>> {
        self.inner.begin(Operation::DigestFinal)?;
        Ok(self
            .inner
            .digest_output
            .lock()
            .expect("digest output")
            .clone())
    }
}

impl FakeSkfProvider {
    /// Start a digest on the fake provider.
    ///
    /// Mirrors [`crate::provider::native::begin_digest`] so tests can cover the
    /// streaming digest lifecycle without a token.
    pub fn begin_digest(&self) -> ProviderResult<Box<dyn DigestGuard>> {
        self.inner.begin(Operation::DigestBegin)?;
        Ok(Box::new(FakeDigest {
            inner: std::sync::Arc::clone(&self.inner),
            exposed: DigestHandle(self.inner.next_handle()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_devices_returns_configured_devices() {
        let provider = FakeSkfProvider::with_devices("FAKE", vec!["dev-a".into(), "dev-b".into()]);
        let devices = provider.enum_devices(true).expect("enum");
        assert_eq!(devices, vec!["dev-a".to_string(), "dev-b".to_string()]);
    }

    #[test]
    fn fail_next_is_consumed_once_then_succeeds() {
        let provider = FakeSkfProvider::with_devices("FAKE", vec!["dev-a".into()]);
        provider.fail_next(Operation::OpenDevice, SkfError::DeviceRemoved);

        let first = provider.open_device("dev-a");
        assert!(
            matches!(first, Err(ProviderError::Injected(SkfError::DeviceRemoved))),
            "the injected failure must surface on the first call"
        );

        let second = provider.open_device("dev-a");
        assert!(
            second.is_ok(),
            "the queue must be consumed, not permanently poisoned"
        );
    }

    #[test]
    fn wrong_pin_is_a_business_outcome_not_an_error() {
        let provider = FakeSkfProvider::new("FAKE");
        provider.fail_next(Operation::VerifyPin, SkfError::PinIncorrect);

        let device = provider.open_device("dev-a").expect("open");
        let app = device.open_application("app").expect("open app");
        let outcome = app.verify_pin("00000000").expect("verify must not error");

        assert!(!outcome.success);
        assert_eq!(outcome.retry_count, 2, "retry counter must be preserved");
    }

    #[test]
    fn missing_container_returns_an_error() {
        let provider = FakeSkfProvider::new("FAKE");
        provider.fail_next(Operation::OpenContainer, SkfError::ContainerMissing);

        let device = provider.open_device("dev-a").expect("open");
        let app = device.open_application("app").expect("open app");
        let result = app.open_container("missing");

        assert!(matches!(
            result,
            Err(ProviderError::Injected(SkfError::ContainerMissing))
        ));
    }

    #[test]
    fn symbol_unavailable_is_distinguishable() {
        let provider = FakeSkfProvider::new("FAKE");
        provider.fail_next(Operation::DigestBegin, SkfError::SymbolUnavailable);

        let device = provider.open_device("dev-a").expect("open");
        let result = device.begin_digest(0x00000001, b"");
        match result {
            Err(ProviderError::Injected(SkfError::SymbolUnavailable)) => {}
            other => panic!("expected SymbolUnavailable, got {:?}", other.err()),
        }
    }

    #[test]
    fn blocking_injection_delays_but_still_succeeds() {
        let provider = FakeSkfProvider::with_devices("FAKE", vec!["dev-a".into()]);
        provider.fail_next(
            Operation::EnumDevices,
            SkfError::Blocking(Duration::from_millis(50)),
        );

        let started = std::time::Instant::now();
        let devices = provider
            .enum_devices(true)
            .expect("must succeed after the delay");
        let elapsed = started.elapsed();

        assert_eq!(devices.len(), 1);
        assert!(
            elapsed >= Duration::from_millis(50),
            "the delay must actually be applied, elapsed {:?}",
            elapsed
        );
    }

    #[test]
    fn dropping_a_device_records_the_release() {
        let provider = FakeSkfProvider::new("FAKE");
        {
            let _device = provider.open_device("dev-a").expect("open");
            assert_eq!(provider.call_count(Operation::CloseDevice), 0);
        }
        assert_eq!(
            provider.call_count(Operation::CloseDevice),
            1,
            "Drop must record the release so Phase 2 can assert it"
        );
    }

    #[test]
    fn dropping_the_whole_chain_records_every_release() {
        let provider = FakeSkfProvider::new("FAKE");
        provider.set_containers(vec!["cont-a".into()]);
        {
            let device = provider.open_device("dev-a").expect("open");
            let app = device.open_application("app").expect("open app");
            let _container = app.open_container("cont-a").expect("open container");
        }
        assert_eq!(provider.call_count(Operation::CloseContainer), 1);
        assert_eq!(provider.call_count(Operation::CloseApplication), 1);
        assert_eq!(provider.call_count(Operation::CloseDevice), 1);
    }

    #[test]
    fn digest_guard_records_close_on_drop() {
        let provider = FakeSkfProvider::new("FAKE");
        provider.set_digest_output(vec![0xCD; 32]);
        {
            let device = provider.open_device("dev-a").expect("open");
            let digest = device.begin_digest(0x00000001, b"").expect("begin");
            digest.update(b"hello").expect("update");
            assert_eq!(digest.finalize().expect("finalize"), vec![0xCD; 32]);
        }
        assert_eq!(provider.call_count(Operation::CloseDigest), 1);
    }

    /// Regression guard for review finding M-01.
    ///
    /// Before the fix, starting a digest required the concrete device type, so a
    /// caller holding a `Box<dyn DeviceGuard>` — which is what
    /// `SkfProvider::open_device` returns — could not reach the digest API at all.
    #[test]
    fn digest_can_be_started_through_a_trait_object_device() {
        let provider = FakeSkfProvider::new("FAKE");
        let device: Box<dyn DeviceGuard> = provider.open_device("dev-a").expect("open");

        let digest = device
            .begin_digest(0x00000001, b"")
            .expect("a trait-object device must be able to start a digest");

        assert!(digest.handle().0 >= FIRST_HANDLE);
        drop(digest);
        assert_eq!(provider.call_count(Operation::CloseDigest), 1);
    }

    #[test]
    fn handles_are_deterministic_and_increasing() {
        let provider = FakeSkfProvider::new("FAKE");
        let first = provider.open_device("a").expect("open").handle();
        let second = provider.open_device("b").expect("open").handle();
        assert_eq!(first.0, FIRST_HANDLE);
        assert_eq!(second.0, FIRST_HANDLE + 1);
    }
}
