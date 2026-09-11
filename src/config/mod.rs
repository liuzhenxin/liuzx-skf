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

/// Raw provider configuration as read from YAML.
///
/// `vendor` maps `VID:PID` strings to provider aliases. The flattened `libs`
/// map is `provider alias -> { os name -> library path }`.
#[derive(Debug, Deserialize, Clone)]
pub struct SkfConfig {
    pub default: String,
    pub vendor: HashMap<String, String>,
    #[serde(flatten)]
    pub libs: HashMap<String, HashMap<String, String>>,
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
            m.get(os)
                .or_else(|| m.iter().find(|(k, _)| k.eq_ignore_ascii_case(os)).map(|(_, v)| v))
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

    for (alias, _platforms) in &config.libs {
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
        assert_eq!(expand_env_vars("%SKF_DEFINITELY_UNSET_VAR%"), "%SKF_DEFINITELY_UNSET_VAR%");
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
        platforms.insert("Windows".to_string(), "native\\GM3000\\windows\\mtoken_gm3000.dll".to_string());
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
            libs,
        };

        let problems = validate(&config, "macos");
        assert!(
            problems.iter().any(|p| p.contains("no path configured for OS 'macos'")),
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
            libs,
        };

        let problems = validate(&config, "macos");
        assert!(problems.iter().any(|p| p.contains("default provider 'ABSENT'")));
        assert!(problems.iter().any(|p| p.contains("unknown provider 'MISSING'")));
    }
}
