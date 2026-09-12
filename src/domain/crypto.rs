//! Signing, encryption, and CSR handlers.
//!
//! Migrated from the `SignData`, `RSASignData`, `CreatePKCS10`, `EncryptData`
//! and `DecryptData` branches. Every native resource is a guard, so the error
//! returns that previously hand-closed `h_cont`/`h_app`/`h_dev` now release them
//! structurally. Observable behaviour is unchanged: the same localized messages,
//! the same DER encodings, and the same response shapes.

use base64::Engine as _;

use crate::crypto::{
    build_rsa_spki, build_sm2_spki, build_subject_dn, der_bit_string, der_context_0,
    der_encode_integer, der_oid, der_sequence, der_small_integer, OID_SHA256_WITH_RSA,
    OID_SM3_WITH_SM2,
};
use crate::protocol::params::Params;
use crate::protocol::{Language, RpcResponse};
use crate::session::SessionState;
use crate::skf::types::{SGD_SM2_1, SGD_SM3, SGD_SM4_CBC, ULONG};

use super::{
    authorized, ecc_blob, load_failed, native_code, normalize_alias, note_device_unavailable,
    rsa_blob, split_cert_key, ServerContext,
};

/// `SignData` — params `[certKey, dataBase64]`.
pub struct SignData;

impl SignData {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = params.id();

        let cert_key = match params.required_str(0, "Missing certKey param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let data_b64 = match params.required_str(1, "Missing data param") {
            Ok(v) => v,
            Err(e) => return e,
        };

        let parts = match split_cert_key(cert_key, 4) {
            Some(p) => p,
            None => {
                return RpcResponse::err(
                    -2,
                    "Invalid certKey format, expected: provider/device/app/container[/serial]"
                        .into(),
                    id,
                )
            }
        };
        let prov_name = normalize_alias(ctx, parts[0]);
        let (dev_name, app_name, cont_name) = (parts[1], parts[2], parts[3]);

        let data_bytes = match params.decode_base64(data_b64, "Invalid base64 data") {
            Ok(b) => b,
            Err(e) => return e,
        };

        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                // The device is gone (or unusable): this session's grants for it
                // must not survive. Previously only the open_application failure
                // cleared them, so a removal was missed when ConnectDev failed
                // first (found by the B2 device-removal UAT).
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenApplication failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        if authorized(state, prov_name, dev_name, app_name).is_err() {
            return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
        }

        let container = match application.open_container(cont_name) {
            Ok(c) => c,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenContainer failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        // The pre-refactor code fell back to RSA (type 1) when the type query
        // failed, so a type-lookup error is not fatal.
        let cont_type = container.container_type().unwrap_or(1);

        let outcome = if cont_type == 2 {
            container.sign_ecc(&data_bytes).map(|sig| {
                // The 64-byte coordinates are right-aligned; keep them whole, as
                // the branch did.
                let r_der = der_encode_integer(&sig[0..64]);
                let s_der = der_encode_integer(&sig[64..128]);
                let seq_len = r_der.len() + s_der.len();
                let mut der = Vec::with_capacity(2 + seq_len);
                der.push(0x30);
                if seq_len < 128 {
                    der.push(seq_len as u8);
                } else {
                    der.push(0x81);
                    der.push(seq_len as u8);
                }
                der.extend_from_slice(&r_der);
                der.extend_from_slice(&s_der);
                base64::engine::general_purpose::STANDARD.encode(&der)
            })
        } else {
            container
                .sign_rsa(&data_bytes)
                .map(|sig| base64::engine::general_purpose::STANDARD.encode(&sig))
        };

        match outcome {
            Ok(sig_b64) => RpcResponse::ok(serde_json::json!(sig_b64), id),
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("签名失败: 0x{:08X}", code),
                        Language::EN => format!("SignData failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `RSASignData` — params `[certKey, dataBase64]`.
pub struct RSASignData;

impl RSASignData {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = params.id();

        let cert_key = match params.required_str(0, "Missing certKey param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let data_b64 = match params.required_str(1, "Missing data param") {
            Ok(v) => v,
            Err(e) => return e,
        };

        let parts = match split_cert_key(cert_key, 4) {
            Some(p) => p,
            None => {
                return RpcResponse::err(
                    -2,
                    "Invalid certKey format, expected: provider/device/app/container[/serial]"
                        .into(),
                    id,
                )
            }
        };
        let prov_name = normalize_alias(ctx, parts[0]);
        let (dev_name, app_name, cont_name) = (parts[1], parts[2], parts[3]);

        let data_bytes = match params.decode_base64(data_b64, "Invalid base64 data") {
            Ok(b) => b,
            Err(e) => return e,
        };

        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                // The device is gone (or unusable): this session's grants for it
                // must not survive. Previously only the open_application failure
                // cleared them, so a removal was missed when ConnectDev failed
                // first (found by the B2 device-removal UAT).
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenApplication failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        if authorized(state, prov_name, dev_name, app_name).is_err() {
            return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
        }

        let container = match application.open_container(cont_name) {
            Ok(c) => c,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenContainer failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        match container.sign_rsa(&data_bytes) {
            Ok(sig) => {
                let sig_b64 = base64::engine::general_purpose::STANDARD.encode(&sig);
                RpcResponse::ok(serde_json::json!(sig_b64), id)
            }
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("RSA签名失败: 0x{:08X}", code),
                        Language::EN => format!("RSASignData failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `EncryptData` — params `[certKey, data, iv, paddingType, symKey?]`.
pub struct EncryptData;

impl EncryptData {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = params.id();

        let cert_key = match params.required_str(0, "Missing certKey param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let data_b64 = match params.required_str(1, "Missing data param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let iv_b64 = match params.required_str(2, "Missing IV param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let padding_type = params.optional_u64(3, 1) as ULONG;
        // Distinguish an absent key from a present-but-empty one: the pre-refactor
        // code decoded `""` to a zero-byte key and rejected it as the wrong length.
        let sym_key_bytes = match params.raw(4).and_then(|v| v.as_str()) {
            Some(encoded) => match params.decode_base64(encoded, "Invalid base64 symKey") {
                Ok(b) => Some(b),
                Err(e) => return e,
            },
            None => None,
        };

        let parts = match split_cert_key(cert_key, 4) {
            Some(p) => p,
            None => {
                return RpcResponse::err(
                    -2,
                    "Invalid certKey format, expected: provider/device/app/container[/serial]"
                        .into(),
                    id,
                )
            }
        };
        let prov_name = normalize_alias(ctx, parts[0]);
        let (dev_name, app_name, cont_name) = (parts[1], parts[2], parts[3]);

        let data_bytes = match params.decode_base64(data_b64, "Invalid base64 data") {
            Ok(b) => b,
            Err(e) => return e,
        };
        let iv_bytes = match params.decode_base64(iv_b64, "Invalid base64 IV") {
            Ok(b) => b,
            Err(e) => return e,
        };
        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                // The device is gone (or unusable): this session's grants for it
                // must not survive. Previously only the open_application failure
                // cleared them, so a removal was missed when ConnectDev failed
                // first (found by the B2 device-removal UAT).
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenApplication failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        if authorized(state, prov_name, dev_name, app_name).is_err() {
            return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
        }

        let container = match application.open_container(cont_name) {
            Ok(c) => c,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenContainer failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        if let Some(ref key_bytes) = sym_key_bytes {
            if key_bytes.len() != 16 {
                return RpcResponse::err(-2, "SM4 key must be 16 bytes".into(), id);
            }
            if let Err(e) = container.set_symm_key(SGD_SM4_CBC, key_bytes) {
                return match native_code(&e) {
                    Some(code) => {
                        let msg = match lang {
                            Language::CN => format!("设置对称密钥失败: 0x{:08X}", code),
                            Language::EN => format!("SetSymmKey failed: 0x{:08X}", code),
                        };
                        RpcResponse::err(code as i32, msg, id)
                    }
                    None => load_failed(e.to_string(), id),
                };
            }
        }

        match container.encrypt(SGD_SM4_CBC, &iv_bytes, padding_type, &data_bytes) {
            Ok(encrypted) => {
                let encrypted_b64 = base64::engine::general_purpose::STANDARD.encode(&encrypted);
                RpcResponse::ok(
                    serde_json::json!({
                        "encryptedData": encrypted_b64,
                        "algorithm": "SM4-CBC"
                    }),
                    id,
                )
            }
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("EncryptData失败: 0x{:08X}", code),
                        Language::EN => format!("EncryptData failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `DecryptData` — params `[certKey, encryptedData, iv, paddingType, symKey?]`.
pub struct DecryptData;

impl DecryptData {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = params.id();

        let cert_key = match params.required_str(0, "Missing certKey param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let encrypted_b64 = match params.required_str(1, "Missing encryptedData param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let iv_b64 = match params.required_str(2, "Missing IV param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let padding_type = params.optional_u64(3, 1) as ULONG;
        // Distinguish an absent key from a present-but-empty one: the pre-refactor
        // code decoded `""` to a zero-byte key and rejected it as the wrong length.
        let sym_key_bytes = match params.raw(4).and_then(|v| v.as_str()) {
            Some(encoded) => match params.decode_base64(encoded, "Invalid base64 symKey") {
                Ok(b) => Some(b),
                Err(e) => return e,
            },
            None => None,
        };

        let parts = match split_cert_key(cert_key, 4) {
            Some(p) => p,
            None => {
                return RpcResponse::err(
                    -2,
                    "Invalid certKey format, expected: provider/device/app/container[/serial]"
                        .into(),
                    id,
                )
            }
        };
        let prov_name = normalize_alias(ctx, parts[0]);
        let (dev_name, app_name, cont_name) = (parts[1], parts[2], parts[3]);

        let encrypted_bytes =
            match params.decode_base64(encrypted_b64, "Invalid base64 encrypted data") {
                Ok(b) => b,
                Err(e) => return e,
            };
        let iv_bytes = match params.decode_base64(iv_b64, "Invalid base64 IV") {
            Ok(b) => b,
            Err(e) => return e,
        };
        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                // The device is gone (or unusable): this session's grants for it
                // must not survive. Previously only the open_application failure
                // cleared them, so a removal was missed when ConnectDev failed
                // first (found by the B2 device-removal UAT).
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenApplication failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        if authorized(state, prov_name, dev_name, app_name).is_err() {
            return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
        }

        let container = match application.open_container(cont_name) {
            Ok(c) => c,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("OpenContainer failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        if let Some(ref key_bytes) = sym_key_bytes {
            if key_bytes.len() != 16 {
                return RpcResponse::err(-2, "SM4 key must be 16 bytes".into(), id);
            }
            if let Err(e) = container.set_symm_key(SGD_SM4_CBC, key_bytes) {
                return match native_code(&e) {
                    Some(code) => {
                        let msg = match lang {
                            Language::CN => format!("设置对称密钥失败: 0x{:08X}", code),
                            Language::EN => format!("SetSymmKey failed: 0x{:08X}", code),
                        };
                        RpcResponse::err(code as i32, msg, id)
                    }
                    None => load_failed(e.to_string(), id),
                };
            }
        }

        match container.decrypt(SGD_SM4_CBC, &iv_bytes, padding_type, &encrypted_bytes) {
            Ok(decrypted) => {
                let decrypted_b64 = base64::engine::general_purpose::STANDARD.encode(&decrypted);
                RpcResponse::ok(
                    serde_json::json!({
                        "data": decrypted_b64,
                        "algorithm": "SM4-CBC"
                    }),
                    id,
                )
            }
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("DecryptData失败: 0x{:08X}", code),
                        Language::EN => format!("DecryptData failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `CreatePKCS10` — params `[provider, device, app, subject, keyType, keyLength, container]`.
pub struct CreatePKCS10;

impl CreatePKCS10 {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        _lang: &Language,
    ) -> RpcResponse {
        let id = params.id();

        let requested = params.optional_str(0, "");
        let prov_name = normalize_alias(ctx, requested);
        let dev_name = match params.required_str(1, "Missing deviceName") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let app_name = match params.required_str(2, "Missing appName") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let subject = match params.required_str(3, "Missing subject") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let key_type = params.optional_str(4, "SM2").to_uppercase();
        let key_length: u32 = params.optional_u64(5, 256) as u32;
        let container_name_param = params.optional_str(6, "");

        let is_ecc = key_type == "SM2" || key_type == "ECC";

        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                // The device is gone (or unusable): this session's grants for it
                // must not survive. Previously only the open_application failure
                // cleared them, so a removal was missed when ConnectDev failed
                // first (found by the B2 device-removal UAT).
                note_device_unavailable(state, prov_name, dev_name);
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        // Note the historic wording: this branch answered `OpenApp failed`, not
        // `OpenApplication failed`, and did not clear device grants. Both are
        // preserved.
        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => {
                        RpcResponse::err(code as i32, format!("OpenApp failed: 0x{:08X}", code), id)
                    }
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        if authorized(state, prov_name, dev_name, app_name).is_err() {
            return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
        }

        // Determine the container name. The fallback sequence (random then
        // timestamp) is unchanged.
        let cont_name = if !container_name_param.is_empty() {
            container_name_param.to_string()
        } else {
            match device.random(8) {
                Ok(rand_bytes) => format!(
                    "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
                    rand_bytes[0],
                    rand_bytes[1],
                    rand_bytes[2],
                    rand_bytes[3],
                    rand_bytes[4],
                    rand_bytes[5],
                    rand_bytes[6],
                    rand_bytes[7]
                ),
                Err(_) => format!(
                    "CSR{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_micros()
                        % 100000000
                ),
            }
        };

        // Create the container; if that fails and a name was supplied, fall back
        // to opening an existing one.
        let container = match application.create_container(&cont_name) {
            Ok(c) => c,
            Err(_) if !container_name_param.is_empty() => {
                match application.open_container(&cont_name) {
                    Ok(c) => c,
                    Err(e) => {
                        return match native_code(&e) {
                            Some(code) => RpcResponse::err(
                                code as i32,
                                format!("Create/OpenContainer failed: 0x{:08X}", code),
                                id,
                            ),
                            None => load_failed(e.to_string(), id),
                        }
                    }
                }
            }
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("Create/OpenContainer failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        // Generate the key pair and the SPKI that goes into the CSR.
        let (spki, sig_alg_oid, hash_alg_id, ecc_key) = if is_ecc {
            match container.gen_ecc_key_pair(SGD_SM2_1) {
                Ok(key) => (
                    build_sm2_spki(&ecc_blob(&key)),
                    OID_SM3_WITH_SM2,
                    SGD_SM3,
                    Some(key),
                ),
                Err(e) => {
                    return match native_code(&e) {
                        Some(code) => RpcResponse::err(
                            code as i32,
                            format!("GenECCKeyPair failed: 0x{:08X}", code),
                            id,
                        ),
                        None => load_failed(e.to_string(), id),
                    }
                }
            }
        } else {
            match container.gen_rsa_key_pair(key_length) {
                Ok(key) => (
                    build_rsa_spki(&rsa_blob(&key)),
                    OID_SHA256_WITH_RSA,
                    0x00000004_u32,
                    None,
                ),
                Err(e) => {
                    return match native_code(&e) {
                        Some(code) => RpcResponse::err(
                            code as i32,
                            format!("GenRSAKeyPair failed: 0x{:08X}", code),
                            id,
                        ),
                        None => load_failed(e.to_string(), id),
                    }
                }
            }
        };

        // Build TBSCertificationRequestInfo.
        let version = der_small_integer(0);
        let subject_dn = build_subject_dn(subject);
        let attributes = der_context_0(&[]);
        let tbs = der_sequence(&[&version, &subject_dn, &spki, &attributes]);

        // Hash the TBS on the token. SM2 needs the public key mixed into Z, so it
        // uses the keyed digest start.
        let digest = if let Some(ref key) = ecc_key {
            device.begin_digest_with_key(hash_alg_id, b"1234567812345678", key)
        } else {
            device.begin_digest(hash_alg_id, &[])
        };
        let digest = match digest {
            Ok(d) => d,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("DigestInit failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
            }
        };
        if let Err(e) = digest.update(&tbs) {
            return match native_code(&e) {
                Some(code) => {
                    RpcResponse::err(code as i32, format!("Digest failed: 0x{:08X}", code), id)
                }
                None => load_failed(e.to_string(), id),
            };
        }
        let hash_buf = match digest.finalize() {
            Ok(h) => h,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => {
                        RpcResponse::err(code as i32, format!("Digest failed: 0x{:08X}", code), id)
                    }
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        // Sign the digest.
        let sig_bits = if is_ecc {
            match container.sign_ecc(&hash_buf) {
                Ok(sig) => {
                    // SM2 values are right-aligned in the 64-byte arrays.
                    let r_der = der_encode_integer(&sig[32..64]);
                    let s_der = der_encode_integer(&sig[96..128]);
                    der_sequence(&[&r_der, &s_der])
                }
                Err(e) => {
                    return match native_code(&e) {
                        Some(code) => RpcResponse::err(
                            code as i32,
                            format!("ECCSignData failed: 0x{:08X}", code),
                            id,
                        ),
                        None => load_failed(e.to_string(), id),
                    }
                }
            }
        } else {
            // SKF_RSA_SignData performs the private-key operation but does not add
            // the hash AlgorithmIdentifier, so SHA256withRSA signs the DER
            // DigestInfo rather than the bare digest.
            const SHA256_DIGEST_INFO_PREFIX: [u8; 19] = [
                0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02,
                0x01, 0x05, 0x00, 0x04, 0x20,
            ];
            let mut digest_info =
                Vec::with_capacity(SHA256_DIGEST_INFO_PREFIX.len() + hash_buf.len());
            digest_info.extend_from_slice(&SHA256_DIGEST_INFO_PREFIX);
            digest_info.extend_from_slice(&hash_buf);
            match container.sign_rsa(&digest_info) {
                Ok(sig) => sig,
                Err(e) => {
                    return match native_code(&e) {
                        Some(code) => RpcResponse::err(
                            code as i32,
                            format!("RSASignData failed: 0x{:08X}", code),
                            id,
                        ),
                        None => load_failed(e.to_string(), id),
                    }
                }
            }
        };

        // Assemble CertificationRequest: SEQUENCE { tbs, sigAlg, BIT STRING sig }.
        let sig_alg = if is_ecc {
            der_sequence(&[&der_oid(sig_alg_oid)])
        } else {
            der_sequence(&[&der_oid(sig_alg_oid), &[0x05, 0x00]])
        };
        let sig_bit_string = der_bit_string(&sig_bits);
        let csr_der = der_sequence(&[&tbs, &sig_alg, &sig_bit_string]);

        // PEM encoding, 64 columns.
        let csr_b64 = base64::engine::general_purpose::STANDARD.encode(&csr_der);
        let mut pem = String::from("-----BEGIN CERTIFICATE REQUEST-----\n");
        for (i, ch) in csr_b64.chars().enumerate() {
            pem.push(ch);
            if (i + 1) % 64 == 0 {
                pem.push('\n');
            }
        }
        if !pem.ends_with('\n') {
            pem.push('\n');
        }
        pem.push_str("-----END CERTIFICATE REQUEST-----");
        let last_csr_path = std::env::temp_dir()
            .join("last_generated.csr")
            .to_string_lossy()
            .into_owned();
        if let Err(e) = std::fs::write(&last_csr_path, pem.as_bytes()) {
            log::warn!("Failed to write debug CSR to {}: {}", last_csr_path, e);
        }

        RpcResponse::ok(
            serde_json::json!({
                "pem": pem,
                "container": cont_name,
                "keyType": key_type,
                "keyLength": key_length
            }),
            id,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use serde_json::Value;

    use crate::domain::test_support::TestContext;
    use crate::provider::fake::FakeSkfProvider;
    use crate::provider::{Operation, SkfError};
    use crate::session::auth::{AuthKey, AuthRejection};
    use crate::session::SessionState;

    use super::*;

    fn params(values: Vec<Value>) -> Params<'static> {
        let leaked: &'static [Value] = Box::leak(values.into_boxed_slice());
        Params::new(leaked, Some(json!(1)))
    }

    fn authorized_state() -> SessionState {
        let mut state = SessionState::new(Duration::from_secs(600));
        state.grant(AuthKey::new("FAKE", "dev-a", "app-a"));
        state
    }

    fn assert_every_resource_released(fake: &FakeSkfProvider) {
        assert_eq!(fake.call_count(Operation::CloseDevice), 1, "device");
        assert_eq!(
            fake.call_count(Operation::CloseApplication),
            1,
            "application"
        );
        assert_eq!(fake.call_count(Operation::CloseContainer), 1, "container");
    }

    /// Regression for the B2 device-removal finding: when `ConnectDev` fails
    /// because the device is gone, this session's grants for it must be cleared,
    /// so a re-insert still requires CheckPIN (SESS-04b). Previously only the
    /// `open_application` failure cleared them.
    #[tokio::test]
    async fn sign_data_open_device_failure_clears_the_grant() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        fake.fail_next(Operation::OpenDevice, SkfError::DeviceRemoved);

        let p = params(vec![json!("FAKE/dev-a/app-a/cnt-a"), json!("aGVsbG8=")]);
        let response = SignData::handle(&ctx, &mut state, &p, &Language::EN);

        assert_ne!(response.error, 0, "the operation must fail");
        assert_eq!(
            state.authorize(&AuthKey::new("FAKE", "dev-a", "app-a")),
            Err(AuthRejection::NotAuthorized),
            "a ConnectDev failure must clear the device's grants"
        );
    }

    /// The fake reports container type 1 (RSA), so `SignData` takes the RSA path.
    #[tokio::test]
    async fn sign_data_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::SignRsa, SkfError::Custom(0x0A00_00F1));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![json!("FAKE/dev-a/app-a/cnt-a"), json!("aGVsbG8=")]);

        let response = SignData::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00F1);
        assert_every_resource_released(&fake);
    }

    #[tokio::test]
    async fn rsa_sign_data_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::SignRsa, SkfError::Custom(0x0A00_00F2));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![json!("FAKE/dev-a/app-a/cnt-a"), json!("aGVsbG8=")]);

        let response = RSASignData::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00F2);
        assert_every_resource_released(&fake);
    }

    #[tokio::test]
    async fn encrypt_data_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::EncryptData, SkfError::Custom(0x0A00_00F3));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![
            json!("FAKE/dev-a/app-a/cnt-a"),
            json!("aGVsbG8="),
            json!("AAAAAAAAAAAAAAAAAAAAAA=="),
            json!(1),
        ]);

        let response = EncryptData::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00F3);
        assert_every_resource_released(&fake);
    }

    #[tokio::test]
    async fn decrypt_data_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::DecryptData, SkfError::Custom(0x0A00_00F4));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![
            json!("FAKE/dev-a/app-a/cnt-a"),
            json!("aGVsbG8="),
            json!("AAAAAAAAAAAAAAAAAAAAAA=="),
            json!(1),
        ]);

        let response = DecryptData::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00F4);
        assert_every_resource_released(&fake);
    }

    /// A key-generation failure is the earliest error after the container opens.
    #[tokio::test]
    async fn create_pkcs10_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::GenEccKeyPair, SkfError::Custom(0x0A00_00F5));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![
            json!("default"),
            json!("dev-a"),
            json!("app-a"),
            json!("CN=Test"),
            json!("SM2"),
            json!(256),
            json!("cnt-a"),
        ]);

        let response = CreatePKCS10::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00F5);
        assert_every_resource_released(&fake);
    }
}
