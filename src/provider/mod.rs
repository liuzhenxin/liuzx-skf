//! Provider abstraction over the vendor SKF C ABI.
//!
//! # Why this module exists
//!
//! Before Phase 1, every request handler called `SkfApi` directly with raw
//! pointers, and ten of those call paths loaded (and then dropped) the vendor
//! library per request. That made the code untestable without hardware and left
//! native handle lifetimes unowned.
//!
//! This module draws one boundary:
//!
//! * [`SkfProvider`] expresses **business operations**, not a mirror of the C
//!   symbol table.
//! * Handles crossing the boundary are **provider-defined newtypes**
//!   ([`DeviceHandle`], [`AppHandle`], [`ContainerHandle`], [`DigestHandle`]), so
//!   a native pointer never escapes.
//! * Resources are returned as **guards** whose `Drop` releases the underlying
//!   handle, which is what makes deterministic release achievable in Phase 2.
//!
//! # Two invariants this module must keep
//!
//! 1. **No native pointer may appear in this file.** `HANDLE`, `DEVHANDLE`,
//!    `HAPPLICATION`, `HCONTAINER`, `SendHandle`, and `*mut c_void` belong to
//!    [`crate::provider::native`] only. `tests/provider_invariants.rs` enforces
//!    this because Phase 2's opaque-identifier work depends on it.
//! 2. **The provider owns the library for its lifetime.** Implementations must
//!    load the vendor library exactly once.
//!
//! Production routing is deliberately incomplete: Phase 1 wires only the
//! self-contained enumeration/state operations (decision D-06). The remaining
//! request branches migrate in Phase 2.

use std::time::Duration;

/// A device resource owned by a provider implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceHandle(pub u64);

/// An open application on a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppHandle(pub u64);

/// An open container inside an application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContainerHandle(pub u64);

/// A streaming digest (hash) operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DigestHandle(pub u64);

/// Every provider operation that can be individually observed.
///
/// Used for two purposes: recording the call sequence (so tests can assert that
/// resources were released) and targeting per-call failure injection in the fake
/// implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Operation {
    // Device lifecycle
    EnumDevices,
    DeviceState,
    WaitForEvent,
    CancelWaitForEvent,
    OpenDevice,
    CloseDevice,
    LockDevice,
    UnlockDevice,
    Transmit,
    Random,
    DeviceInfo,
    SetLabel,
    // Application lifecycle
    EnumApplications,
    OpenApplication,
    CloseApplication,
    VerifyPin,
    // Container lifecycle
    EnumContainers,
    OpenContainer,
    CloseContainer,
    CreateContainer,
    DeleteContainer,
    ContainerType,
    ExportCertificate,
    ImportCertificate,
    // Cryptography
    SignEcc,
    SignRsa,
    EccVerify,
    RsaVerify,
    DigestBegin,
    DigestUpdate,
    DigestFinal,
    CloseDigest,
    GenEccKeyPair,
    GenRsaKeyPair,
    ImportEccKeyPair,
    ImportRsaKeyPair,
    SetSymmKey,
    EncryptData,
    DecryptData,
}

/// A failure the fake implementation can inject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkfError {
    /// The supplied PIN was rejected.
    PinIncorrect,
    /// The token is locked out after too many failed attempts.
    PinLocked,
    /// The requested container does not exist.
    ContainerMissing,
    /// The vendor library does not export the required symbol.
    SymbolUnavailable,
    /// The device disappeared mid-operation.
    DeviceRemoved,
    /// Simulates a slow vendor call. The operation still succeeds after the delay;
    /// this exists so Phase 3 can prove a stalled token does not block others.
    Blocking(Duration),
    /// Any specific vendor return code.
    Custom(u32),
}

/// Provider-level error.
///
/// The native return code is preserved rather than flattened, because callers
/// surface it in RPC responses and the recorded v0.2.0 contract asserts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// A vendor call returned a non-zero code.
    Native {
        code: u32,
        context: &'static str,
    },
    /// The vendor library does not export the requested symbol.
    SymbolUnavailable { name: &'static str },
    /// The vendor library could not be loaded.
    LibraryLoadFailed {
        path: String,
        arch: &'static str,
        detail: String,
    },
    /// The provider alias has no library path for the current operating system.
    ProviderNotConfigured { alias: String, os: &'static str },
    /// A failure injected by the fake implementation.
    Injected(SkfError),
    /// The caller supplied an argument the provider cannot use (for example a
    /// device name containing an interior NUL byte).
    InvalidArgument { context: &'static str },
}

impl ProviderError {
    /// Build a [`ProviderError::Native`] from a vendor return code.
    ///
    /// `SAR_COULDNOTGETFUNCADDR` is normalised to
    /// [`ProviderError::SymbolUnavailable`] so callers do not have to special
    /// case it, matching the pre-refactor behaviour.
    pub fn from_native(code: u32, context: &'static str) -> Self {
        if code == crate::skf::types::SAR_COULDNOTGETFUNCADDR {
            return ProviderError::SymbolUnavailable { name: context };
        }
        ProviderError::Native { code, context }
    }

    /// The underlying vendor return code, when there is one.
    pub fn hex(&self) -> Option<u32> {
        match self {
            ProviderError::Native { code, .. } => Some(*code),
            ProviderError::Injected(SkfError::Custom(code)) => Some(*code),
            _ => None,
        }
    }
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Native { code, context } => {
                write!(f, "{} failed: 0x{:08X}", context, code)
            }
            ProviderError::SymbolUnavailable { name } => {
                write!(f, "symbol '{}' is not available in the loaded library", name)
            }
            ProviderError::LibraryLoadFailed { path, arch, detail } => write!(
                f,
                "Failed to load library '{}' (process architecture: {}): {}",
                path, arch, detail
            ),
            ProviderError::ProviderNotConfigured { alias, os } => {
                write!(f, "Provider '{}' not configured for OS '{}'", alias, os)
            }
            ProviderError::Injected(err) => write!(f, "injected failure: {:?}", err),
            ProviderError::InvalidArgument { context } => {
                write!(f, "invalid argument in {}", context)
            }
        }
    }
}

impl std::error::Error for ProviderError {}

/// Result alias for provider operations.
pub type ProviderResult<T> = Result<T, ProviderError>;

/// Outcome of a PIN verification.
///
/// A wrong PIN is a normal business result, not an error: returning `Err` here
/// would lose the retry counter that the contract exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinOutcome {
    pub success: bool,
    pub retry_count: u32,
}

/// A device resource. Dropping the guard releases the native handle.
pub trait DeviceGuard: Send {
    fn handle(&self) -> DeviceHandle;
    fn state(&self) -> ProviderResult<u32>;
    fn lock(&self, timeout: Duration) -> ProviderResult<()>;
    fn unlock(&self) -> ProviderResult<()>;
    fn transmit(&self, command: &[u8]) -> ProviderResult<Vec<u8>>;
    fn random(&self, len: usize) -> ProviderResult<Vec<u8>>;
    fn info(&self) -> ProviderResult<DeviceInfo>;
    fn set_label(&self, label: &str) -> ProviderResult<()>;
    fn open_application(&self, name: &str) -> ProviderResult<Box<dyn ApplicationGuard>>;
}

/// An open application. Dropping the guard closes it.
pub trait ApplicationGuard: Send {
    fn handle(&self) -> AppHandle;
    fn enum_containers(&self) -> ProviderResult<Vec<String>>;
    fn verify_pin(&self, pin: &str) -> ProviderResult<PinOutcome>;
    fn open_container(&self, name: &str) -> ProviderResult<Box<dyn ContainerGuard>>;
    fn delete_container(&self, name: &str) -> ProviderResult<()>;
}

/// An open container. Dropping the guard closes it.
pub trait ContainerGuard: Send {
    fn handle(&self) -> ContainerHandle;
    fn container_type(&self) -> ProviderResult<u32>;
    fn export_certificate(&self, sign_flag: bool) -> ProviderResult<Vec<u8>>;
    fn import_certificate(&self, sign_flag: bool, cert: &[u8]) -> ProviderResult<()>;
    fn sign_ecc(&self, digest: &[u8]) -> ProviderResult<Vec<u8>>;
    fn sign_rsa(&self, data: &[u8]) -> ProviderResult<Vec<u8>>;
    fn set_symm_key(&self, alg_id: u32, key: &[u8]) -> ProviderResult<()>;
    fn encrypt(&self, alg_id: u32, iv: &[u8], padding: u32, data: &[u8]) -> ProviderResult<Vec<u8>>;
    fn decrypt(&self, alg_id: u32, iv: &[u8], padding: u32, data: &[u8]) -> ProviderResult<Vec<u8>>;
}

/// A streaming digest. Dropping the guard closes both the hash and its device.
pub trait DigestGuard: Send {
    fn handle(&self) -> DigestHandle;
    fn update(&self, data: &[u8]) -> ProviderResult<()>;
    fn finalize(&self) -> ProviderResult<Vec<u8>>;
}

/// Device identity and capability data.
///
/// Strings are decoded from fixed-size vendor buffers; they are `String` rather
/// than `&CStr` so the guard keeps no borrow of vendor memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub manufacturer: String,
    pub issuer: String,
    pub label: String,
    pub serial_number: String,
    pub total_space: u32,
    pub free_space: u32,
    pub max_ecc_buffer_size: u32,
    pub max_buffer_size: u32,
}

/// The vendor-neutral provider surface.
///
/// # Implementation contract
///
/// * The vendor library must be loaded **exactly once** and owned for the
///   provider's lifetime. Re-loading per call invalidates handles that earlier
///   calls handed out.
/// * Every guard returned here owns its native handle and must release it in
///   `Drop`, including on error paths.
/// * No native pointer may appear in a signature or a guard type.
/// * Implementations are shared across connections, so `Send + Sync` is required;
///   serialisation of non-thread-safe vendor calls is the implementation's
///   responsibility (Phase 3).
pub trait SkfProvider: Send + Sync {
    /// Provider alias as it appears in configuration.
    fn alias(&self) -> &str;

    /// Enumerate devices known to the vendor library.
    fn enum_devices(&self, present_only: bool) -> ProviderResult<Vec<String>>;

    /// Query the state code of a named device without opening it.
    fn device_state(&self, name: &str) -> ProviderResult<u32>;

    /// Block until a device event arrives, returning `(device_name, event_code)`.
    ///
    /// Returns the vendor error when no event occurs; a cancelled wait surfaces
    /// as the vendor's "device not found"-style code rather than a hang.
    fn wait_for_event(&self, buf_len: usize) -> ProviderResult<(String, u32)>;

    /// Cancel an in-flight [`SkfProvider::wait_for_event`].
    ///
    /// The vendor call is process-global, so cancelling works from a different
    /// connection than the one waiting.
    fn cancel_wait_for_event(&self) -> ProviderResult<()>;

    /// Open a device, transferring handle ownership to the returned guard.
    fn open_device(&self, name: &str) -> ProviderResult<Box<dyn DeviceGuard>>;
}

/// Deterministic in-memory provider for tests.
///
/// Feature-gated: `cfg(test)` covers unit tests inside the library, and the
/// `test-provider` feature covers integration tests (which are separate crates
/// and therefore never see `cfg(test)`).
#[cfg(any(test, feature = "test-provider"))]
pub mod fake;

pub mod native;
