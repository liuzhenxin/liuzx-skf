//! Contract oracle for the v0.2.0 JSON-RPC surface.
//!
//! The fixtures in `tests/fixtures/v0.2.0/` were recorded from the **pre-refactor**
//! service. This file owns two responsibilities:
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

use serde_json::{json, Value};

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

#[cfg(test)]
mod tests {
    use super::*;

    /// An in-memory fixture keeps these tests independent of the recorder, so
    /// they pass both before and after the fixtures are written.
    ///
    /// `response.result` holds the raw recorded value; normalization maps it to
    /// a marker on BOTH sides of the comparison.
    fn in_memory_fixture() -> (Value, Value) {
        let response = json!({ "error": 0, "result": "140234567890123", "id": 1 });
        let normalize = json!({ "result": "<opaque:handle>" });
        (response, normalize)
    }

    /// Both sides must be normalized before comparison.
    fn compare(actual: Value, expected: &Value, normalize: &Value) -> bool {
        apply_normalize(actual, normalize) == apply_normalize(expected.clone(), normalize)
    }

    /// The oracle MUST be able to fail. If this test starts passing with equal
    /// values for mutated input, the whole regression suite is meaningless.
    #[test]
    fn oracle_rejects_mutated_response() {
        let (response, normalize) = in_memory_fixture();
        let mut mutated = response.clone();
        mutated["error"] = json!(-99);

        assert!(
            !compare(mutated, &response, &normalize),
            "a mutated 'error' field must survive normalization and be detected"
        );
    }

    /// `<unordered>` must make array order irrelevant WITHOUT hiding content.
    #[test]
    fn oracle_ignores_declared_array_order_but_still_checks_contents() {
        let expected = json!({ "error": 0, "result": ["a", "b", "c"], "id": 1 });
        let normalize = json!({ "result": UNORDERED_MARKER });

        // Same elements, different order: must be accepted.
        let reordered = json!({ "error": 0, "result": ["c", "a", "b"], "id": 1 });
        assert_eq!(
            apply_normalize(reordered, &normalize),
            apply_normalize(expected.clone(), &normalize),
            "declared unordered path must ignore element order"
        );

        // Different contents: must still be rejected.
        let changed = json!({ "error": 0, "result": ["c", "a", "zzz"], "id": 1 });
        assert_ne!(
            apply_normalize(changed, &normalize),
            apply_normalize(expected.clone(), &normalize),
            "unordered comparison must not hide a changed element"
        );

        // Different length: must still be rejected.
        let shorter = json!({ "error": 0, "result": ["a", "b"], "id": 1 });
        assert_ne!(
            apply_normalize(shorter, &normalize),
            apply_normalize(expected, &normalize),
            "unordered comparison must not hide a removed element"
        );
    }

    /// Only declared paths may be ignored; anything else must still be compared.
    #[test]
    fn oracle_accepts_only_normalized_differences() {
        let (response, normalize) = in_memory_fixture();

        // Declared volatile path: normalization must erase the difference.
        let mut volatile_changed = response.clone();
        volatile_changed["result"] = json!("987654321098765");
        assert!(
            compare(volatile_changed, &response, &normalize),
            "declared volatile path must normalize away the difference"
        );

        // Undeclared path: must NOT be erased.
        let mut undeclared_changed = response.clone();
        undeclared_changed["id"] = json!(42);
        assert!(
            !compare(undeclared_changed, &response, &normalize),
            "an undeclared change must not be normalized away"
        );
    }

    /// Structural check over whatever has been recorded so far. Tolerates an
    /// empty directory so it can run before recording completes; the count
    /// assertion lives in the recording task.
    #[test]
    fn all_fixtures_are_wellformed() {
        let fixtures = load_all().expect("fixtures directory readable");
        if fixtures.is_empty() {
            println!("no fixtures yet");
            return;
        }

        for fixture in &fixtures {
            let stem = fixture
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            assert_eq!(
                stem, fixture.method,
                "{}: file name must match the 'method' field",
                fixture.path.display()
            );
            assert_eq!(
                fixture.provenance.get("volatile_fields_reviewed"),
                Some(&json!(true)),
                "{}: volatile fields must have been reviewed at record time",
                fixture.path.display()
            );
        }

        println!("fixture count: {}", fixtures.len());
    }
}
