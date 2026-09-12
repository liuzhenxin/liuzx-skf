#![allow(dead_code, non_camel_case_types, non_snake_case)]

use std::ffi::c_void;

// Retrun Codes
pub type ULONG = u32;
pub const SAR_OK: ULONG = 0x00000000;
pub const SAR_FAIL: ULONG = 0x0A000001;
pub const SAR_UNKNOWNERR: ULONG = 0x0A000002;
pub const SAR_NOTSUPPORTLETPROTECT: ULONG = 0x0A000003;
pub const SAR_LOADLIBREARYFAILED: ULONG = 0x0A000004;
pub const SAR_COULDNOTGETFUNCADDR: ULONG = 0x0A000005;
pub const SAR_ILLEGAL_ARGUMENT: ULONG = 0x0A000006;
pub const SAR_NOTSUPPORTYET: ULONG = 0x0A000007;

// Basic Types
pub type BYTE = u8;
pub type CHAR = i8; // Rust `char` is 4 bytes (Unicode), C `char` is 1 byte
pub type BOOL = u8; // Usually 1 byte in C
pub type UINT = u32;
pub type WORD = u16;
pub type DWORD = u32;
pub type FLAGS = u32;
pub type LPSTR = *mut i8;
pub type LPWSTR = *mut u16; // wide char
pub type HANDLE = *mut c_void;
pub type DEVHANDLE = HANDLE;
pub type HAPPLICATION = HANDLE;
pub type HCONTAINER = HANDLE;

// Send + Sync wrapper for HANDLE types (for use in async contexts)
// SAFETY: SKF handles are only valid while the device/app is open,
// and we ensure they are used within the same process context.
// The actual safety is managed by the SKF library.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct SendHandle(pub HANDLE);

unsafe impl Send for SendHandle {}
unsafe impl Sync for SendHandle {}

// Convert helper
impl From<HANDLE> for SendHandle {
    fn from(h: HANDLE) -> Self {
        Self(h)
    }
}

impl From<SendHandle> for HANDLE {
    fn from(s: SendHandle) -> Self {
        s.0
    }
}

// Structs

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct VERSION {
    pub major: BYTE,
    pub minor: BYTE,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct DEVINFO {
    pub Version: VERSION,
    pub Manufacturer: [CHAR; 64],
    pub Issuer: [CHAR; 64],
    pub Label: [CHAR; 32],
    pub SerialNumber: [CHAR; 32],
    pub HWVersion: VERSION,
    pub FirmwareVersion: VERSION,
    pub AlgSymCap: ULONG,
    pub AlgAsymCap: ULONG,
    pub AlgHashCap: ULONG,
    pub DevAuthAlgId: ULONG,
    pub TotalSpace: ULONG,
    pub FreeSpace: ULONG,
    pub MaxECCBufferSize: ULONG,
    pub MaxBufferSize: ULONG,
    pub Reserved: [BYTE; 64],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ECCPUBLICKEYBLOB {
    pub BitLen: ULONG,
    pub XCoordinate: [BYTE; 64],
    pub YCoordinate: [BYTE; 64],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ECCPRIVATEKEYBLOB {
    pub BitLen: ULONG,
    pub PrivateKey: [BYTE; 64],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ECCCIPHERBLOB {
    pub XCoordinate: [BYTE; 64],
    pub YCoordinate: [BYTE; 64],
    pub Hash: [BYTE; 32],
    pub CipherLen: ULONG,
    pub Cipher: [BYTE; 1], // Variable length, simplified for fixed struct. In practice, usually allocated larger.
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ECCSIGNATUREBLOB {
    pub r: [BYTE; 64],
    pub s: [BYTE; 64],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct BLOCKCIPHERPARAM {
    pub IV: [BYTE; 32],
    pub IVLen: ULONG,
    pub PaddingType: ULONG,
    pub FeedBitLen: ULONG,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ENVELOPEDKEYBLOB {
    pub Version: ULONG,
    pub ulSymmAlgID: ULONG,
    pub ulBits: ULONG,
    pub cbEncryptedPriKey: [BYTE; 64],
    pub PubKey: ECCPUBLICKEYBLOB,
    pub ECCCipherBlob: ECCCIPHERBLOB,
}

pub const MAX_IV_LEN: usize = 32;
pub const MAX_FILE_NAME_LEN: usize = 32;

// Alg IDs (Sample)
pub const SGD_SM1_ECB: ULONG = 0x00000101;
pub const SGD_SM1_CBC: ULONG = 0x00000102;
pub const SGD_SM4_ECB: ULONG = 0x00000401;
pub const SGD_SM4_CBC: ULONG = 0x00000402;
pub const SGD_SM2_1: ULONG = 0x00020100;
pub const SGD_SM3: ULONG = 0x00000001;
pub const SGD_SHA1: ULONG = 0x00000002;
pub const SGD_SHA256: ULONG = 0x00000004;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct RSAPUBLICKEYBLOB {
    pub AlgID: ULONG,
    pub BitLen: ULONG,
    pub Modulus: [BYTE; 256], // max 2048-bit
    pub PublicExponent: [BYTE; 4],
}
