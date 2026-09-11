//! Security-relevant request handlers, migrated out of the binary dispatcher.
//!
//! # Why this layer exists
//!
//! Before plan 02-04 the twelve handlers that touch a token — containers,
//! certificates, signing, key generation and PIN verification — lived in
//! `src/main.rs` and reached the vendor library directly. Every one of them
//! hand-closed its native handles on each success path and, in several cases,
//! leaked them on the error paths. Moving them here makes two things possible:
//!
//! * **Release on every path is structural.** Each handler resolves resources as
//!   provider guards. A guard releases its handle in `Drop`, so a `?`-style early
//!   return releases exactly what was acquired — that is requirement RES-04.
//! * **The binary is a composition root.** `main.rs` keeps the dispatcher, the
//!   session implementation and the fourteen branches that are deliberately not
//!   migrated this phase (decision D-17); the security work lives here.
//!
//! # Authority comes from the session
//!
//! A handler never reads a process-global PIN cache. It asks the caller-supplied
//! [`SessionState`] whether *this* session holds a live grant for the
//! `(provider, device, application)` triple, and a successful `CheckPIN` records
//! only a deadline. The two operations that observe a device disappearing clear
//! that session's grants for the device.
//!
//! # Alias resolution
//!
//! A handler names a provider alias; it does not load a library. [`ServerContext`]
//! resolves the alias to a provider, which is what keeps vendor-library loading
//! out of this module entirely.

pub mod container;
pub mod crypto;
pub mod keys;
pub mod pin;

use std::sync::Arc;

use crate::protocol::{Language, RpcResponse};
use crate::provider::{EccPublicKey, ProviderError, RsaPublicKey, SkfProvider};
use crate::skf::types::{ECCPUBLICKEYBLOB, RSAPUBLICKEYBLOB};
use crate::session::auth::{AuthKey, AuthRejection};
use crate::session::SessionState;

/// Resolves a provider alias for a handler.
///
/// Implemented by the binary's process-wide context. The trait exists so the
/// domain layer never names the binary's `SkfContext`, and so a test can supply a
/// fake without a vendor library.
pub trait ServerContext: Send + Sync {
    /// Alias a request means when it asks for `"default"` (or omits the alias).
    fn default_alias(&self) -> &str;

    /// Resolve an alias to a provider, or return a message suitable for the
    /// `Load Lib Failed` response.
    fn resolve(&self, alias: &str) -> Result<Arc<dyn SkfProvider>, String>;
}

/// The `Load Lib Failed` response shared by most migrated branches.
pub fn load_failed(message: String, id: Option<serde_json::Value>) -> RpcResponse {
    RpcResponse::err(-5, format!("Load Lib Failed: {}", message), id)
}

/// The localized `Load Lib Failed` response used by the key-generation branches.
pub fn load_failed_localized(
    message: String,
    lang: &Language,
    id: Option<serde_json::Value>,
) -> RpcResponse {
    let text = match lang {
        Language::CN => format!("加载库失败: {}", message),
        Language::EN => format!("Load Lib Failed: {}", message),
    };
    RpcResponse::err(-1, text, id)
}

/// Normalise the provider alias the way the container/CSR branches always did.
///
/// An absent alias, an empty string, or the literal `"default"` all mean the
/// configured default. `DeleteContainer` and `ImportCertificate` deliberately do
/// **not** call this: they treated an empty string as a real (unconfigured) alias.
pub fn normalize_alias<'a>(ctx: &'a dyn ServerContext, requested: &'a str) -> &'a str {
    if requested.is_empty() || requested == "default" {
        ctx.default_alias()
    } else {
        requested
    }
}

/// Confirm this session holds a live authorization for one application.
pub fn authorized(
    state: &mut SessionState,
    provider: &str,
    device: &str,
    application: &str,
) -> Result<(), AuthRejection> {
    state.authorize(&AuthKey::new(provider, device, application))
}

/// Clear this session's grants for a device an operation just found missing.
pub fn note_device_unavailable(state: &mut SessionState, provider: &str, device: &str) {
    let removed = state.invalidate_device(provider, device);
    if removed > 0 {
        log::info!(
            "device {}/{} is unavailable; cleared {} authorization grant(s)",
            provider,
            device,
            removed
        );
    }
}

/// The native return code behind a provider failure, when there is one.
pub fn native_code(err: &ProviderError) -> Option<u32> {
    match err {
        ProviderError::Native { code, .. } => Some(*code),
        ProviderError::Injected(crate::provider::SkfError::Custom(code)) => Some(*code),
        _ => None,
    }
}

/// Materialise a provider [`EccPublicKey`] as the native blob the SPKI builders
/// and the wire encoding use.
///
/// The provider returns a value type; the two callers that need the vendor layout
/// (the CSR builder and the `GenECCKeyPair` wire encoding) reconstruct it here, so
/// the layout stays a binary-crate concern.
pub fn ecc_blob(key: &EccPublicKey) -> ECCPUBLICKEYBLOB {
    let mut blob: ECCPUBLICKEYBLOB = unsafe { std::mem::zeroed() };
    blob.BitLen = key.bit_len;
    let x_len = key.x.len().min(blob.XCoordinate.len());
    blob.XCoordinate[..x_len].copy_from_slice(&key.x[..x_len]);
    let y_len = key.y.len().min(blob.YCoordinate.len());
    blob.YCoordinate[..y_len].copy_from_slice(&key.y[..y_len]);
    blob
}

/// Materialise a provider [`RsaPublicKey`] as the native blob the SPKI builder
/// and the wire encoding use.
pub fn rsa_blob(key: &RsaPublicKey) -> RSAPUBLICKEYBLOB {
    let mut blob: RSAPUBLICKEYBLOB = unsafe { std::mem::zeroed() };
    blob.AlgID = key.alg_id;
    blob.BitLen = key.bit_len;
    let n_len = key.modulus.len().min(blob.Modulus.len());
    blob.Modulus[..n_len].copy_from_slice(&key.modulus[..n_len]);
    let e_len = key.exponent.len().min(blob.PublicExponent.len());
    blob.PublicExponent[..e_len].copy_from_slice(&key.exponent[..e_len]);
    blob
}

/// The raw bytes of a plain-old-data native blob, matching the pre-refactor
/// `from_raw_parts` encoding used by the key-generation responses.
pub fn struct_bytes<T>(value: &T) -> Vec<u8> {
    let ptr = value as *const T as *const u8;
    // Safety: `ptr` points at `value`, which is a live `T`; the slice covers
    // exactly `size_of::<T>()` readable bytes.
    unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of::<T>()).to_vec() }
}

/// Test doubles for the domain handlers.
///
/// Available to library unit tests and, through the `test-provider` feature, to
/// integration tests. Never compiled into a release build.
#[cfg(any(test, feature = "test-provider"))]
pub mod test_support {
    use std::sync::Arc;

    use crate::provider::SkfProvider;

    use super::ServerContext;

    /// A context that resolves every alias to one injected provider.
    pub struct TestContext {
        provider: Arc<dyn SkfProvider>,
        alias: String,
    }

    impl TestContext {
        pub fn new(provider: Arc<dyn SkfProvider>) -> Self {
            let alias = provider.alias().to_string();
            Self { provider, alias }
        }
    }

    impl ServerContext for TestContext {
        fn default_alias(&self) -> &str {
            &self.alias
        }

        fn resolve(&self, _alias: &str) -> Result<Arc<dyn SkfProvider>, String> {
            Ok(Arc::clone(&self.provider))
        }
    }
}

/// Split a `certKey` of the form `provider/device/app/container[/serial]`.
///
/// Returns `None` when fewer than `min` segments are present, letting each branch
/// keep its own error message.
pub fn split_cert_key(cert_key: &str, min: usize) -> Option<Vec<&str>> {
    let parts: Vec<&str> = cert_key.splitn(5, '/').collect();
    if parts.len() < min {
        None
    } else {
        Some(parts)
    }
}
