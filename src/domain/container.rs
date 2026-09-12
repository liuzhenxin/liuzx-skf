//! Container and certificate handlers.
//!
//! Migrated from the `CreateContainer`, `DeleteContainer`, `GetContainerType` and
//! `ImportCertificate` branches of the binary dispatcher. The observable contract
//! — error codes, localized messages, response shapes — is reproduced verbatim;
//! the only intended change is that each native resource is now a guard, so every
//! error return releases what it acquired (RES-04).
//!
//! Four handlers live in this file, so each is a unit struct with its own
//! `handle`. That keeps a single request type from shadowing another and keeps the
//! dispatch site in `main.rs` explicit.

use base64::Engine as _;

use crate::protocol::params::Params;
use crate::protocol::{Language, RpcResponse};
use crate::session::SessionState;

use super::{
    authorized, load_failed, native_code, normalize_alias, note_device_unavailable, ServerContext,
};

/// `CreateContainer` — params `[provider, device, app, container]`.
pub struct CreateContainer;

impl CreateContainer {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = request_id(params);

        let requested = params.optional_str(0, "");
        let prov_name = normalize_alias(ctx, requested);
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

        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
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

        // The created container is opened only long enough to confirm creation;
        // dropping the guard closes it, exactly as the pre-refactor code did.
        let outcome = application.create_container(cont_name).map(|_| ());

        match outcome {
            Ok(()) => RpcResponse::ok(serde_json::json!(cont_name), id),
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("创建容器失败: 0x{:08X}", code),
                        Language::EN => format!("CreateContainer failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `DeleteContainer` — params `[provider, device, app, container]`.
///
/// Deliberately has **no** authorization check: that is what the pre-refactor
/// branch did, and removing a container does not touch key material.
pub struct DeleteContainer;

impl DeleteContainer {
    pub fn handle(
        ctx: &dyn ServerContext,
        _state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = request_id(params);

        // Unlike the other container branches, this one did not fold an empty
        // alias into the default; `ctx.resolve("")` fails like `get_api("")` did.
        let prov_name = params.optional_str(0, ctx.default_alias());
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

        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        let application = match device.open_application(app_name) {
            Ok(a) => a,
            Err(e) => {
                note_device_unavailable(_state, prov_name, dev_name);
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

        match application.delete_container(cont_name) {
            Ok(()) => RpcResponse::ok(serde_json::json!(true), id),
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("删除容器失败: 0x{:08X}", code),
                        Language::EN => format!("DeleteContainer failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `GetContainerType` — params `[provider, device, app, container]`.
pub struct GetContainerType;

impl GetContainerType {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = request_id(params);

        let requested = params.optional_str(0, "");
        let prov_name = normalize_alias(ctx, requested);
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

        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
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

        match container.container_type() {
            Ok(cont_type) => {
                let type_str = match cont_type {
                    0x01 => "Sign",
                    0x02 => "Enc",
                    0x03 => "Both",
                    _ => "Unknown",
                };
                RpcResponse::ok(
                    serde_json::json!({
                        "type": cont_type,
                        "typeStr": type_str,
                    }),
                    id,
                )
            }
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("获取容器类型失败: 0x{:08X}", code),
                        Language::EN => format!("GetContainerType failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// `ImportCertificate` — params `[provider, device, app, container, signFlag, cert]`.
pub struct ImportCertificate;

impl ImportCertificate {
    pub fn handle(
        ctx: &dyn ServerContext,
        state: &mut SessionState,
        params: &Params<'_>,
        lang: &Language,
    ) -> RpcResponse {
        let id = request_id(params);

        let prov_name = params.optional_str(0, ctx.default_alias());
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
        let b_sign_flag = match params.raw(4).and_then(|v| v.as_bool()) {
            Some(true) => 1,
            Some(false) => 0,
            None => return RpcResponse::err(-2, "Missing bSignFlag param".into(), id),
        };
        let cert_str = match params.required_str(5, "Missing certData param") {
            Ok(v) => v,
            Err(e) => return e,
        };

        // Guard the encoded blob before any decode. The PEM path strips markers and
        // decodes below this check, so checking the whole string is a safe upper
        // bound and keeps the limit in one place.
        if let Err(e) = params.check_payload_len(cert_str) {
            return e;
        }
        let cert_bytes = match decode_certificate(cert_str) {
            Ok(bytes) => bytes,
            Err(e) => return RpcResponse::err(-3, format!("Invalid cert base64: {}", e), id),
        };
        let cert_bytes = cert_bytes.as_slice();

        let provider = match ctx.resolve(prov_name) {
            Ok(p) => p,
            Err(e) => return load_failed(e, id),
        };

        let device = match provider.open_device(dev_name) {
            Ok(d) => d,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => RpcResponse::err(
                        code as i32,
                        format!("ConnectDev failed: 0x{:08X}", code),
                        id,
                    ),
                    None => load_failed(e.to_string(), id),
                }
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

        match container.import_certificate(b_sign_flag != 0, cert_bytes) {
            Ok(()) => RpcResponse::ok(serde_json::json!(true), id),
            Err(e) => match native_code(&e) {
                Some(code) => {
                    let msg = match lang {
                        Language::CN => format!("导入证书失败: 0x{:08X}", code),
                        Language::EN => format!("ImportCertificate failed: 0x{:08X}", code),
                    };
                    RpcResponse::err(code as i32, msg, id)
                }
                None => load_failed(e.to_string(), id),
            },
        }
    }
}

/// The request id, recovered from the params object.
///
/// [`Params`] owns the id so its rejections stay correlatable; handlers need the
/// same id for their own responses. The public `Params` API does not expose it, so
/// this mirrors the stored `id` via the raw values' companion field through a
/// dedicated accessor added below.
fn request_id(params: &Params<'_>) -> Option<serde_json::Value> {
    params.id()
}

/// Decode a certificate that may be PEM-wrapped or raw base64.
///
/// Reproduces the pre-refactor rule exactly: a PEM body strips the BEGIN/END
/// markers and newlines and, **if that fails to decode, falls back to the raw
/// bytes**; a non-PEM string decodes as base64 and a failure is an error.
fn decode_certificate(cert_str: &str) -> Result<Vec<u8>, base64::DecodeError> {
    if cert_str.contains("-----BEGIN") {
        let b64 = cert_str
            .replace("-----BEGIN CERTIFICATE-----", "")
            .replace("-----END CERTIFICATE-----", "")
            .replace('\n', "")
            .replace('\r', "");
        match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(b) => Ok(b),
            Err(_) => Ok(cert_str.as_bytes().to_vec()),
        }
    } else {
        base64::engine::general_purpose::STANDARD.decode(cert_str)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;
    use serde_json::Value;

    use crate::domain::test_support::TestContext;
    use crate::protocol::Language;
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

    /// RES-04 core evidence: a native failure inside `CreateContainer` still
    /// releases the application and device it opened.
    #[tokio::test]
    async fn create_container_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::CreateContainer, SkfError::Custom(0x0A00_00FF));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        let p = params(vec![
            json!("default"),
            json!("dev-a"),
            json!("app-a"),
            json!("cnt-a"),
        ]);

        let response = CreateContainer::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00FF);
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
    }

    /// `DeleteContainer` has no authorization check; the release guarantee must
    /// hold on its failure path regardless.
    #[tokio::test]
    async fn delete_container_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::DeleteContainer, SkfError::Custom(0x0A00_00FE));
        let ctx = TestContext::new(fake.clone());
        let mut state = SessionState::new(Duration::from_secs(600));
        let p = params(vec![
            json!("default"),
            json!("dev-a"),
            json!("app-a"),
            json!("cnt-a"),
        ]);

        let response = DeleteContainer::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00FE);
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
    }

    /// A failure after the container is open must also close the container.
    #[tokio::test]
    async fn get_container_type_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::ContainerType, SkfError::Custom(0x0A00_00FD));
        let ctx = TestContext::new(fake.clone());
        let mut state = SessionState::new(Duration::from_secs(600));
        let p = params(vec![
            json!("default"),
            json!("dev-a"),
            json!("app-a"),
            json!("cnt-a"),
        ]);

        let response = GetContainerType::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00FD);
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
        assert_eq!(fake.call_count(Operation::CloseContainer), 1);
    }

    /// The authorization path opens three resources; a failed import must release
    /// all three.
    #[tokio::test]
    async fn import_certificate_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::ImportCertificate, SkfError::Custom(0x0A00_00FC));
        let ctx = TestContext::new(fake.clone());
        let mut state = authorized_state();
        // `ImportCertificate` does not fold `"default"` into the configured
        // default (matching the pre-refactor branch), so the alias is literal.
        let p = params(vec![
            json!("FAKE"),
            json!("dev-a"),
            json!("app-a"),
            json!("cnt-a"),
            json!(true),
            json!("aGVsbG8="),
        ]);

        let response = ImportCertificate::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0x0A00_00FC);
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
        assert_eq!(fake.call_count(Operation::CloseContainer), 1);
    }
}
