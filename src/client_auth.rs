//! Client authentication modes and identities (Phase 8: AUTH-01..AUTH-04).
//!
//! This module is the single source of truth for *what* a client proved and for
//! the small decisions around it: which mode requires which material, whether a
//! mode may be exposed on a non-loopback address, and how a presented bearer
//! token is compared. It deliberately does not touch TLS or the transport — the
//! server module wires these decisions into rustls and the WebSocket upgrade.

/// How the service authenticates a connecting client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientAuthMode {
    /// No client authentication. Only acceptable on a loopback bind (AUTH-03).
    None,
    /// A client certificate verified against a configured CA (AUTH-01).
    Mtls,
    /// A bearer token presented in the WebSocket upgrade request (AUTH-02).
    Token,
}

impl ClientAuthMode {
    /// Parse the configured `tls.client_auth` value.
    ///
    /// Unknown values are an error rather than a silent fallback to `none`: a
    /// typo must not disable authentication.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "mtls" => Ok(Self::Mtls),
            "token" => Ok(Self::Token),
            other => Err(format!(
                "tls: unknown client_auth '{}' (expected none, mtls or token)",
                other
            )),
        }
    }

    /// Whether this mode must not be exposed on a non-loopback address.
    pub fn requires_loopback(self) -> bool {
        matches!(self, Self::None)
    }

    /// A non-sensitive label for logs and diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Mtls => "mtls",
            Self::Token => "token",
        }
    }
}

/// What a connection proved about itself.
///
/// Carried into the session (AUTH-04) so authorization decisions can be
/// audited. It contains only non-secret facts: a token is represented by the
/// `Token` variant, never by its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientIdentity {
    /// No authentication was required or performed.
    Anonymous,
    /// A valid bearer token was presented; the value is intentionally not kept.
    Token,
    /// A client certificate was presented and verified; the CN is kept for audit.
    Certificate { subject_cn: Option<String> },
}

impl ClientIdentity {
    /// A non-sensitive class name for logging.
    pub fn class(&self) -> &'static str {
        match self {
            Self::Anonymous => "anonymous",
            Self::Token => "token",
            Self::Certificate { .. } => "certificate",
        }
    }
}

/// Constant-time byte comparison.
///
/// Used for the bearer token so a timing side channel cannot recover the token
/// prefix. The length is not treated as a secret; everything else is compared
/// without short-circuiting.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

impl ClientIdentity {
    /// Whether the presented `Authorization` header carries the expected token.
    ///
    /// The header must use the `Bearer ` scheme (case-insensitive); any other
    /// form is a non-match. The comparison is constant time.
    pub fn token_matches(expected: &[u8], presented: &str) -> bool {
        let presented = presented.trim();
        let (scheme, value) = match presented.split_once(' ') {
            Some((scheme, value)) => (scheme, value.trim_start()),
            None => return false,
        };
        if !scheme.eq_ignore_ascii_case("Bearer") {
            return false;
        }
        constant_time_eq(expected, value.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_the_three_known_modes_and_normalizes_case() {
        assert_eq!(ClientAuthMode::parse("none").unwrap(), ClientAuthMode::None);
        assert_eq!(ClientAuthMode::parse("MTLS").unwrap(), ClientAuthMode::Mtls);
        assert_eq!(
            ClientAuthMode::parse(" Token ").unwrap(),
            ClientAuthMode::Token
        );
    }

    #[test]
    fn parse_rejects_unknown_modes() {
        assert!(ClientAuthMode::parse("jwt").is_err());
        assert!(ClientAuthMode::parse("").is_err());
        assert!(ClientAuthMode::parse("mtls ").unwrap() == ClientAuthMode::Mtls);
    }

    #[test]
    fn only_none_requires_loopback() {
        assert!(ClientAuthMode::None.requires_loopback());
        assert!(!ClientAuthMode::Mtls.requires_loopback());
        assert!(!ClientAuthMode::Token.requires_loopback());
    }

    #[test]
    fn constant_time_eq_matches_exact_bytes_only() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrez"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
        assert!(!constant_time_eq(b"", b"x"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn token_matches_requires_the_bearer_prefix_and_the_exact_value() {
        let expected = b"secret".as_slice();
        assert!(ClientIdentity::token_matches(expected, "Bearer secret"));
        assert!(ClientIdentity::token_matches(expected, "bearer secret"));
        // Surrounding whitespace is ordinary header OWS and is tolerated.
        assert!(ClientIdentity::token_matches(expected, "  Bearer secret  "));
        assert!(!ClientIdentity::token_matches(expected, "secret"));
        assert!(!ClientIdentity::token_matches(expected, "Bearer secre"));
        assert!(!ClientIdentity::token_matches(expected, "Bearer wrong"));
        assert!(!ClientIdentity::token_matches(expected, "Basic secret"));
        assert!(!ClientIdentity::token_matches(expected, ""));
    }

    #[test]
    fn identity_class_is_non_sensitive() {
        assert_eq!(ClientIdentity::Anonymous.class(), "anonymous");
        assert_eq!(ClientIdentity::Token.class(), "token");
        assert_eq!(
            ClientIdentity::Certificate {
                subject_cn: Some("alice".into())
            }
            .class(),
            "certificate"
        );
    }
}
