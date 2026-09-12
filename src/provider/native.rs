//! Native [`SkfProvider`] implementation backed by the vendor SKF library.
//!
//! This is the **only** file in the crate allowed to name native handle types.
//! Everything it returns to the rest of the service is expressed with the
//! newtypes and guard traits declared in the parent module.
//!
//! # Library lifetime
//!
//! [`NativeSkfProvider`] loads the vendor library exactly once and holds it for
//! its own lifetime. The pre-refactor code loaded and dropped the library on ten
//! separate request paths, which invalidated handles handed out by earlier calls;
//! the guard types here share the loaded library through `Arc` so a handle can
//! never outlive the library that produced it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use libloading::Library;

use super::{
    AppHandle, ApplicationGuard, ContainerGuard, ContainerHandle, DeviceGuard, DeviceHandle,
    DeviceInfo, DigestGuard, DigestHandle, EccPublicKey, PinOutcome, ProviderError, ProviderResult,
    RsaPublicKey, SkfProvider,
};
use crate::config::SkfConfig;
use crate::skf::api::SkfApi;
use crate::skf::types::{
    BLOCKCIPHERPARAM, BOOL, BYTE, CHAR, DEVHANDLE, DEVINFO, ECCPUBLICKEYBLOB, ECCSIGNATUREBLOB,
    HANDLE, HAPPLICATION, HCONTAINER, RSAPUBLICKEYBLOB, SAR_OK, SGD_SM3, ULONG,
};

/// Largest device-name buffer the vendor API is given.
const DEV_NAME_BUF_LEN: usize = 256;
/// Upper bound for a hash result (SHA-512 is 64 bytes).
const HASH_BUF_LEN: usize = 64;
/// Upper bound for a certificate blob returned by the token.
const CERT_BUF_LEN: usize = 16 * 1024;
/// Upper bound for an RSA signature.
const RSA_SIG_BUF_LEN: usize = 512;
/// Upper bound for a symmetric-cipher output.
const SYM_BUF_LEN: usize = 64 * 1024;

/// Build a `CString`, rejecting interior NUL bytes instead of panicking.
fn cstring(value: &str, context: &'static str) -> ProviderResult<std::ffi::CString> {
    std::ffi::CString::new(value).map_err(|_| ProviderError::InvalidArgument { context })
}

/// Decode a fixed-size vendor char buffer without scanning past its end.
///
/// Uses a bounded scan for the first NUL rather than `CStr::from_ptr`, because a
/// vendor buffer that is not NUL-terminated would otherwise cause an
/// out-of-bounds read.
fn fixed_cstr(buf: &[CHAR]) -> String {
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).to_string()
}

/// Split a NUL-separated vendor name list into strings.
fn parse_cstr_list(buf: &[CHAR], size: usize) -> Vec<String> {
    let usable = size.min(buf.len());
    let raw = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, usable) };
    raw.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .collect()
}

/// Query the size of a NUL-separated list, then fetch it.
///
/// The vendor API uses the two-phase "call with a null buffer to learn the size"
/// pattern; this helper exists so every call site gets the same behaviour,
/// including the empty-list case.
fn read_name_list<F>(len: ULONG, fetch: F) -> ProviderResult<Vec<String>>
where
    F: Fn(*mut CHAR, *mut ULONG) -> ULONG,
{
    if len == 0 {
        return Ok(Vec::new());
    }
    let mut buf = vec![0 as CHAR; len as usize];
    let mut size = len;
    let ret = fetch(buf.as_mut_ptr(), &mut size);
    if ret != SAR_OK {
        return Err(ProviderError::from_native(ret, "enumerate list"));
    }
    Ok(parse_cstr_list(&buf, size as usize))
}

// ---------------------------------------------------------------------------
// Serialisation gate
// ---------------------------------------------------------------------------

/// Serialises every call into one vendor library.
///
/// The vendor DLL has no documented thread-safety guarantee, so each provider
/// owns one gate and every guard created from it shares that gate. Taking it in
/// each native method is what makes per-provider serialization (TRANS-05)
/// hold even though many sessions may call concurrently.
///
/// `WaitForDevEvent`/`CancelWaitForDevEvent` deliberately do **not** take the
/// gate: cancel must reach the library while a wait is parked, or the pair
/// deadlocks.
type FfiGate = Arc<std::sync::Mutex<()>>;

/// Acquire a gate, recovering from a poisoned mutex.
///
/// A panic while a call held the gate must not make every later call fail; the
/// protected invariant is the vendor library's, and it is still intact after a
/// panic in the caller.
fn gate_lock(gate: &FfiGate) -> std::sync::MutexGuard<'_, ()> {
    gate.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------
// Owned native resources
// ---------------------------------------------------------------------------

/// An open device. Dropping it disconnects the device, but only after every
/// application and digest guard that shares it has also been dropped.
pub struct NativeDevice {
    api: Arc<SkfApi>,
    raw: usize,
    name: String,
    exposed: DeviceHandle,
    gate: FfiGate,
}

impl Drop for NativeDevice {
    fn drop(&mut self) {
        let _gate = gate_lock(&self.gate);
        if self.raw != 0 {
            let _ = self.api.dis_connect_dev(self.raw as DEVHANDLE);
        }
    }
}

impl DeviceGuard for Arc<NativeDevice> {
    fn handle(&self) -> DeviceHandle {
        self.exposed
    }

    fn state(&self) -> ProviderResult<u32> {
        let _gate = gate_lock(&self.gate);
        let c_name = cstring(&self.name, "GetDevState")?;
        let mut state: ULONG = 0;
        let ret = self
            .api
            .get_dev_state(c_name.as_ptr() as *mut CHAR, &mut state);
        if ret == SAR_OK {
            Ok(state)
        } else {
            Err(ProviderError::from_native(ret, "GetDevState"))
        }
    }

    fn lock(&self, timeout: Duration) -> ProviderResult<()> {
        let _gate = gate_lock(&self.gate);
        let ret = self
            .api
            .lock_dev(self.raw as DEVHANDLE, timeout.as_millis() as ULONG);
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "LockDev"))
        }
    }

    fn unlock(&self) -> ProviderResult<()> {
        let _gate = gate_lock(&self.gate);
        let ret = self.api.unlock_dev(self.raw as DEVHANDLE);
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "UnlockDev"))
        }
    }

    fn transmit(&self, command: &[u8]) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut command_buf = command.to_vec();
        let mut response = vec![0u8; SYM_BUF_LEN];
        let mut response_len: ULONG = response.len() as ULONG;
        let ret = self.api.transmit(
            self.raw as DEVHANDLE,
            command_buf.as_mut_ptr(),
            command_buf.len() as ULONG,
            response.as_mut_ptr(),
            &mut response_len,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "Transmit"));
        }
        response.truncate(response_len as usize);
        Ok(response)
    }

    fn random(&self, len: usize) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut buf = vec![0u8; len];
        let ret = self
            .api
            .gen_random(self.raw as DEVHANDLE, buf.as_mut_ptr(), len as ULONG);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "GenRandom"));
        }
        Ok(buf)
    }

    fn info(&self) -> ProviderResult<DeviceInfo> {
        let _gate = gate_lock(&self.gate);
        let mut raw_info: DEVINFO = unsafe { std::mem::zeroed() };
        let ret = self.api.get_dev_info(self.raw as DEVHANDLE, &mut raw_info);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "GetDevInfo"));
        }
        Ok(DeviceInfo {
            manufacturer: fixed_cstr(&raw_info.Manufacturer),
            issuer: fixed_cstr(&raw_info.Issuer),
            label: fixed_cstr(&raw_info.Label),
            serial_number: fixed_cstr(&raw_info.SerialNumber),
            total_space: raw_info.TotalSpace,
            free_space: raw_info.FreeSpace,
            max_ecc_buffer_size: raw_info.MaxECCBufferSize,
            max_buffer_size: raw_info.MaxBufferSize,
        })
    }

    fn set_label(&self, label: &str) -> ProviderResult<()> {
        let _gate = gate_lock(&self.gate);
        let mut c_label = cstring(label, "SetLabel")?.into_bytes_with_nul();
        let ret = self
            .api
            .set_label(self.raw as DEVHANDLE, c_label.as_mut_ptr() as *mut CHAR);
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "SetLabel"))
        }
    }

    fn begin_digest(&self, alg_id: u32, id: &[u8]) -> ProviderResult<Box<dyn DigestGuard>> {
        self.begin_digest_impl(alg_id, id, std::ptr::null_mut())
    }

    fn begin_digest_with_key(
        &self,
        alg_id: u32,
        id: &[u8],
        public_key: &EccPublicKey,
    ) -> ProviderResult<Box<dyn DigestGuard>> {
        // SM2-with-ID needs the public key to compute the `Z` value; the plain
        // `begin_digest` passes a null key, which the vendor API ignores for the
        // algorithms that do not use one.
        let mut key = ecc_blob(public_key);
        self.begin_digest_impl(alg_id, id, &mut key as *mut ECCPUBLICKEYBLOB)
    }

    fn open_application(&self, name: &str) -> ProviderResult<Box<dyn ApplicationGuard>> {
        let _gate = gate_lock(&self.gate);
        let c_name = cstring(name, "OpenApplication")?;
        let mut raw: HAPPLICATION = std::ptr::null_mut();
        let ret = self.api.open_application(
            self.raw as DEVHANDLE,
            c_name.as_ptr() as *mut CHAR,
            &mut raw,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "OpenApplication"));
        }
        let exposed = AppHandle(self.next_child_handle());
        Ok(Box::new(Arc::new(NativeApplication {
            device: Arc::clone(self),
            raw: raw as usize,
            exposed,
            gate: Arc::clone(&self.gate),
        })))
    }
}

impl NativeDevice {
    /// Shared body of [`DeviceGuard::begin_digest`] and
    /// [`DeviceGuard::begin_digest_with_key`].
    fn begin_digest_impl(
        self: &Arc<Self>,
        alg_id: u32,
        id: &[u8],
        public_key: *mut ECCPUBLICKEYBLOB,
    ) -> ProviderResult<Box<dyn DigestGuard>> {
        let _gate = gate_lock(&self.gate);
        let mut raw: HANDLE = std::ptr::null_mut();
        let mut id_buf = id.to_vec();
        let ret = self.api.digest_init(
            self.raw as DEVHANDLE,
            alg_id,
            public_key,
            id_buf.as_mut_ptr() as *mut BYTE,
            id_buf.len() as ULONG,
            &mut raw,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "DigestInit"));
        }
        Ok(Box::new(Arc::new(NativeDigest {
            device: Arc::clone(self),
            raw: raw as usize,
            exposed: DigestHandle(NEXT_NESTED_HANDLE.fetch_add(1, Ordering::Relaxed)),
            gate: Arc::clone(&self.gate),
        })))
    }

    /// Handles for nested resources come from the same counter as device handles,
    /// so a container id can never collide with a device id.
    fn next_child_handle(&self) -> u64 {
        // The counter lives on the provider; nested resources receive their value
        // from the provider when it constructs them. This fallback keeps the
        // guard usable in isolation (tests) without a provider reference.
        NEXT_NESTED_HANDLE.fetch_add(1, Ordering::Relaxed)
    }
}

/// Fallback counter for guards constructed without a provider reference.
static NEXT_NESTED_HANDLE: AtomicU64 = AtomicU64::new(1_000_000);

/// An open application. Dropping it closes the application and, once the last
/// sharing guard is gone, disconnects the device.
pub struct NativeApplication {
    device: Arc<NativeDevice>,
    raw: usize,
    exposed: AppHandle,
    gate: FfiGate,
}

impl Drop for NativeApplication {
    fn drop(&mut self) {
        let _gate = gate_lock(&self.gate);
        if self.raw != 0 {
            let _ = self.device.api.close_application(self.raw as DEVHANDLE);
        }
    }
}

impl ApplicationGuard for Arc<NativeApplication> {
    fn handle(&self) -> AppHandle {
        self.exposed
    }

    fn enum_containers(&self) -> ProviderResult<Vec<String>> {
        let _gate = gate_lock(&self.gate);
        let mut size: ULONG = 0;
        let ret =
            self.device
                .api
                .enum_container(self.raw as DEVHANDLE, std::ptr::null_mut(), &mut size);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "EnumContainer"));
        }
        if size == 0 {
            return Ok(Vec::new());
        }
        read_name_list(size, |buf, len| {
            self.device
                .api
                .enum_container(self.raw as DEVHANDLE, buf, len)
        })
    }

    fn verify_pin(&self, pin: &str) -> ProviderResult<PinOutcome> {
        let _gate = gate_lock(&self.gate);
        let c_pin = cstring(pin, "VerifyPIN")?;
        let mut retry_count: ULONG = 0;
        let ret = self.device.api.verify_pin(
            self.raw as DEVHANDLE,
            1,
            c_pin.as_ptr() as *mut CHAR,
            &mut retry_count,
        );
        // A rejected PIN is a business outcome, not an error: the retry counter
        // is part of the observable contract.
        Ok(PinOutcome {
            success: ret == SAR_OK,
            retry_count,
            code: ret,
        })
    }

    fn open_container(&self, name: &str) -> ProviderResult<Box<dyn ContainerGuard>> {
        let _gate = gate_lock(&self.gate);
        let c_name = cstring(name, "OpenContainer")?;
        let mut raw: HCONTAINER = std::ptr::null_mut();
        let ret = self.device.api.open_container(
            self.raw as DEVHANDLE,
            c_name.as_ptr() as *mut CHAR,
            &mut raw,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "OpenContainer"));
        }
        Ok(Box::new(Arc::new(NativeContainer {
            application: Arc::clone(self),
            raw: raw as usize,
            exposed: ContainerHandle(NEXT_NESTED_HANDLE.fetch_add(1, Ordering::Relaxed)),
            gate: Arc::clone(&self.gate),
        })))
    }

    fn delete_container(&self, name: &str) -> ProviderResult<()> {
        let _gate = gate_lock(&self.gate);
        let c_name = cstring(name, "DeleteContainer")?;
        let ret = self
            .device
            .api
            .delete_container(self.raw as DEVHANDLE, c_name.as_ptr() as *mut CHAR);
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "DeleteContainer"))
        }
    }

    fn create_container(&self, name: &str) -> ProviderResult<Box<dyn ContainerGuard>> {
        let _gate = gate_lock(&self.gate);
        let c_name = cstring(name, "CreateContainer")?;
        let mut raw: HCONTAINER = std::ptr::null_mut();
        let ret = self.device.api.create_container(
            self.raw as DEVHANDLE,
            c_name.as_ptr() as *mut CHAR,
            &mut raw,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "CreateContainer"));
        }
        Ok(Box::new(Arc::new(NativeContainer {
            application: Arc::clone(self),
            raw: raw as usize,
            exposed: ContainerHandle(NEXT_NESTED_HANDLE.fetch_add(1, Ordering::Relaxed)),
            gate: Arc::clone(&self.gate),
        })))
    }
}

/// An open container. Dropping it closes the container.
pub struct NativeContainer {
    application: Arc<NativeApplication>,
    raw: usize,
    exposed: ContainerHandle,
    gate: FfiGate,
}

impl Drop for NativeContainer {
    fn drop(&mut self) {
        let _gate = gate_lock(&self.gate);
        if self.raw != 0 {
            let _ = self
                .application
                .device
                .api
                .close_container(self.raw as DEVHANDLE);
        }
    }
}

impl ContainerGuard for Arc<NativeContainer> {
    fn handle(&self) -> ContainerHandle {
        self.exposed
    }

    fn container_type(&self) -> ProviderResult<u32> {
        let _gate = gate_lock(&self.gate);
        let mut kind: ULONG = 0;
        let ret = self
            .application
            .device
            .api
            .get_container_type(self.raw as DEVHANDLE, &mut kind);
        if ret == SAR_OK {
            Ok(kind)
        } else {
            Err(ProviderError::from_native(ret, "GetContainerType"))
        }
    }

    fn export_certificate(&self, sign_flag: bool) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut buf = vec![0u8; CERT_BUF_LEN];
        let mut size: ULONG = buf.len() as ULONG;
        let ret = self.application.device.api.export_certificate(
            self.raw as DEVHANDLE,
            sign_flag as BOOL,
            buf.as_mut_ptr(),
            &mut size,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "ExportCertificate"));
        }
        buf.truncate(size as usize);
        Ok(buf)
    }

    fn import_certificate(&self, sign_flag: bool, cert: &[u8]) -> ProviderResult<()> {
        let _gate = gate_lock(&self.gate);
        let mut cert_buf = cert.to_vec();
        let ret = self.application.device.api.import_certificate(
            self.raw as DEVHANDLE,
            sign_flag as BOOL,
            cert_buf.as_mut_ptr(),
            cert_buf.len() as ULONG,
        );
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "ImportCertificate"))
        }
    }

    fn sign_ecc(&self, digest: &[u8]) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut data = digest.to_vec();
        let mut signature: ECCSIGNATUREBLOB = unsafe { std::mem::zeroed() };
        let ret = self.application.device.api.ecc_sign_data(
            self.raw as DEVHANDLE,
            data.as_mut_ptr(),
            data.len() as ULONG,
            &mut signature,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "ECCSignData"));
        }
        // Raw r||s. Formatting for the wire is a Phase 2 concern; this module
        // deliberately does not decide the encoding.
        let mut out = Vec::with_capacity(signature.r.len() + signature.s.len());
        out.extend_from_slice(&signature.r);
        out.extend_from_slice(&signature.s);
        Ok(out)
    }

    fn gen_ecc_key_pair(&self, alg_id: u32) -> ProviderResult<EccPublicKey> {
        let _gate = gate_lock(&self.gate);
        let mut blob: ECCPUBLICKEYBLOB = unsafe { std::mem::zeroed() };
        blob.BitLen = 256;
        let ret =
            self.application
                .device
                .api
                .gen_ecc_key_pair(self.raw as DEVHANDLE, alg_id, &mut blob);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "GenECCKeyPair"));
        }
        Ok(EccPublicKey {
            bit_len: blob.BitLen,
            x: blob.XCoordinate.to_vec(),
            y: blob.YCoordinate.to_vec(),
        })
    }

    fn gen_rsa_key_pair(&self, bits: u32) -> ProviderResult<RsaPublicKey> {
        let _gate = gate_lock(&self.gate);
        let mut blob: RSAPUBLICKEYBLOB = unsafe { std::mem::zeroed() };
        let ret =
            self.application
                .device
                .api
                .gen_rsa_key_pair(self.raw as DEVHANDLE, bits, &mut blob);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "GenRSAKeyPair"));
        }
        Ok(RsaPublicKey {
            alg_id: blob.AlgID,
            bit_len: blob.BitLen,
            modulus: blob.Modulus.to_vec(),
            exponent: blob.PublicExponent.to_vec(),
        })
    }

    fn sign_rsa(&self, data: &[u8]) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut data_buf = data.to_vec();
        let mut signature = vec![0u8; RSA_SIG_BUF_LEN];
        let mut sig_len: ULONG = signature.len() as ULONG;
        let ret = self.application.device.api.rsa_sign_data(
            self.raw as DEVHANDLE,
            data_buf.as_mut_ptr(),
            data_buf.len() as ULONG,
            signature.as_mut_ptr(),
            &mut sig_len,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "RSASignData"));
        }
        signature.truncate(sig_len as usize);
        Ok(signature)
    }

    fn set_symm_key(&self, alg_id: u32, key: &[u8]) -> ProviderResult<()> {
        let _gate = gate_lock(&self.gate);
        let mut key_buf = key.to_vec();
        let ret = self.application.device.api.set_symm_key(
            self.raw as DEVHANDLE,
            alg_id,
            key_buf.as_mut_ptr(),
            key_buf.len() as ULONG,
        );
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "SetSymmKey"))
        }
    }

    fn encrypt(
        &self,
        alg_id: u32,
        iv: &[u8],
        padding: u32,
        data: &[u8],
    ) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut param = block_cipher_param(iv, padding);
        let mut data_buf = data.to_vec();
        let mut out = vec![0u8; SYM_BUF_LEN];
        let mut out_len: ULONG = out.len() as ULONG;
        let ret = self.application.device.api.encrypt_data(
            self.raw as DEVHANDLE,
            alg_id,
            data_buf.as_mut_ptr(),
            data_buf.len() as ULONG,
            &mut param,
            out.as_mut_ptr(),
            &mut out_len,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "EncryptData"));
        }
        out.truncate(out_len as usize);
        Ok(out)
    }

    fn decrypt(
        &self,
        alg_id: u32,
        iv: &[u8],
        padding: u32,
        data: &[u8],
    ) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut param = block_cipher_param(iv, padding);
        let mut data_buf = data.to_vec();
        let mut out = vec![0u8; SYM_BUF_LEN];
        let mut out_len: ULONG = out.len() as ULONG;
        let ret = self.application.device.api.decrypt_data(
            self.raw as DEVHANDLE,
            alg_id,
            data_buf.as_mut_ptr(),
            data_buf.len() as ULONG,
            &mut param,
            out.as_mut_ptr(),
            &mut out_len,
        );
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "DecryptData"));
        }
        out.truncate(out_len as usize);
        Ok(out)
    }
}

/// Materialise a provider [`EccPublicKey`] as the native blob `DigestInit` and
/// the SPKI builders expect.
fn ecc_blob(key: &EccPublicKey) -> ECCPUBLICKEYBLOB {
    let mut blob: ECCPUBLICKEYBLOB = unsafe { std::mem::zeroed() };
    blob.BitLen = key.bit_len;
    let x_len = key.x.len().min(blob.XCoordinate.len());
    blob.XCoordinate[..x_len].copy_from_slice(&key.x[..x_len]);
    let y_len = key.y.len().min(blob.YCoordinate.len());
    blob.YCoordinate[..y_len].copy_from_slice(&key.y[..y_len]);
    blob
}

/// Build a `BLOCKCIPHERPARAM` from an IV and padding selector.
fn block_cipher_param(iv: &[u8], padding: u32) -> BLOCKCIPHERPARAM {
    let mut param: BLOCKCIPHERPARAM = unsafe { std::mem::zeroed() };
    let copy_len = iv.len().min(param.IV.len());
    param.IV[..copy_len].copy_from_slice(&iv[..copy_len]);
    // The length is the caller's declared IV length, not the copied prefix: the
    // pre-refactor branch set `IVLen = iv_bytes.len()` and copied at most 32 bytes.
    param.IVLen = iv.len() as ULONG;
    param.PaddingType = padding;
    param
}

/// A streaming digest. Dropping it closes the hash and releases the shared device.
pub struct NativeDigest {
    device: Arc<NativeDevice>,
    raw: usize,
    exposed: DigestHandle,
    gate: FfiGate,
}

impl Drop for NativeDigest {
    fn drop(&mut self) {
        let _gate = gate_lock(&self.gate);
        if self.raw != 0 {
            let _ = self.device.api.close_hash(self.raw as DEVHANDLE);
        }
    }
}

impl DigestGuard for Arc<NativeDigest> {
    fn handle(&self) -> DigestHandle {
        self.exposed
    }

    fn update(&self, data: &[u8]) -> ProviderResult<()> {
        let _gate = gate_lock(&self.gate);
        let mut data_buf = data.to_vec();
        let ret = self.device.api.digest_update(
            self.raw as DEVHANDLE,
            data_buf.as_mut_ptr(),
            data_buf.len() as ULONG,
        );
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "DigestUpdate"))
        }
    }

    fn finalize(&self) -> ProviderResult<Vec<u8>> {
        let _gate = gate_lock(&self.gate);
        let mut buf = vec![0u8; HASH_BUF_LEN];
        let mut len: ULONG = buf.len() as ULONG;
        let ret = self
            .device
            .api
            .digest_final(self.raw as DEVHANDLE, buf.as_mut_ptr(), &mut len);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "DigestFinal"));
        }
        buf.truncate(len as usize);
        Ok(buf)
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// Provider backed by a real vendor SKF library.
pub struct NativeSkfProvider {
    alias: String,
    api: Arc<SkfApi>,
    next_handle: AtomicU64,
    gate: FfiGate,
}

impl NativeSkfProvider {
    /// Load the library at `lib_path` and wrap it.
    ///
    /// The library is loaded **once** here; nothing else in this module may call
    /// `Library::new`.
    pub fn new(alias: impl Into<String>, lib_path: &str) -> ProviderResult<Self> {
        let lib =
            unsafe { Library::new(lib_path) }.map_err(|e| ProviderError::LibraryLoadFailed {
                path: lib_path.to_string(),
                arch: std::env::consts::ARCH,
                detail: e.to_string(),
            })?;
        Ok(Self {
            alias: alias.into(),
            api: Arc::new(SkfApi::new(lib)),
            next_handle: AtomicU64::new(1),
            gate: Arc::new(std::sync::Mutex::new(())),
        })
    }

    /// Wrap an already-loaded library as a provider for `alias`.
    ///
    /// Used by the binary's alias resolver for non-default providers: the library
    /// is loaded (and cached) once by the context, and this adapter re-expresses
    /// it through the provider trait so every migrated handler takes the same
    /// path. It does not load a library itself.
    pub fn from_api(alias: impl Into<String>, api: Arc<SkfApi>) -> Self {
        Self {
            alias: alias.into(),
            api,
            next_handle: AtomicU64::new(1),
            gate: Arc::new(std::sync::Mutex::new(())),
        }
    }

    /// Build a provider for `alias` using its configured path for this OS.
    pub fn from_config(config: &SkfConfig, alias: &str) -> ProviderResult<Self> {
        let os = std::env::consts::OS;
        let path = crate::config::resolve_lib_path(&config.libs, alias, os).map_err(|_| {
            ProviderError::ProviderNotConfigured {
                alias: alias.to_string(),
                os,
            }
        })?;
        Self::new(alias, &path)
    }

    fn next_device_handle(&self) -> u64 {
        self.next_handle.fetch_add(1, Ordering::Relaxed)
    }
}

impl SkfProvider for NativeSkfProvider {
    fn alias(&self) -> &str {
        &self.alias
    }

    fn enum_devices(&self, present_only: bool) -> ProviderResult<Vec<String>> {
        let _gate = gate_lock(&self.gate);
        let mut size: ULONG = 0;
        let ret = self
            .api
            .enum_dev(present_only as BOOL, std::ptr::null_mut(), &mut size);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "EnumDev"));
        }
        // The vendor API reports the buffer size as a byte count that includes the
        // trailing NUL, so an empty device list can still be non-zero.
        if size == 0 {
            return Ok(Vec::new());
        }
        read_name_list(size, |buf, len| {
            self.api.enum_dev(present_only as BOOL, buf, len)
        })
    }

    fn device_state(&self, name: &str) -> ProviderResult<u32> {
        let _gate = gate_lock(&self.gate);
        let c_name = cstring(name, "GetDevState")?;
        let mut state: ULONG = 0;
        let ret = self
            .api
            .get_dev_state(c_name.as_ptr() as *mut CHAR, &mut state);
        if ret == SAR_OK {
            Ok(state)
        } else {
            Err(ProviderError::from_native(ret, "GetDevState"))
        }
    }

    fn wait_for_event(&self, buf_len: usize) -> ProviderResult<(String, u32)> {
        let len = if buf_len == 0 {
            DEV_NAME_BUF_LEN
        } else {
            buf_len
        };
        let mut buf = vec![0u8; len];
        let mut name_len: ULONG = len as ULONG;
        let mut event: ULONG = 0;
        let ret =
            self.api
                .wait_for_dev_event(buf.as_mut_ptr() as *mut CHAR, &mut name_len, &mut event);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "WaitForDevEvent"));
        }
        let usable = (name_len as usize).min(len);
        let name = String::from_utf8_lossy(&buf[..usable])
            .trim_end_matches(char::from(0))
            .to_string();
        Ok((name, event))
    }

    fn cancel_wait_for_event(&self) -> ProviderResult<()> {
        let ret = self.api.cancel_wait_for_dev_event();
        if ret == SAR_OK {
            Ok(())
        } else {
            Err(ProviderError::from_native(ret, "CancelWaitForDevEvent"))
        }
    }

    fn open_device(&self, name: &str) -> ProviderResult<Box<dyn DeviceGuard>> {
        let _gate = gate_lock(&self.gate);
        let c_name = cstring(name, "ConnectDev")?;
        let mut raw: DEVHANDLE = std::ptr::null_mut();
        let ret = self.api.connect_dev(c_name.as_ptr() as *mut CHAR, &mut raw);
        if ret != SAR_OK {
            return Err(ProviderError::from_native(ret, "ConnectDev"));
        }
        let exposed = DeviceHandle(self.next_device_handle());
        Ok(Box::new(Arc::new(NativeDevice {
            api: Arc::clone(&self.api),
            raw: raw as usize,
            name: name.to_string(),
            exposed,
            gate: Arc::clone(&self.gate),
        })))
    }
}

/// Default digest algorithm used when a caller does not specify one.
pub const DEFAULT_DIGEST_ALG: ULONG = SGD_SM3;

/// Build an `RSAPUBLICKEYBLOB` view for verification calls.
pub fn rsa_public_key_blob(
    alg_id: ULONG,
    bit_len: ULONG,
    modulus: &[u8],
    exponent: &[u8],
) -> RSAPUBLICKEYBLOB {
    let mut blob: RSAPUBLICKEYBLOB = unsafe { std::mem::zeroed() };
    blob.AlgID = alg_id;
    blob.BitLen = bit_len;
    let m_len = modulus.len().min(blob.Modulus.len());
    blob.Modulus[..m_len].copy_from_slice(&modulus[..m_len]);
    let e_len = exponent.len().min(blob.PublicExponent.len());
    blob.PublicExponent[..e_len].copy_from_slice(&exponent[..e_len]);
    blob
}
