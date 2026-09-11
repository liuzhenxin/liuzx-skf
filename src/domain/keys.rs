//! Key-pair generation handlers.
//!
//! Migrated from the `GenECCKeyPair` and `GenRSAKeyPair` branches. Both
//! previously loaded the vendor library per request; they now resolve a provider
//! and hold the container as a guard, which removes the remaining per-call
//! library load (research finding V-2).

use base64::Engine as _;

use crate::protocol::params::Params;
use crate::protocol::{Language, RpcResponse};
use crate::session::SessionState;
use crate::skf::types::{SGD_SM2_1, ULONG};

use super::{
    authorized, ecc_blob, load_failed, load_failed_localized, native_code,
    note_device_unavailable, rsa_blob, struct_bytes, ServerContext,
};

/// `GenECCKeyPair` — params `[provider, device, app, container, algId]`.
pub struct GenECCKeyPair;

impl GenECCKeyPair {
    pub async fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = params.id();

        // The alias is required here, and is not folded into the default; that is
        // what the branch did before the migration.
        let provider_name = match params.required_str(0, "Missing providerName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let dev_name = match params.required_str(1, "Missing deviceName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let app_name = match params.required_str(2, "Missing appName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let cont_name = match params.required_str(3, "Missing containerName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let alg_id = params.optional_u64(4, SGD_SM2_1 as u64) as ULONG;

        let provider = match ctx.resolve(provider_name) {
            Ok(p) => p,
            Err(e) => return load_failed_localized(e, lang, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => {
                        RpcResponse::err(code as i32, format!("ConnectDev failed: 0x{:08X}", code), id)
                    }
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                note_device_unavailable(state, provider_name, dev_name);
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

        if authorized(state, provider_name, dev_name, app_name).is_err() {
            return RpcResponse::err(
                -10,
                "User not logged in. Call CheckPIN first.".into(),
                id,
            );
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

        match container.gen_ecc_key_pair(alg_id) {
            Ok(key) => {
                let pub_key_bytes = struct_bytes(&ecc_blob(&key));
                let pub_key_b64 = base64::engine::general_purpose::STANDARD.encode(&pub_key_bytes);
                RpcResponse::ok(
                    serde_json::json!({
                        "publicKeyBase64": pub_key_b64,
                        "bitLen": key.bit_len,
                    }),
                    id,
                )
            }
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("生成ECC密钥对失败: 0x{:08X}", code),
                        Language::EN => format!("GenECCKeyPair failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `GenRSAKeyPair` — params `[provider, device, app, container, bitsLen]`.
pub struct GenRSAKeyPair;

impl GenRSAKeyPair {
    pub async fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = params.id();

        let provider_name = match params.required_str(0, "Missing providerName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let dev_name = match params.required_str(1, "Missing deviceName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let app_name = match params.required_str(2, "Missing appName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let cont_name = match params.required_str(3, "Missing containerName param") {
            Ok(v) => v,
            Err(e) => return e,
        };
        let bits_len = params.optional_u64(4, 2048) as ULONG;

        let provider = match ctx.resolve(provider_name) {
            Ok(p) => p,
            Err(e) => return load_failed_localized(e, lang, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => {
                        RpcResponse::err(code as i32, format!("ConnectDev failed: 0x{:08X}", code), id)
                    }
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                note_device_unavailable(state, provider_name, dev_name);
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

        if authorized(state, provider_name, dev_name, app_name).is_err() {
            return RpcResponse::err(
                -10,
                "User not logged in. Call CheckPIN first.".into(),
                id,
            );
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

        match container.gen_rsa_key_pair(bits_len) {
            Ok(key) => {
                let pub_key_bytes = struct_bytes(&rsa_blob(&key));
                let pub_key_b64 = base64::engine::general_purpose::STANDARD.encode(&pub_key_bytes);
                RpcResponse::ok(
                    serde_json::json!({
                        "publicKeyBase64": pub_key_b64,
                        "bitLen": key.bit_len,
                    }),
                    id,
                )
            }
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("生成RSA密钥对失败: 0x{:08X}", code),
                        Language::EN => format!("GenRSAKeyPair failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
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
    use crate::session::auth::AuthKey;
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

    #[tokio::test]
    async fn gen_ecc_key_pair_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::GenEccKeyPair, SkfError::Custom(0x0A00_00E1));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![
            json!("FAKE"),
            json!("dev-a"),
            json!("app-a"),
            json!("cnt-a"),
            Value::Null,
        ]);

        let response = GenECCKeyPair::handle(&ctx, &mut state, &p, &Language::EN).await;

        assert_eq!(response.error, 0x0A00_00E1);
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
        assert_eq!(fake.call_count(Operation::CloseContainer), 1);
    }

    #[tokio::test]
    async fn gen_rsa_key_pair_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::GenRsaKeyPair, SkfError::Custom(0x0A00_00E2));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![
            json!("FAKE"),
            json!("dev-a"),
            json!("app-a"),
            json!("cnt-a"),
            json!(2048),
        ]);

        let response = GenRSAKeyPair::handle(&ctx, &mut state, &p, &Language::EN).await;

        assert_eq!(response.error, 0x0A00_00E2);
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
        assert_eq!(fake.call_count(Operation::CloseContainer), 1);
    }
}
