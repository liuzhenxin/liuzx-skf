#![allow(dead_code)]

//! Shared helpers for the v0.2.0 contract fixtures.
//!
//! Included by both `tests/fixture_oracle.rs` and `tests/contract_fixtures.rs`
//! via `mod common;`, so the normalization rules live in exactly one place.
//!
//! The fixtures in `tests/fixtures/v0.2.0/` were recorded from the **pre-refactor**
//! service. This module owns two responsibilities:
//!
//! 1. Normalizing volatile fields that were declared *at record time* (never
//!    widened afterwards — see `tests/fixtures/v0.2.0/README.md`).
//! 2. Proving the oracle can actually fail. A regression suite that cannot fail
//!    is worthless, so the negative self-tests here are not optional.
//!
//! The end-to-end replay driver lives in `tests/contract_fixtures.rs` and reuses
//! `apply_normalize` from this module.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// A placeholder marker is any string shaped like `<...>`.
fn is_marker(value: &Value) -> bool {
    match value.as_str() {
        Some(s) => s.starts_with('<') && s.ends_with('>') && s.len() >= 3,
        None => false,
    }
}

/// Marker meaning "compare this array without regard to element order".
///
/// Needed because `EnumProvider` iterates a `HashMap`, so the provider list is
/// the same set in a different order on every process start. Sorting keeps the
/// contents asserted while ignoring the unstable ordering.
pub const UNORDERED_MARKER: &str = "<unordered>";

/// Replace every path declared in `spec` with its marker string.
///
/// `spec` mirrors the response shape; leaf values shaped like `<marker>` are
/// substituted into the actual value at the same path, and the special marker
/// `<unordered>` sorts the array at that path. Paths absent from `spec` are left
/// untouched, which is what makes an undeclared change detectable.
pub fn apply_normalize(actual: Value, spec: &Value) -> Value {
    match (actual, spec) {
        (Value::Object(actual_map), Value::Object(spec_map)) => {
            let mut out = serde_json::Map::with_capacity(actual_map.len());
            for (key, value) in actual_map {
                match spec_map.get(&key) {
                    Some(spec_value) => out.insert(key, apply_normalize(value, spec_value)),
                    None => out.insert(key, value),
                };
            }
            Value::Object(out)
        }
        (Value::Array(items), Value::Array(spec_items)) => {
            // Declarations for arrays are positional; extra actual elements are
            // passed through so a change in length stays detectable.
            let normalized = items
                .into_iter()
                .enumerate()
                .map(|(index, item)| match spec_items.get(index) {
                    Some(spec_item) => apply_normalize(item, spec_item),
                    None => item,
                })
                .collect();
            Value::Array(normalized)
        }
        (actual, spec) if spec.as_str() == Some(UNORDERED_MARKER) => match actual {
            Value::Array(items) => {
                let mut sorted: Vec<Value> = items.into_iter().collect();
                sorted.sort_by_key(|item| item.to_string());
                Value::Array(sorted)
            }
            other => other,
        },
        (actual, spec) if is_marker(spec) => spec.clone(),
        (actual, _) => actual,
    }
}

/// A recorded contract case.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub method: String,
    /// Prerequisite requests replayed before `request` on the same connection.
    pub setup: Vec<Value>,
    pub request: Value,
    pub response: Value,
    pub normalize: Value,
    pub provenance: Value,
    pub path: PathBuf,
}

impl Fixture {
    /// Normalize an actual response using this fixture's recorded declaration.
    pub fn normalize_actual(&self, actual: Value) -> Value {
        apply_normalize(actual, &self.normalize)
    }

    /// Normalize the recorded response the same way, so both sides of the
    /// comparison have volatile fields erased consistently.
    pub fn normalized_expected(&self) -> Value {
        apply_normalize(self.response.clone(), &self.normalize)
    }
}

pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/v0.2.0")
}

pub fn load_fixture(path: &Path) -> Result<Fixture, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: invalid JSON: {}", path.display(), e))?;

    let method = value
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{}: missing 'method'", path.display()))?
        .to_string();

    let setup = value
        .get("setup")
        .map(|v| match v.as_array() {
            Some(items) => items.clone(),
            None => Vec::new(),
        })
        .unwrap_or_default();

    let request = value
        .get("request")
        .cloned()
        .ok_or_else(|| format!("{}: missing 'request'", path.display()))?;
    let response = value
        .get("response")
        .cloned()
        .ok_or_else(|| format!("{}: missing 'response'", path.display()))?;
    let normalize = value
        .get("normalize")
        .cloned()
        .ok_or_else(|| format!("{}: missing 'normalize'", path.display()))?;
    let provenance = value
        .get("provenance")
        .cloned()
        .ok_or_else(|| format!("{}: missing 'provenance'", path.display()))?;

    Ok(Fixture {
        method,
        setup,
        request,
        response,
        normalize,
        provenance,
        path: path.to_path_buf(),
    })
}

/// All fixtures sorted by method name. Empty until the recorder has been run.
pub fn load_all() -> Result<Vec<Fixture>, String> {
    let dir = fixtures_dir();
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("{}: {}", dir.display(), e))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    paths.sort();

    let mut fixtures = Vec::with_capacity(paths.len());
    for path in paths {
        fixtures.push(load_fixture(&path)?);
    }
    Ok(fixtures)
}

/// Methods the v0.2.0 service exposes. Used to assert the recording is complete.
pub const EXPECTED_METHODS: &[&str] = &[
    "SetLanguage",
    "WaitForDevEvent",
    "EnumProvider",
    "EnumDevice",
    "ConnectDev",
    "EnumApplication",
    "EnumContainer",
    "DeleteContainer",
    "IssueCertificate",
    "ImportCertificate",
    "SignData",
    "DisConnectDev",
    "FindCertificates",
    "GenerateRandom",
    "Digest",
    "CheckPIN",
    "CreatePKCS10",
    "EncryptData",
    "DecryptData",
    "GetDevInfo",
    "GetDevState",
    "SetLabel",
    "ECCVerify",
    "CreateContainer",
    "GetContainerType",
    "RSASignData",
    "LockDev",
    "UnlockDev",
    "Transmit",
    "CancelWaitForDevEvent",
    "GenECCKeyPair",
    "GenRSAKeyPair",
    "RSAVerify",
    "DigestInit",
    "DigestUpdate",
    "DigestFinal",
    "CloseHash",
];
