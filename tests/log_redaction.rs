//! Redaction discipline for log output (OBS-02).
//!
//! The service must never log a PIN, key, session key, or decrypted payload.
//! Today no call site does; this test keeps it that way by scanning the source
//! for logging macros that mention a sensitive identifier. A deliberate exception
//! is allowed with a `// redaction-allow: <reason>` comment on the same line.

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn source_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&manifest_dir().join("src"), &mut files);
    files.sort();
    files
}

/// Token-aware containment so `pin` does not match `pinned` or `spin`.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = 0;
    while let Some(offset) = haystack[start..].find(needle) {
        let index = start + offset;
        let before_ok = haystack[..index]
            .chars()
            .next_back()
            .map(|c| !is_word(c))
            .unwrap_or(true);
        let after_ok = haystack[index + needle.len()..]
            .chars()
            .next()
            .map(|c| !is_word(c))
            .unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        start = index + needle.len();
    }
    false
}

/// True when the line invokes a logging or direct-print macro.
fn is_log_macro_line(line: &str) -> bool {
    const MACROS: &[&str] = &[
        "log::trace!",
        "log::debug!",
        "log::info!",
        "log::warn!",
        "log::error!",
        "println!",
        "eprintln!",
    ];
    MACROS.iter().any(|m| line.contains(m))
}

const SENSITIVE_WORDS: &[&str] = &[
    "pin",
    "pin_str",
    "key_bytes",
    "sym_key",
    "private_key",
    "decrypted",
    "cert_bytes",
    "payload",
    // Phase 7 TLS material: paths are as sensitive as contents, because they
    // reveal the install layout (TLS-04).
    "key_file",
    "cert_file",
    "tls_key",
];

#[test]
fn no_log_macro_references_sensitive_identifiers() {
    let mut violations = Vec::new();
    for path in source_files() {
        let text = fs::read_to_string(&path).expect("read source");
        for (number, line) in text.lines().enumerate() {
            if !is_log_macro_line(line) {
                continue;
            }
            if line.contains("redaction-allow:") {
                continue;
            }
            let sensitive = SENSITIVE_WORDS.iter().any(|w| contains_word(line, w))
                || line.contains("req.params");
            if sensitive {
                violations.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "logging must not reference sensitive values; see tests/log_redaction.rs:\n{}",
        violations.join("\n")
    );
}

#[test]
fn no_log_macro_logs_request_params() {
    for path in source_files() {
        let text = fs::read_to_string(&path).expect("read source");
        for (number, line) in text.lines().enumerate() {
            if is_log_macro_line(line) && line.contains("req.params") {
                panic!(
                    "{}:{}: request parameters must not be logged: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                );
            }
        }
    }
}

#[test]
fn logging_module_sanitizes_external_values() {
    let cleaned = skf_service::logging::sanitize("value\nwith\rcontrols\u{0}");
    assert!(!cleaned.contains('\n'));
    assert!(!cleaned.contains('\r'));
    assert!(!cleaned.contains('\u{0}'));
    assert_eq!(cleaned, "valuewithcontrols");
}
