//! `CheckPIN` — verify a user PIN and record a session grant.
//!
//! This is the only handler that authorizes a session. It deliberately keeps no
//! copy of the PIN: a successful verification records a deadline in the session's
//! authorization table (SESS-05), and the guard chain releases the application and
//! device on the way out.

use crate::protocol::params::Params;
use crate::protocol::{Language, RpcResponse};
use crate::session::auth::AuthKey;
use crate::session::SessionState;

use super::{
    load_failed, native_code, normalize_alias, note_device_unavailable, split_cert_key,
    ServerContext,
};

/// `CheckPIN` — params `[certKey, pin]`.
pub struct CheckPIN;

impl CheckPIN {
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
        let pin_str = match params.required_str(1, "Missing PIN param") {
            Ok(v) => v,
            Err(e) => return e,
        };

        let parts = match split_cert_key(cert_key, 3) {
            Some(p) => p,
            None => {
                return RpcResponse::err(
                    -2,
                    "Invalid certKey format, expected at least: provider/device/app".into(),
                    id,
                )
            }
        };
        let prov_name = normalize_alias(ctx, parts[0]);
        let dev_name = parts[1];
        let app_name = parts[2];

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
                    Some(code) => {
                        let extra = if code == 0x0A00_002E {
                            " (Application Not Exists)"
                        } else {
                            ""
                        };
                        RpcResponse::err(
                            code as i32,
                            format!("OpenApplication failed: 0x{:08X}{}", code, extra),
                            id,
                        )
                    }
                    None => load_failed(e.to_string(), id),
                };
            }
        };

        let outcome = match application.verify_pin(pin_str) {
            Ok(outcome) => outcome,
            Err(e) => {
                return match native_code(&e) {
                    Some(code) => {
                        let msg = match lang {
                            Language::CN => format!("PIN验证失败: 0x{:08X}, 剩余次数: 0", code),
                            Language::EN => {
                                format!("VerifyPIN failed: 0x{:08X}, retries left: 0", code)
                            }
                        };
                        RpcResponse::err(code as i32, msg, id)
                    }
                    None => load_failed(e.to_string(), id),
                }
            }
        };

        if outcome.success {
            // Only a deadline is stored, never the PIN.
            state.grant(AuthKey::new(prov_name, dev_name, app_name));
            RpcResponse::ok(serde_json::json!(true), id)
        } else {
            let msg = match lang {
                Language::CN => format!(
                    "PIN验证失败: 0x{:08X}, 剩余次数: {}",
                    outcome.code, outcome.retry_count
                ),
                Language::EN => format!(
                    "VerifyPIN failed: 0x{:08X}, retries left: {}",
                    outcome.code, outcome.retry_count
                ),
            };
            RpcResponse::err(outcome.code as i32, msg, id)
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

    /// A native failure while opening the application must still release the
    /// device that was opened first.
    #[tokio::test]
    async fn check_pin_releases_on_native_error() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        fake.fail_next(Operation::VerifyPin, SkfError::DeviceRemoved);
        let ctx = TestContext::new(fake.clone());
        let mut state = SessionState::new(Duration::from_secs(600));
        let p = params(vec![json!("FAKE/dev-a/app-a"), json!("00000000")]);

        let response = CheckPIN::handle(&ctx, &mut state, &p, &Language::EN);

        assert_ne!(response.error, 0, "an injected failure must not succeed");
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
    }

    /// SESS-05 at the handler boundary: a successful verification records a grant
    /// and nothing else. The structural proof that the grant holds no PIN is
    /// `auth::tests::a_grant_holds_only_a_deadline`; this asserts the handler
    /// actually took that path and released the application and device.
    #[tokio::test]
    async fn check_pin_success_records_a_grant_and_releases() {
        let fake = Arc::new(FakeSkfProvider::new("FAKE"));
        let ctx = TestContext::new(fake.clone());
        let mut state = SessionState::new(Duration::from_secs(600));
        let p = params(vec![json!("FAKE/dev-a/app-a"), json!("00000000")]);

        let response = CheckPIN::handle(&ctx, &mut state, &p, &Language::EN);

        assert_eq!(response.error, 0);
        assert_eq!(state.auth().len(), 1, "exactly one grant must be recorded");
        assert_eq!(
            state.authorize(&AuthKey::new("FAKE", "dev-a", "app-a")),
            Ok(()),
            "the grant must authorize the verified triple"
        );
        assert_eq!(fake.call_count(Operation::CloseDevice), 1);
        assert_eq!(fake.call_count(Operation::CloseApplication), 1);
    }
}
