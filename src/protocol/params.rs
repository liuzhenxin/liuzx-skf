//! Typed access to positional request parameters.
//!
//! # Why this exists
//!
//! Before Phase 2 every branch reached into `req.params` by index and handled the
//! failure inline:
//!
//! ```ignore
//! let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
//!     Some(d) => d,
//!     None => return RpcResponse::err(-2, "Missing deviceName param".into(), id),
//! };
//! ```
//!
//! That pattern appears 71 times across the 19 branches Phase 2 migrates. It is
//! repetitive, easy to get subtly wrong, and — most importantly — the **messages
//! differ per branch**, so a shared helper must not invent its own wording.
//!
//! # Contract preservation
//!
//! Every helper here takes the caller's message and produces the same `-2` code the
//! inline code produced. The fixtures recorded those messages, so a helper that
//! normalised them would show up as contract drift. Callers pass their existing
//! strings verbatim.

use base64::Engine as _;
use serde_json::Value;

use super::RpcResponse;

/// Positional parameter reader for one request.
///
/// Holds the request id so an error response is always correlatable — the reason
/// `RpcResponse::err` takes the id rather than letting a caller patch it in later.
pub struct Params<'a> {
    values: &'a [Value],
    id: Option<Value>,
}

impl<'a> Params<'a> {
    pub fn new(values: &'a [Value], id: Option<Value>) -> Self {
        Self { values, id }
    }

    /// Number of supplied parameters.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The raw value at `index`, if present.
    pub fn raw(&self, index: usize) -> Option<&'a Value> {
        self.values.get(index)
    }

    /// A required string.
    ///
    /// Missing **or wrongly typed** is an error, matching the inline code that used
    /// `and_then(|v| v.as_str())` and fell through to the same message.
    pub fn required_str(&self, index: usize, message: &str) -> Result<&'a str, RpcResponse> {
        self.values
            .get(index)
            .and_then(Value::as_str)
            .ok_or_else(|| self.reject(message))
    }

    /// An optional string, defaulting when absent or wrongly typed.
    pub fn optional_str(&self, index: usize, default: &'a str) -> &'a str {
        self.values
            .get(index)
            .and_then(Value::as_str)
            .unwrap_or(default)
    }

    /// A required unsigned integer.
    pub fn required_u64(&self, index: usize, message: &str) -> Result<u64, RpcResponse> {
        self.values
            .get(index)
            .and_then(Value::as_u64)
            .ok_or_else(|| self.reject(message))
    }

    /// An optional unsigned integer.
    pub fn optional_u64(&self, index: usize, default: u64) -> u64 {
        self.values
            .get(index)
            .and_then(Value::as_u64)
            .unwrap_or(default)
    }

    /// An optional boolean.
    pub fn optional_bool(&self, index: usize, default: bool) -> bool {
        self.values
            .get(index)
            .and_then(Value::as_bool)
            .unwrap_or(default)
    }

    /// A required base64 string, decoded.
    ///
    /// The message is assembled as `"{message}: {error}"` because the inline code
    /// did the same (`format!("Invalid base64 data: {}", e)`), and the fixtures do
    /// not cover that path — keeping the shape means a future fixture can assert it.
    pub fn required_base64(
        &self,
        index: usize,
        message: &str,
    ) -> Result<Vec<u8>, RpcResponse> {
        let encoded = self.required_str(index, message)?;
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|e| self.reject(&format!("{}: {}", message, e)))
    }

    /// An optional base64 string; empty or absent decodes to an empty buffer.
    pub fn optional_base64(&self, index: usize) -> Vec<u8> {
        match self.values.get(index).and_then(Value::as_str) {
            Some(encoded) if !encoded.is_empty() => {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// Build the `-2` rejection the inline code produced.
    fn reject(&self, message: &str) -> RpcResponse {
        RpcResponse::err(-2, message.to_string(), self.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(values: Vec<Value>) -> Params<'static> {
        // Leaked for the test's lifetime; the reader only borrows.
        let leaked: &'static [Value] = Box::leak(values.into_boxed_slice());
        Params::new(leaked, Some(json!(42)))
    }

    #[test]
    fn a_present_string_is_returned() {
        let p = params(vec![json!("GM3000")]);
        assert_eq!(
            p.required_str(0, "Missing providerName param").expect("present"),
            "GM3000"
        );
    }

    /// The recorded contract asserts `-2` and the branch's own wording, so both must
    /// survive the helper.
    #[test]
    fn a_missing_string_rejects_with_minus_two_and_the_callers_message() {
        let p = params(vec![]);
        let response = p.required_str(1, "Missing deviceName param").unwrap_err();
        assert_eq!(response.error, -2);
        assert_eq!(response.message.as_deref(), Some("Missing deviceName param"));
    }

    #[test]
    fn a_wrongly_typed_string_is_rejected_the_same_way() {
        let p = params(vec![json!(7)]);
        let response = p.required_str(0, "Missing param").unwrap_err();
        assert_eq!(response.error, -2);
        assert_eq!(response.message.as_deref(), Some("Missing param"));
    }

    #[test]
    fn rejections_carry_the_request_id() {
        let p = params(vec![]);
        let response = p.required_str(0, "Missing param").unwrap_err();
        assert_eq!(response.id, Some(json!(42)), "an error must stay correlatable");
    }

    #[test]
    fn optional_accessors_fall_back() {
        let p = params(vec![json!("x")]);
        assert_eq!(p.optional_str(5, "default"), "default");
        assert_eq!(p.optional_u64(0, 7), 7, "a string is not a u64");
        assert_eq!(p.optional_bool(0, true), true);
        assert_eq!(p.len(), 1);
        assert!(!p.is_empty());
    }

    #[test]
    fn base64_decodes_and_reports_invalid_input() {
        let p = params(vec![json!("aGVsbG8=")]);
        assert_eq!(p.required_base64(0, "Invalid base64 data").unwrap(), b"hello");

        let bad = params(vec![json!("not base64!!")]);
        let response = bad.required_base64(0, "Invalid base64 data").unwrap_err();
        assert_eq!(response.error, -2);
        assert!(
            response
                .message
                .as_deref()
                .unwrap_or_default()
                .starts_with("Invalid base64 data: "),
            "message shape must match the inline form: {:?}",
            response.message
        );
    }

    #[test]
    fn optional_base64_treats_absent_and_empty_as_no_bytes() {
        let p = params(vec![json!(""), json!("aGk=")]);
        assert!(p.optional_base64(0).is_empty());
        assert_eq!(p.optional_base64(1), b"hi");
        assert!(p.optional_base64(9).is_empty());
    }

    #[test]
    fn integers_are_read_and_missing_ones_reject() {
        let p = params(vec![json!(256)]);
        assert_eq!(p.required_u64(0, "Missing keyLength param").expect("present"), 256);
        let response = p.required_u64(1, "Missing keyLength param").unwrap_err();
        assert_eq!(response.error, -2);
    }
}
