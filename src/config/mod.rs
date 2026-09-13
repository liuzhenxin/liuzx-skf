//! Provider configuration: loading, validation, and platform path resolution.
//!
//! Extracted from `src/main.rs` during Phase 1. The config path is passed in
//! explicitly so that tests never depend on the process working directory or on
//! `SKF_*` environment variables — those belong to the binary's composition
//! root, not to this module.
//!
//! The lookup and expansion logic below was moved verbatim; error message
//! wording is intentionally preserved because it is observable through the
//! `EnumDevice` / `Load Lib Failed` contract recorded in
//! `tests/fixtures/v0.2.0/`.

use std::collections::HashMap;

use serde::Deserialize;

/// TLS material and the client-authentication mode (Phase 7).
///
/// A **named** field on `SkfConfig` for the same reason as `allow_remote`: an
/// unnamed top-level key would be captured by the flattened `libs` map.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct TlsConfig {
    /// PEM certificate chain path.
    #[serde(default)]
    pub cert_file: Option<String>,
    /// PEM private-key path (a secret; never logged or diagnosed).
    #[serde(default)]
    pub key_file: Option<String>,
    /// Client-authentication mode: `none`, `mtls`, or `token`.
    #[serde(default)]
    pub client_auth: Option<String>,
    /// PEM CA bundle used to verify client certificates (mTLS).
    #[serde(default)]
    pub client_ca_file: Option<String>,
    /// File containing the bearer token (token mode). A secret; never logged.
    #[serde(default)]
    pub token_file: Option<String>,
}

/// Raw provider configuration as read from YAML.
///
/// `vendor` maps `VID:PID` strings to provider aliases. The flattened `libs`
/// map is `provider alias -> { os name -> library path }`.
#[derive(Debug, Deserialize, Clone)]
pub struct SkfConfig {
    pub default: String,
    pub vendor: HashMap<String, String>,
    /// Explicit opt-in for binding the WebSocket listener to a non-loopback
    /// address. `None`/`Some(false)` means loopback-only.
    ///
    /// A **named** field on purpose: an unnamed top-level key would be captured
    /// by the flattened `libs` map and could break parsing (Phase 2 D-07).
    #[serde(default)]
    pub allow_remote: Option<bool>,
    /// Optional TLS configuration. Absent means the listener is plaintext.
    #[serde(default)]
    pub tls: Option<TlsConfig>,
    #[serde(flatten)]
    pub libs: HashMap<String, HashMap<String, String>>,
}

impl SkfConfig {
    /// Environment variable that opts in at runtime.
    pub const REMOTE_ENV: &'static str = "SKF_ALLOW_REMOTE";

    /// Environment override for the TLS certificate path.
    pub const TLS_CERT_ENV: &'static str = "SKF_TLS_CERT";
    /// Environment override for the TLS private-key path.
    pub const TLS_KEY_ENV: &'static str = "SKF_TLS_KEY";
    /// Environment override for the client-authentication mode.
    pub const TLS_CLIENT_AUTH_ENV: &'static str = "SKF_TLS_CLIENT_AUTH";
    /// Environment override for the mTLS client CA bundle path.
    pub const TLS_CLIENT_CA_ENV: &'static str = "SKF_TLS_CLIENT_CA";
    /// Environment variable holding the bearer token value directly.
    pub const TLS_TOKEN_ENV: &'static str = "SKF_TLS_TOKEN";
    /// Environment override for the bearer-token file path.
    pub const TLS_TOKEN_FILE_ENV: &'static str = "SKF_TLS_TOKEN_FILE";

    /// Whether non-loopback binding is permitted.
    ///
    /// YAML `allow_remote: true` or the remote env var set to `1`/`true`
    /// (case-insensitive) both opt in; either is sufficient. Absent/false
    /// everywhere means the service is loopback-only.
    pub fn allows_remote(&self) -> bool {
        if self.allow_remote == Some(true) {
            return true;
        }
        matches!(
            std::env::var(Self::REMOTE_ENV)
                .ok()
                .as_deref()
                .map(|v| v.trim().to_ascii_lowercase()),
            Some(v) if v == "1" || v == "true"
        )
    }

    /// The configured TLS certificate path, with the environment override first.
    pub fn tls_cert_path(&self) -> Option<String> {
        env_non_empty(Self::TLS_CERT_ENV)
            .or_else(|| self.tls.as_ref().and_then(|t| t.cert_file.clone()))
            .filter(|value| !value.trim().is_empty())
    }

    /// The configured TLS private-key path, with the environment override first.
    pub fn tls_key_path(&self) -> Option<String> {
        env_non_empty(Self::TLS_KEY_ENV)
            .or_else(|| self.tls.as_ref().and_then(|t| t.key_file.clone()))
            .filter(|value| !value.trim().is_empty())
    }

    /// The client-authentication mode, lower-cased; defaults to `none`.
    pub fn tls_client_auth(&self) -> String {
        env_non_empty(Self::TLS_CLIENT_AUTH_ENV)
            .or_else(|| self.tls.as_ref().and_then(|t| t.client_auth.clone()))
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "none".to_string())
    }

    /// The configured mTLS client CA path, with the environment override first.
    pub fn tls_client_ca_path(&self) -> Option<String> {
        env_non_empty(Self::TLS_CLIENT_CA_ENV)
            .or_else(|| self.tls.as_ref().and_then(|t| t.client_ca_file.clone()))
            .filter(|value| !value.trim().is_empty())
    }

    /// The configured bearer token, read from the environment or a file.
    ///
    /// This returns a secret. Callers must not log the result. A missing or
    /// unreadable source yields `None`, which `validate_tls` turns into a
    /// configuration error for `token` mode.
    pub fn tls_token(&self) -> Option<String> {
        if let Some(value) = env_non_empty(Self::TLS_TOKEN_ENV) {
            return Some(value);
        }
        let path = env_non_empty(Self::TLS_TOKEN_FILE_ENV)
            .or_else(|| self.tls.as_ref().and_then(|t| t.token_file.clone()))
            .filter(|value| !value.trim().is_empty())?;
        let token = std::fs::read_to_string(path).ok()?;
        let token = token.trim().to_string();
        if token.is_empty() {
            None
        } else {
            Some(token)
        }
    }

    /// Validate the TLS and client-authentication block before any listener is
    /// created.
    ///
    /// Unknown modes are errors, never a silent fallback to `none` (D-02).
    pub fn validate_tls(&self) -> Result<(), String> {
        match (self.tls_cert_path(), self.tls_key_path()) {
            (None, None) | (Some(_), Some(_)) => {}
            _ => return Err("tls: both cert_file and key_file are required".to_string()),
        }
        match self.tls_client_auth().as_str() {
            "none" => {}
            "mtls" => {
                if self.tls_cert_path().is_none() {
                    return Err(
                        "tls: client_auth 'mtls' requires cert_file and key_file".to_string()
                    );
                }
                if self.tls_client_ca_path().is_none() {
                    return Err("tls: client_auth 'mtls' requires client_ca_file".to_string());
                }
            }
            "token" => {
                if self.tls_token().is_none() {
                    return Err(
                        "tls: client_auth 'token' requires SKF_TLS_TOKEN or token_file".to_string(),
                    );
                }
            }
            other => {
                return Err(format!(
                    "tls: unknown client_auth '{}' (expected none, mtls or token)",
                    other
                ));
            }
        }
        Ok(())
    }
}

/// Read an environment variable, treating empty/whitespace as unset.
fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// Read and parse a configuration file.
///
/// The error text is preserved from the pre-refactor implementation so the
/// recorded error contracts still match.
pub fn load(path: &str) -> anyhow::Result<SkfConfig> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("failed to read config '{}': {}", path, e))?;
    let config = serde_yaml::from_str(&text)?;
    Ok(config)
}

/// Expand `%VAR%` references using the process environment.
///
/// Moved verbatim. Two behaviours matter and are covered by tests:
/// an unknown variable is left as `%VAR%`, and an unclosed `%` is emitted
/// literally rather than swallowing the remainder of the string.
pub fn expand_env_vars(path: &str) -> String {
    let mut result = String::new();
    let mut chars = path.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let mut var_name = String::new();
            let mut closed = false;
            while let Some(&n) = chars.peek() {
                if n == '%' {
                    chars.next();
                    closed = true;
                    break;
                }
                var_name.push(chars.next().unwrap());
            }

            if closed && !var_name.is_empty() {
                match std::env::var(&var_name) {
                    Ok(val) => result.push_str(&val),
                    Err(_) => {
                        // If var not found, keep original string
                        result.push('%');
                        result.push_str(&var_name);
                        result.push('%');
                    }
                }
            } else {
                // Not a valid var format, push literal % and what we ate
                result.push('%');
                result.push_str(&var_name);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Resolve the library path for `provider` on `os`, expanding environment
/// variables in the configured value.
///
/// The OS key match is case-insensitive to stay tolerant of hand-edited YAML.
/// The error message is preserved verbatim from the pre-refactor code because
/// callers surface it in RPC responses.
pub fn resolve_lib_path(
    libs: &HashMap<String, HashMap<String, String>>,
    provider: &str,
    os: &str,
) -> anyhow::Result<String> {
    let raw_path = libs
        .get(provider)
        .and_then(|m| {
            m.get(os).or_else(|| {
                m.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(os))
                    .map(|(_, v)| v)
            })
        })
        .ok_or_else(|| {
            let available_providers: Vec<_> = libs.keys().collect();
            anyhow::anyhow!(
                "Provider '{}' not configured for OS '{}'. Avail Provs: {:?}",
                provider,
                os,
                available_providers
            )
        })?;

    Ok(expand_env_vars(raw_path))
}

/// Collect configuration diagnostics.
///
/// Phase 1 only *reports* problems; it deliberately does not change startup
/// behaviour (fail-fast validation is a later phase). An empty result means the
/// configuration looks usable for `os`.
///
/// Only the selected (`default`) provider must resolve for `os`. Other providers
/// are checked when they do resolve, and silently skipped when they do not — a
/// multi-platform config legitimately omits operating systems (the shipped
/// `config/skf.yaml` defines FishMan for Windows only).
pub fn validate(config: &SkfConfig, os: &str) -> Vec<String> {
    let mut problems = Vec::new();

    if !config.libs.contains_key(&config.default) {
        problems.push(format!(
            "default provider '{}' is not defined in the provider map",
            config.default
        ));
    } else {
        match resolve_lib_path(&config.libs, &config.default, os) {
            Ok(path) => {
                if path.trim().is_empty() {
                    problems.push(format!(
                        "default provider '{}' resolved to an empty path",
                        config.default
                    ));
                }
                if path.contains('%') {
                    problems.push(format!(
                        "default provider '{}' has an unresolved environment variable in path '{}'",
                        config.default, path
                    ));
                }
            }
            Err(_) => problems.push(format!(
                "default provider '{}' has no path configured for OS '{}'",
                config.default, os
            )),
        }
    }

    for alias in config.libs.keys() {
        if alias == &config.default {
            continue;
        }
        // Providers without a path for this OS are expected in multi-platform
        // configs and are not reported.
        if let Ok(path) = resolve_lib_path(&config.libs, alias, os) {
            if path.contains('%') {
                problems.push(format!(
                    "provider '{}' has an unresolved environment variable in path '{}'",
                    alias, path
                ));
            }
        }
    }

    for (vpid, alias) in &config.vendor {
        if !config.libs.contains_key(alias) {
            problems.push(format!(
                "vendor mapping '{}' references unknown provider '{}'",
                vpid, alias
            ));
        }
    }

    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_path(relative: &str) -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(relative)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn expand_env_vars_keeps_unknown_variables_literal() {
        assert_eq!(
            expand_env_vars("%SKF_DEFINITELY_UNSET_VAR%"),
            "%SKF_DEFINITELY_UNSET_VAR%"
        );
    }

    #[test]
    fn expand_env_vars_keeps_unclosed_marker_literal() {
        assert_eq!(expand_env_vars("%UNCLOSED"), "%UNCLOSED");
        assert_eq!(expand_env_vars("/a/%UNCLOSED/b"), "/a/%UNCLOSED/b");
    }

    #[test]
    fn expand_env_vars_handles_empty_and_doubled_percent() {
        // Pre-refactor behaviour: `%%` is not a valid marker (the variable name is
        // empty), so only ONE literal `%` is emitted. Asserting the real behaviour
        // here rather than an idealised one is the point — this module was moved
        // verbatim and any behaviour change must be a deliberate, separate change.
        assert_eq!(expand_env_vars("a%%b"), "a%b");
        assert_eq!(expand_env_vars("%%"), "%");
        assert_eq!(expand_env_vars(""), "");
        assert_eq!(expand_env_vars("no markers here"), "no markers here");
    }

    #[test]
    fn expand_env_vars_substitutes_known_variable() {
        // Wrap a variable this test controls so it cannot collide with user env.
        std::env::set_var("SKF_PHASE1_TEST_VAR", "expanded");
        assert_eq!(expand_env_vars("x/%SKF_PHASE1_TEST_VAR%/y"), "x/expanded/y");
        std::env::remove_var("SKF_PHASE1_TEST_VAR");
    }

    #[test]
    fn resolve_lib_path_matches_os_key_case_insensitively() {
        let mut libs = HashMap::new();
        let mut platforms = HashMap::new();
        platforms.insert(
            "Windows".to_string(),
            "native\\GM3000\\windows\\mtoken_gm3000.dll".to_string(),
        );
        libs.insert("GM3000".to_string(), platforms);

        let exact = resolve_lib_path(&libs, "GM3000", "Windows").expect("exact match");
        let lower = resolve_lib_path(&libs, "GM3000", "windows").expect("case-insensitive match");
        assert_eq!(exact, lower);
    }

    #[test]
    fn resolve_lib_path_reports_unknown_provider() {
        let libs: HashMap<String, HashMap<String, String>> = HashMap::new();
        let err = resolve_lib_path(&libs, "NOPE", "macos").expect_err("must fail");
        let text = err.to_string();
        assert!(
            text.contains("Provider 'NOPE' not configured"),
            "error text must stay stable for the recorded contract, got: {}",
            text
        );
    }

    #[test]
    fn shipped_configs_load_and_validate_cleanly() {
        // Each config is validated against the OS it ships for: the Windows
        // package config legitimately defines no macOS or Linux path.
        let cases = [
            ("config/skf.yaml", std::env::consts::OS),
            ("packaging/windows/skf-windows-x86.yaml", "windows"),
        ];

        for (relative, os) in cases {
            let config = load(&repo_path(relative))
                .unwrap_or_else(|e| panic!("{} must parse: {}", relative, e));
            assert_eq!(config.default, "GM3000", "{} default provider", relative);

            let problems = validate(&config, os);
            assert!(
                problems.is_empty(),
                "{} reported diagnostics for {}: {:?}",
                relative,
                os,
                problems
            );
        }
    }

    #[test]
    fn validate_reports_missing_path_for_the_selected_provider() {
        let mut libs = HashMap::new();
        let mut platforms = HashMap::new();
        platforms.insert("windows".to_string(), "native/x.dll".to_string());
        libs.insert("GM3000".to_string(), platforms);

        let config = SkfConfig {
            default: "GM3000".to_string(),
            vendor: HashMap::new(),
            allow_remote: None,
            tls: None,
            libs,
        };

        let problems = validate(&config, "macos");
        assert!(
            problems
                .iter()
                .any(|p| p.contains("no path configured for OS 'macos'")),
            "expected a missing-path diagnostic, got {:?}",
            problems
        );
    }

    #[test]
    fn windows_package_config_keeps_the_bundled_relative_dll_path() {
        let config = load(&repo_path("packaging/windows/skf-windows-x86.yaml"))
            .expect("package config must parse");
        let path = resolve_lib_path(&config.libs, "GM3000", "windows").expect("windows path");
        assert_eq!(
            path, "native\\GM3000\\windows\\mtoken_gm3000.dll",
            "the bundled package must load the DLL from its own directory"
        );
    }

    /// Environment state is process-global, so the env tests share one lock with
    /// any other unit test that reads env-driven configuration.
    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::test_env::lock()
    }

    #[test]
    fn yaml_allow_remote_true_opts_in() {
        let _guard = env_lock();
        std::env::remove_var(SkfConfig::REMOTE_ENV);
        let yaml =
            "default: GM3000\nvendor: {}\nallow_remote: true\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(config.allows_remote());
        assert!(
            config.libs.contains_key("GM3000"),
            "the named field must not swallow the flattened providers"
        );
    }

    #[test]
    fn absent_allow_remote_is_loopback_only() {
        let _guard = env_lock();
        std::env::remove_var(SkfConfig::REMOTE_ENV);
        let config: SkfConfig =
            serde_yaml::from_str("default: GM3000\nvendor: {}\nGM3000:\n  macos: native/x.dylib\n")
                .expect("parse");
        assert!(!config.allows_remote());
    }

    #[test]
    fn allow_remote_false_is_loopback_only() {
        let _guard = env_lock();
        std::env::remove_var(SkfConfig::REMOTE_ENV);
        let config: SkfConfig = serde_yaml::from_str(
            "default: GM3000\nvendor: {}\nallow_remote: false\nGM3000:\n  macos: native/x.dylib\n",
        )
        .expect("parse");
        assert!(!config.allows_remote(), "an explicit false must not opt in");
    }

    #[test]
    fn env_var_opts_in() {
        let _guard = env_lock();
        let config: SkfConfig =
            serde_yaml::from_str("default: GM3000\nvendor: {}\nGM3000:\n  macos: native/x.dylib\n")
                .expect("parse");
        std::env::set_var(SkfConfig::REMOTE_ENV, "TRUE");
        assert!(
            config.allows_remote(),
            "case-insensitive truthy value must opt in"
        );
        std::env::remove_var(SkfConfig::REMOTE_ENV);
        assert!(!config.allows_remote());
    }

    #[test]
    fn load_reports_missing_file_with_stable_message() {
        let err = load(&repo_path("config/does-not-exist.yaml")).expect_err("must fail");
        let text = err.to_string();
        assert!(
            text.starts_with("failed to read config '"),
            "error text must stay stable, got: {}",
            text
        );
    }

    #[test]
    fn validate_flags_unknown_default_and_vendor_alias() {
        let mut libs = HashMap::new();
        let mut platforms = HashMap::new();
        platforms.insert("macos".to_string(), "native/x.dylib".to_string());
        libs.insert("GM3000".to_string(), platforms);

        let mut vendor = HashMap::new();
        vendor.insert("055c:e618".to_string(), "MISSING".to_string());

        let config = SkfConfig {
            default: "ABSENT".to_string(),
            vendor,
            allow_remote: None,
            tls: None,
            libs,
        };

        let problems = validate(&config, "macos");
        assert!(problems
            .iter()
            .any(|p| p.contains("default provider 'ABSENT'")));
        assert!(problems
            .iter()
            .any(|p| p.contains("unknown provider 'MISSING'")));
    }

    #[test]
    fn tls_block_parses_and_does_not_break_providers() {
        let _guard = env_lock();
        clear_tls_env();
        let yaml = "default: GM3000\nvendor: {}\ntls:\n  cert_file: config/server.crt\n  key_file: config/server.key\n  client_auth: none\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(
            config.libs.contains_key("GM3000"),
            "tls must not swallow libs"
        );
        assert_eq!(config.tls_cert_path().as_deref(), Some("config/server.crt"));
        assert_eq!(config.tls_key_path().as_deref(), Some("config/server.key"));
        assert_eq!(config.tls_client_auth(), "none");
        assert!(config.validate_tls().is_ok());
    }

    #[test]
    fn mtls_without_client_ca_is_rejected() {
        let _guard = env_lock();
        clear_tls_env();
        let yaml = "default: GM3000\nvendor: {}\ntls:\n  cert_file: c.crt\n  key_file: c.key\n  client_auth: mtls\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        let error = config.validate_tls().expect_err("mtls needs a client CA");
        assert!(error.contains("client_ca_file"), "got: {error}");
    }

    #[test]
    fn mtls_with_ca_and_server_cert_is_accepted() {
        let _guard = env_lock();
        clear_tls_env();
        let yaml = "default: GM3000\nvendor: {}\ntls:\n  cert_file: c.crt\n  key_file: c.key\n  client_ca_file: ca.crt\n  client_auth: mtls\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        assert!(config.validate_tls().is_ok());
        assert_eq!(config.tls_client_ca_path().as_deref(), Some("ca.crt"));
    }

    #[test]
    fn token_without_source_is_rejected() {
        let _guard = env_lock();
        clear_tls_env();
        let yaml = "default: GM3000\nvendor: {}\ntls:\n  client_auth: token\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        let error = config.validate_tls().expect_err("token needs a source");
        assert!(error.contains("SKF_TLS_TOKEN"), "got: {error}");
    }

    #[test]
    fn token_from_token_file_is_accepted() {
        let _guard = env_lock();
        clear_tls_env();
        let token_path =
            std::env::temp_dir().join(format!("skf-config-token-{}.txt", std::process::id()));
        std::fs::write(&token_path, "file-token\n").expect("write token");
        let yaml = format!(
            "default: GM3000\nvendor: {{}}\ntls:\n  client_auth: token\n  token_file: {}\nGM3000:\n  macos: native/x.dylib\n",
            token_path.display()
        );
        let config: SkfConfig = serde_yaml::from_str(&yaml).expect("parse");
        assert_eq!(config.tls_token().as_deref(), Some("file-token"));
        assert!(config.validate_tls().is_ok());
        let _ = std::fs::remove_file(&token_path);
    }

    #[test]
    fn unknown_client_auth_is_rejected() {
        let _guard = env_lock();
        clear_tls_env();
        let yaml = "default: GM3000\nvendor: {}\ntls:\n  client_auth: jwt\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        let error = config.validate_tls().expect_err("unknown mode must fail");
        assert!(error.contains("unknown client_auth"), "got: {error}");
    }

    #[test]
    fn token_value_is_not_in_the_error_message() {
        let _guard = env_lock();
        clear_tls_env();
        std::env::set_var(SkfConfig::TLS_TOKEN_ENV, "super-secret-token");
        // Half-configured TLS, so validation fails for an unrelated reason; the
        // token value must not appear anywhere in that message.
        let yaml = "default: GM3000\nvendor: {}\ntls:\n  cert_file: c.crt\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        let error = config.validate_tls().expect_err("half config must fail");
        assert!(
            !error.contains("super-secret-token"),
            "token leaked: {error}"
        );
        clear_tls_env();
    }

    #[test]
    fn half_configured_tls_is_rejected() {
        let _guard = env_lock();
        clear_tls_env();
        let yaml = "default: GM3000\nvendor: {}\ntls:\n  cert_file: c.crt\nGM3000:\n  macos: native/x.dylib\n";
        let config: SkfConfig = serde_yaml::from_str(yaml).expect("parse");
        let error = config.validate_tls().expect_err("half config must fail");
        assert!(
            error.contains("both cert_file and key_file"),
            "got: {error}"
        );
    }

    #[test]
    fn env_overrides_yaml_and_absent_tls_is_none() {
        let _guard = env_lock();
        clear_tls_env();
        let config: SkfConfig =
            serde_yaml::from_str("default: GM3000\nvendor: {}\nGM3000:\n  macos: native/x.dylib\n")
                .expect("parse");
        assert!(config.tls_cert_path().is_none());
        assert!(config.tls_key_path().is_none());
        assert!(config.tls_client_ca_path().is_none());
        assert!(config.tls_token().is_none());
        assert_eq!(config.tls_client_auth(), "none");

        std::env::set_var(SkfConfig::TLS_CERT_ENV, "/tmp/env.crt");
        std::env::set_var(SkfConfig::TLS_KEY_ENV, "/tmp/env.key");
        std::env::set_var(SkfConfig::TLS_CLIENT_AUTH_ENV, "NONE");
        std::env::set_var(SkfConfig::TLS_CLIENT_CA_ENV, "/tmp/ca.crt");
        std::env::set_var(SkfConfig::TLS_TOKEN_ENV, "env-token");
        assert_eq!(config.tls_cert_path().as_deref(), Some("/tmp/env.crt"));
        assert_eq!(config.tls_key_path().as_deref(), Some("/tmp/env.key"));
        assert_eq!(config.tls_client_ca_path().as_deref(), Some("/tmp/ca.crt"));
        assert_eq!(config.tls_token().as_deref(), Some("env-token"));
        assert_eq!(config.tls_client_auth(), "none");
        assert!(config.validate_tls().is_ok());

        clear_tls_env();
    }

    /// Clear the Phase 7/8 TLS environment overrides.
    fn clear_tls_env() {
        for key in [
            SkfConfig::TLS_CERT_ENV,
            SkfConfig::TLS_KEY_ENV,
            SkfConfig::TLS_CLIENT_AUTH_ENV,
            SkfConfig::TLS_CLIENT_CA_ENV,
            SkfConfig::TLS_TOKEN_ENV,
            SkfConfig::TLS_TOKEN_FILE_ENV,
        ] {
            std::env::remove_var(key);
        }
    }
}
