#![allow(dead_code, non_camel_case_types, non_snake_case)]
use super::types::*;
use libloading::{Library, Symbol};
use std::sync::Arc;

// Function Pointer Types
pub type F_SKF_WaitForDevEvent = unsafe extern "system" fn(szDevName: *mut CHAR, pulDevNameLen: *mut ULONG, pulEvent: *mut ULONG) -> ULONG;
pub type F_SKF_CancelWaitForDevEvent = unsafe extern "system" fn() -> ULONG;
pub type F_SKF_EnumDev = unsafe extern "system" fn(bPresent: BOOL, szNameList: *mut CHAR, pulSize: *mut ULONG) -> ULONG;
pub type F_SKF_ConnectDev = unsafe extern "system" fn(szName: *mut CHAR, hDev: *mut DEVHANDLE) -> ULONG;
pub type F_SKF_DisConnectDev = unsafe extern "system" fn(hDev: DEVHANDLE) -> ULONG;
pub type F_SKF_GetDevState = unsafe extern "system" fn(szDevName: *mut CHAR, pulDevState: *mut ULONG) -> ULONG;
pub type F_SKF_SetLabel = unsafe extern "system" fn(hDev: DEVHANDLE, szLabel: *mut CHAR) -> ULONG;
pub type F_SKF_GetDevInfo = unsafe extern "system" fn(hDev: DEVHANDLE, pDevInfo: *mut DEVINFO) -> ULONG;
pub type F_SKF_LockDev = unsafe extern "system" fn(hDev: DEVHANDLE, ulTimeOut: ULONG) -> ULONG;
pub type F_SKF_UnlockDev = unsafe extern "system" fn(hDev: DEVHANDLE) -> ULONG;
pub type F_SKF_Transmit = unsafe extern "system" fn(hDev: DEVHANDLE, pbCommand: *mut BYTE, ulCommandLen: ULONG, pbData: *mut BYTE, pulDataLen: *mut ULONG) -> ULONG;

// App Management
pub type F_SKF_EnumApplication = unsafe extern "system" fn(hDev: DEVHANDLE, szAppName: *mut CHAR, pulSize: *mut ULONG) -> ULONG;
pub type F_SKF_OpenApplication = unsafe extern "system" fn(hDev: DEVHANDLE, szAppName: *mut CHAR, hApplication: *mut HAPPLICATION) -> ULONG;
pub type F_SKF_CloseApplication = unsafe extern "system" fn(hApp: HAPPLICATION) -> ULONG;

// Container Management
pub type F_SKF_EnumContainer = unsafe extern "system" fn(hApp: HAPPLICATION, szContainerName: *mut CHAR, pulSize: *mut ULONG) -> ULONG;
pub type F_SKF_OpenContainer = unsafe extern "system" fn(hApp: HAPPLICATION, szContainerName: *mut CHAR, hContainer: *mut HCONTAINER) -> ULONG;
pub type F_SKF_CloseContainer = unsafe extern "system" fn(hContainer: HCONTAINER) -> ULONG;

// Auth & Crypto
pub type F_SKF_VerifyPIN = unsafe extern "system" fn(hApp: HAPPLICATION, ulPINType: ULONG, szPIN: *mut CHAR, pulRetryCount: *mut ULONG) -> ULONG;
pub type F_SKF_GenRandom = unsafe extern "system" fn(hDev: DEVHANDLE, pbRandom: *mut BYTE, ulRandomLen: ULONG) -> ULONG;
pub type F_SKF_ECCSignData = unsafe extern "system" fn(hContainer: HCONTAINER, pbData: *mut BYTE, ulDataLen: ULONG, pSignature: *mut ECCSIGNATUREBLOB) -> ULONG;
pub type F_SKF_ECCVerify = unsafe extern "system" fn(hDev: DEVHANDLE, pECCPubKeyBlob: *mut ECCPUBLICKEYBLOB, pbData: *mut BYTE, ulDataLen: ULONG, pSignature: *mut ECCSIGNATUREBLOB) -> ULONG;
pub type F_SKF_ExportCertificate = unsafe extern "system" fn(hContainer: HCONTAINER, bSignFlag: BOOL, pbCert: *mut BYTE, pulCertLen: *mut ULONG) -> ULONG;
pub type F_SKF_GetContainerType = unsafe extern "system" fn(hContainer: HCONTAINER, pulContainerType: *mut ULONG) -> ULONG;
pub type F_SKF_RSASignData = unsafe extern "system" fn(hContainer: HCONTAINER, pbData: *mut BYTE, ulDataLen: ULONG, pbSignature: *mut BYTE, pulSignLen: *mut ULONG) -> ULONG;
pub type F_SKF_DigestInit = unsafe extern "system" fn(hDev: DEVHANDLE, ulAlgID: ULONG, pPubKey: *mut ECCPUBLICKEYBLOB, pucID: *mut BYTE, ulIDLen: ULONG, phHash: *mut HANDLE) -> ULONG;
pub type F_SKF_Digest = unsafe extern "system" fn(hHash: HANDLE, pbData: *mut BYTE, ulDataLen: ULONG, pbHashData: *mut BYTE, pulHashLen: *mut ULONG) -> ULONG;
pub type F_SKF_CreateContainer = unsafe extern "system" fn(hApp: HAPPLICATION, szContainerName: *mut CHAR, phContainer: *mut HCONTAINER) -> ULONG;
pub type F_SKF_GenECCKeyPair = unsafe extern "system" fn(hContainer: HCONTAINER, ulAlgId: ULONG, pBlob: *mut ECCPUBLICKEYBLOB) -> ULONG;
pub type F_SKF_GenRSAKeyPair = unsafe extern "system" fn(hContainer: HCONTAINER, ulBitsLen: ULONG, pBlob: *mut RSAPUBLICKEYBLOB) -> ULONG;
pub type F_SKF_DeleteContainer = unsafe extern "system" fn(hApp: HAPPLICATION, szContainerName: *mut CHAR) -> ULONG;
pub type F_SKF_ImportCertificate = unsafe extern "system" fn(hContainer: HCONTAINER, bSignFlag: BOOL, pbCert: *mut BYTE, ulCertLen: ULONG) -> ULONG;
pub type F_SKF_ImportECCKeyPair = unsafe extern "system" fn(hContainer: HCONTAINER, pEnvelopedKeyBlob: *const ENVELOPEDKEYBLOB) -> ULONG;
pub type F_SKF_ImportRSAKeyPair = unsafe extern "system" fn(hContainer: HCONTAINER, ulSymAlgId: ULONG, pbWrappedKey: *mut BYTE, ulWrappedKeyLen: ULONG, pbEncryptedData: *mut BYTE, ulEncryptedDataLen: ULONG) -> ULONG;

// Symmetric Encryption/Decryption
pub type F_SKF_EncryptData = unsafe extern "system" fn(hContainer: HCONTAINER, ulAlgID: ULONG, pbData: *mut BYTE, ulDataLen: ULONG, pBlockCipherParam: *mut BLOCKCIPHERPARAM, pbEncryptedData: *mut BYTE, pulEncryptedDataLen: *mut ULONG) -> ULONG;
pub type F_SKF_DecryptData = unsafe extern "system" fn(hContainer: HCONTAINER, ulAlgID: ULONG, pbEncryptedData: *mut BYTE, ulEncryptedDataLen: ULONG, pBlockCipherParam: *mut BLOCKCIPHERPARAM, pbData: *mut BYTE, pulDataLen: *mut ULONG) -> ULONG;

// P2 - PIN Management
pub type F_SKF_ChangePIN = unsafe extern "system" fn(hApp: HAPPLICATION, ulOldPINType: ULONG, szOldPIN: *mut CHAR, ulNewPINType: ULONG, szNewPIN: *mut CHAR) -> ULONG;
pub type F_SKF_UnblockPIN = unsafe extern "system" fn(hApp: HAPPLICATION, ulPUKType: ULONG, szPUK: *mut CHAR, ulNewPINType: ULONG, szNewPIN: *mut CHAR) -> ULONG;
pub type F_SKF_GetPINInfo = unsafe extern "system" fn(hApp: HAPPLICATION, ulPINType: ULONG, pulMaxLen: *mut ULONG, pulMinLen: *mut ULONG, pulRetryCount: *mut ULONG) -> ULONG;

// P2 - Application Management
pub type F_SKF_CreateApplication = unsafe extern "system" fn(hDev: DEVHANDLE, szAppName: *mut CHAR) -> ULONG;
pub type F_SKF_DeleteApplication = unsafe extern "system" fn(hDev: DEVHANDLE, szAppName: *mut CHAR) -> ULONG;

// P2 - Step-by-step Symmetric Encryption/Decryption
pub type F_SKF_EncryptInit = unsafe extern "system" fn(hContainer: HCONTAINER, ulAlgID: ULONG, pBlockCipherParam: *mut BLOCKCIPHERPARAM, phCipher: *mut HANDLE) -> ULONG;
pub type F_SKF_EncryptUpdate = unsafe extern "system" fn(hCipher: HANDLE, pbData: *mut BYTE, ulDataLen: ULONG, pbEncryptedData: *mut BYTE, pulDataLen: *mut ULONG) -> ULONG;
pub type F_SKF_EncryptFinal = unsafe extern "system" fn(hCipher: HANDLE) -> ULONG;

pub type F_SKF_DecryptInit = unsafe extern "system" fn(hContainer: HCONTAINER, ulAlgID: ULONG, pBlockCipherParam: *mut BLOCKCIPHERPARAM, phCipher: *mut HANDLE) -> ULONG;
pub type F_SKF_DecryptUpdate = unsafe extern "system" fn(hCipher: HANDLE, pbEncryptedData: *mut BYTE, ulDataLen: ULONG, pbData: *mut BYTE, pulDataLen: *mut ULONG) -> ULONG;
pub type F_SKF_DecryptFinal = unsafe extern "system" fn(hCipher: HANDLE) -> ULONG;

// P2 - Step-by-step Hash
pub type F_SKF_DigestUpdate = unsafe extern "system" fn(hHash: HANDLE, pbData: *mut BYTE, ulDataLen: ULONG) -> ULONG;
pub type F_SKF_DigestFinal = unsafe extern "system" fn(hHash: HANDLE, pbHashData: *mut BYTE, pulHashLen: *mut ULONG) -> ULONG;
pub type F_SKF_CloseHash = unsafe extern "system" fn(hHash: HANDLE) -> ULONG;

// P2 - RSA Verify
pub type F_SKF_RSAVerify = unsafe extern "system" fn(hDev: DEVHANDLE, pRSAPubKeyBlob: *mut RSAPUBLICKEYBLOB, pbData: *mut BYTE, ulDataLen: ULONG, pbSignature: *mut BYTE, ulSignLen: ULONG) -> ULONG;

// P2 - Key Management
pub type F_SKF_ExportPublicKey = unsafe extern "system" fn(hContainer: HCONTAINER, ulAlgID: ULONG, pbPublicKey: *mut BYTE, pulPublicKeyLen: *mut ULONG) -> ULONG;
pub type F_SKF_RNGSeed = unsafe extern "system" fn(pbSeed: *mut BYTE, ulSeedLen: ULONG) -> ULONG;
pub type F_SKF_SetSymmKey = unsafe extern "system" fn(hContainer: HCONTAINER, ulAlgID: ULONG, pbSymKey: *mut BYTE, ulSymKeyLen: ULONG) -> ULONG;

// Helper Wrapper
pub struct SkfApi {
    lib: Arc<Library>,
}

impl SkfApi {
    pub fn new(lib: Library) -> Self {
        Self { lib: Arc::new(lib) }
    }

    pub unsafe fn get_func<T>(&self, name: &[u8]) -> Result<Symbol<'_, T>, libloading::Error> {
        self.lib.get(name)
    }

    // Event Management
    pub fn wait_for_dev_event(&self, dev_name: *mut CHAR, dev_name_len: *mut ULONG, event: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_WaitForDevEvent>(b"SKF_WaitForDevEvent") {
                Ok(f) => f(dev_name, dev_name_len, event),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn cancel_wait_for_dev_event(&self) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_CancelWaitForDevEvent>(b"SKF_CancelWaitForDevEvent") {
                Ok(f) => f(),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // Example wrapper methods (optional, but convenient)
    pub fn enum_dev(&self, present: BOOL, name_list: *mut CHAR, size: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_EnumDev>(b"SKF_EnumDev") {
                Ok(f) => f(present, name_list, size),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn connect_dev(&self, name: *mut CHAR, dev_handle: *mut DEVHANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ConnectDev>(b"SKF_ConnectDev") {
                Ok(f) => f(name, dev_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }
    
    pub fn dis_connect_dev(&self, dev_handle: DEVHANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DisConnectDev>(b"SKF_DisConnectDev") {
                Ok(f) => f(dev_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }
    
    pub fn gen_random(&self, dev_handle: DEVHANDLE, random: *mut BYTE, len: ULONG) -> ULONG {
        unsafe {
           match self.get_func::<F_SKF_GenRandom>(b"SKF_GenRandom") {
                Ok(f) => f(dev_handle, random, len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn enum_application(&self, dev_handle: DEVHANDLE, app_name: *mut CHAR, size: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_EnumApplication>(b"SKF_EnumApplication") {
                Ok(f) => f(dev_handle, app_name, size),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn open_application(&self, dev_handle: DEVHANDLE, app_name: *mut CHAR, app_handle: *mut HAPPLICATION) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_OpenApplication>(b"SKF_OpenApplication") {
                Ok(f) => f(dev_handle, app_name, app_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn close_application(&self, app_handle: HAPPLICATION) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_CloseApplication>(b"SKF_CloseApplication") {
                Ok(f) => f(app_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn enum_container(&self, app_handle: HAPPLICATION, container_name: *mut CHAR, size: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_EnumContainer>(b"SKF_EnumContainer") {
                Ok(f) => f(app_handle, container_name, size),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn open_container(&self, app_handle: HAPPLICATION, container_name: *mut CHAR, container_handle: *mut HCONTAINER) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_OpenContainer>(b"SKF_OpenContainer") {
                Ok(f) => f(app_handle, container_name, container_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn close_container(&self, container_handle: HCONTAINER) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_CloseContainer>(b"SKF_CloseContainer") {
                Ok(f) => f(container_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn export_certificate(&self, container_handle: HCONTAINER, sign_flag: BOOL, cert: *mut BYTE, size: *mut ULONG) -> ULONG {
         unsafe {
            match self.get_func::<F_SKF_ExportCertificate>(b"SKF_ExportCertificate") {
                Ok(f) => f(container_handle, sign_flag, cert, size),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn verify_pin(&self, app_handle: HAPPLICATION, pin_type: ULONG, pin: *mut CHAR, retry_count: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_VerifyPIN>(b"SKF_VerifyPIN") {
                Ok(f) => f(app_handle, pin_type, pin, retry_count),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn ecc_sign_data(&self, container_handle: HCONTAINER, data: *mut BYTE, data_len: ULONG, signature: *mut ECCSIGNATUREBLOB) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ECCSignData>(b"SKF_ECCSignData") {
                Ok(f) => f(container_handle, data, data_len, signature),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn get_container_type(&self, container_handle: HCONTAINER, container_type: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_GetContainerType>(b"SKF_GetContainerType") {
                Ok(f) => f(container_handle, container_type),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn rsa_sign_data(&self, container_handle: HCONTAINER, data: *mut BYTE, data_len: ULONG, signature: *mut BYTE, sig_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_RSASignData>(b"SKF_RSASignData") {
                Ok(f) => f(container_handle, data, data_len, signature, sig_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn digest_init(&self, dev_handle: DEVHANDLE, alg_id: ULONG, pub_key: *mut ECCPUBLICKEYBLOB, id: *mut BYTE, id_len: ULONG, hash_handle: *mut HANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DigestInit>(b"SKF_DigestInit") {
                Ok(f) => f(dev_handle, alg_id, pub_key, id, id_len, hash_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn digest(&self, hash_handle: HANDLE, data: *mut BYTE, data_len: ULONG, hash_data: *mut BYTE, hash_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_Digest>(b"SKF_Digest") {
                Ok(f) => f(hash_handle, data, data_len, hash_data, hash_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn create_container(&self, app_handle: HAPPLICATION, name: *mut CHAR, container_handle: *mut HCONTAINER) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_CreateContainer>(b"SKF_CreateContainer") {
                Ok(f) => f(app_handle, name, container_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn gen_ecc_key_pair(&self, container_handle: HCONTAINER, alg_id: ULONG, pub_key: *mut ECCPUBLICKEYBLOB) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_GenECCKeyPair>(b"SKF_GenECCKeyPair") {
                Ok(f) => f(container_handle, alg_id, pub_key),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn gen_rsa_key_pair(&self, container_handle: HCONTAINER, bits_len: ULONG, pub_key: *mut RSAPUBLICKEYBLOB) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_GenRSAKeyPair>(b"SKF_GenRSAKeyPair") {
                Ok(f) => f(container_handle, bits_len, pub_key),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn delete_container(&self, app_handle: HAPPLICATION, name: *mut CHAR) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DeleteContainer>(b"SKF_DeleteContainer") {
                Ok(f) => f(app_handle, name),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn import_certificate(&self, container_handle: HCONTAINER, sign_flag: BOOL, cert: *mut BYTE, cert_len: ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ImportCertificate>(b"SKF_ImportCertificate") {
                Ok(f) => f(container_handle, sign_flag, cert, cert_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn import_ecc_key_pair(&self, container_handle: HCONTAINER, enveloped_key_blob: *const ENVELOPEDKEYBLOB) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ImportECCKeyPair>(b"SKF_ImportECCKeyPair") {
                Ok(f) => f(container_handle, enveloped_key_blob),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn import_rsa_key_pair(&self, container_handle: HCONTAINER, sym_alg_id: ULONG, wrapped_key: *mut BYTE, wrapped_key_len: ULONG, encrypted_data: *mut BYTE, encrypted_data_len: ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ImportRSAKeyPair>(b"SKF_ImportRSAKeyPair") {
                Ok(f) => f(container_handle, sym_alg_id, wrapped_key, wrapped_key_len, encrypted_data, encrypted_data_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn encrypt_data(&self, container_handle: HCONTAINER, alg_id: ULONG, data: *mut BYTE, data_len: ULONG, block_cipher_param: *mut BLOCKCIPHERPARAM, encrypted_data: *mut BYTE, encrypted_data_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_EncryptData>(b"SKF_EncryptData") {
                Ok(f) => f(container_handle, alg_id, data, data_len, block_cipher_param, encrypted_data, encrypted_data_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn decrypt_data(&self, container_handle: HCONTAINER, alg_id: ULONG, encrypted_data: *mut BYTE, encrypted_data_len: ULONG, block_cipher_param: *mut BLOCKCIPHERPARAM, data: *mut BYTE, data_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DecryptData>(b"SKF_DecryptData") {
                Ok(f) => f(container_handle, alg_id, encrypted_data, encrypted_data_len, block_cipher_param, data, data_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // P2 - PIN Management
    pub fn change_pin(&self, app_handle: HAPPLICATION, old_pin_type: ULONG, old_pin: *mut CHAR, new_pin_type: ULONG, new_pin: *mut CHAR) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ChangePIN>(b"SKF_ChangePIN") {
                Ok(f) => f(app_handle, old_pin_type, old_pin, new_pin_type, new_pin),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn unblock_pin(&self, app_handle: HAPPLICATION, puk_type: ULONG, puk: *mut CHAR, new_pin_type: ULONG, new_pin: *mut CHAR) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_UnblockPIN>(b"SKF_UnblockPIN") {
                Ok(f) => f(app_handle, puk_type, puk, new_pin_type, new_pin),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn get_pin_info(&self, app_handle: HAPPLICATION, pin_type: ULONG, max_len: *mut ULONG, min_len: *mut ULONG, retry_count: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_GetPINInfo>(b"SKF_GetPINInfo") {
                Ok(f) => f(app_handle, pin_type, max_len, min_len, retry_count),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // P2 - Application Management
    pub fn create_application(&self, dev_handle: DEVHANDLE, app_name: *mut CHAR) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_CreateApplication>(b"SKF_CreateApplication") {
                Ok(f) => f(dev_handle, app_name),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn delete_application(&self, dev_handle: DEVHANDLE, app_name: *mut CHAR) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DeleteApplication>(b"SKF_DeleteApplication") {
                Ok(f) => f(dev_handle, app_name),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // P2 - Step-by-step Symmetric Encryption/Decryption
    pub fn encrypt_init(&self, container_handle: HCONTAINER, alg_id: ULONG, block_cipher_param: *mut BLOCKCIPHERPARAM, cipher_handle: *mut HANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_EncryptInit>(b"SKF_EncryptInit") {
                Ok(f) => f(container_handle, alg_id, block_cipher_param, cipher_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn encrypt_update(&self, cipher_handle: HANDLE, data: *mut BYTE, data_len: ULONG, encrypted_data: *mut BYTE, encrypted_data_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_EncryptUpdate>(b"SKF_EncryptUpdate") {
                Ok(f) => f(cipher_handle, data, data_len, encrypted_data, encrypted_data_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn encrypt_final(&self, cipher_handle: HANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_EncryptFinal>(b"SKF_EncryptFinal") {
                Ok(f) => f(cipher_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn decrypt_init(&self, container_handle: HCONTAINER, alg_id: ULONG, block_cipher_param: *mut BLOCKCIPHERPARAM, cipher_handle: *mut HANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DecryptInit>(b"SKF_DecryptInit") {
                Ok(f) => f(container_handle, alg_id, block_cipher_param, cipher_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn decrypt_update(&self, cipher_handle: HANDLE, encrypted_data: *mut BYTE, encrypted_data_len: ULONG, data: *mut BYTE, data_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DecryptUpdate>(b"SKF_DecryptUpdate") {
                Ok(f) => f(cipher_handle, encrypted_data, encrypted_data_len, data, data_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn decrypt_final(&self, cipher_handle: HANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DecryptFinal>(b"SKF_DecryptFinal") {
                Ok(f) => f(cipher_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // P2 - Step-by-step Hash
    pub fn digest_update(&self, hash_handle: HANDLE, data: *mut BYTE, data_len: ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DigestUpdate>(b"SKF_DigestUpdate") {
                Ok(f) => f(hash_handle, data, data_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn digest_final(&self, hash_handle: HANDLE, hash_data: *mut BYTE, hash_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_DigestFinal>(b"SKF_DigestFinal") {
                Ok(f) => f(hash_handle, hash_data, hash_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // P2 - RSA Verify
    pub fn rsa_verify(&self, dev_handle: DEVHANDLE, rsa_pub_key_blob: *mut RSAPUBLICKEYBLOB, data: *mut BYTE, data_len: ULONG, signature: *mut BYTE, sig_len: ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_RSAVerify>(b"SKF_RSAVerify") {
                Ok(f) => f(dev_handle, rsa_pub_key_blob, data, data_len, signature, sig_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // P2 - Key Management
    pub fn export_public_key(&self, container_handle: HCONTAINER, alg_id: ULONG, public_key: *mut BYTE, public_key_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ExportPublicKey>(b"SKF_ExportPublicKey") {
                Ok(f) => f(container_handle, alg_id, public_key, public_key_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn rng_seed(&self, seed: *mut BYTE, seed_len: ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_RNGSeed>(b"SKF_RNGSeed") {
                Ok(f) => f(seed, seed_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn set_symm_key(
        &self,
        container_handle: HCONTAINER,
        alg_id: ULONG,
        sym_key: *mut BYTE,
        sym_key_len: ULONG,
    ) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_SetSymmKey>(b"SKF_SetSymmKey") {
                Ok(f) => f(container_handle, alg_id, sym_key, sym_key_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    // Device Management
    pub fn get_dev_info(&self, dev_handle: DEVHANDLE, dev_info: *mut DEVINFO) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_GetDevInfo>(b"SKF_GetDevInfo") {
                Ok(f) => f(dev_handle, dev_info),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn get_dev_state(&self, dev_name: *mut CHAR, dev_state: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_GetDevState>(b"SKF_GetDevState") {
                Ok(f) => f(dev_name, dev_state),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn set_label(&self, dev_handle: DEVHANDLE, label: *mut CHAR) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_SetLabel>(b"SKF_SetLabel") {
                Ok(f) => f(dev_handle, label),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn ecc_verify(&self, dev_handle: DEVHANDLE, ecc_pub_key: *mut ECCPUBLICKEYBLOB, data: *mut BYTE, data_len: ULONG, signature: *mut ECCSIGNATUREBLOB) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_ECCVerify>(b"SKF_ECCVerify") {
                Ok(f) => f(dev_handle, ecc_pub_key, data, data_len, signature),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn lock_dev(&self, dev_handle: DEVHANDLE, timeout: ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_LockDev>(b"SKF_LockDev") {
                Ok(f) => f(dev_handle, timeout),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn unlock_dev(&self, dev_handle: DEVHANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_UnlockDev>(b"SKF_UnlockDev") {
                Ok(f) => f(dev_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn transmit(&self, dev_handle: DEVHANDLE, command: *mut BYTE, command_len: ULONG, response: *mut BYTE, response_len: *mut ULONG) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_Transmit>(b"SKF_Transmit") {
                Ok(f) => f(dev_handle, command, command_len, response, response_len),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }

    pub fn close_hash(&self, hash_handle: HANDLE) -> ULONG {
        unsafe {
            match self.get_func::<F_SKF_CloseHash>(b"SKF_CloseHash") {
                Ok(f) => f(hash_handle),
                Err(_) => SAR_COULDNOTGETFUNCADDR,
            }
        }
    }
}
