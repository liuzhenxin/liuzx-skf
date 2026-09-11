//! Request and response types for the service's JSON-RPC-style protocol.
//!
//! Moved verbatim from `src/main.rs` in Phase 2 (plan 02-04). The wire shape is
//! deliberately unchanged: the v0.2.0 client and 37 recorded fixtures depend on it,
//! including the fact that this is *JSON-RPC-like* rather than strictly compliant.
//!
//! Two things to know before changing anything here:
//!
//! * Responses carry a numeric `error` field that is `0` on success, and the
//!   `result` field is omitted when null. That is the established contract.
//! * `params` is a positional array. Phase 2 added [`params`] to read it with type
//!   checking while preserving each branch's existing error code and message.

pub mod params;

use serde::{Deserialize, Serialize};

/// One incoming request.
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub method: String,
    #[serde(default)]
    pub params: Vec<serde_json::Value>,
    pub id: Option<serde_json::Value>,
}

/// One outgoing response.
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub error: i32,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub result: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<serde_json::Value>,
}

impl RpcResponse {
    pub fn ok(result: serde_json::Value, id: Option<serde_json::Value>) -> Self {
        Self {
            error: 0,
            result,
            message: None,
            id,
        }
    }

    pub fn err(code: i32, msg: String, id: Option<serde_json::Value>) -> Self {
        Self {
            error: code,
            result: serde_json::Value::Null,
            message: Some(msg),
            id,
        }
    }
}

/// Which language failure messages are rendered in.
///
/// The pre-refactor server defaulted every connection to English, and `SetLanguage`
/// switches it for the life of that connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    EN,
    CN,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_request_without_params_deserializes_to_an_empty_list() {
        let request: RpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"EnumProvider","id":1}"#)
                .expect("deserialize");
        assert_eq!(request.method, "EnumProvider");
        assert!(request.params.is_empty());
        assert_eq!(request.id, Some(json!(1)));
    }

    #[test]
    fn a_request_without_an_id_is_accepted() {
        let request: RpcRequest =
            serde_json::from_str(r#"{"method":"SetLanguage","params":["EN"]}"#).expect("deserialize");
        assert_eq!(request.params.len(), 1);
        assert!(request.id.is_none());
    }

    #[test]
    fn ok_responses_carry_error_zero_and_omit_the_message() {
        let response = RpcResponse::ok(json!("OK"), Some(json!(1)));
        assert_eq!(response.error, 0);
        assert!(response.message.is_none());

        let text = serde_json::to_string(&response).expect("serialize");
        assert!(!text.contains("message"), "a success must not carry a message: {}", text);
        assert!(!text.contains("error\":1"));
    }

    #[test]
    fn err_responses_omit_a_null_result() {
        let response = RpcResponse::err(-2, "Missing param".into(), Some(json!(7)));
        let value: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&response).expect("serialize"))
                .expect("parse");

        assert_eq!(value["error"], json!(-2));
        assert_eq!(value["message"], json!("Missing param"));
        assert_eq!(value["id"], json!(7));
        assert!(
            value.get("result").is_none(),
            "a null result must be omitted, not serialized as null"
        );
    }
}
