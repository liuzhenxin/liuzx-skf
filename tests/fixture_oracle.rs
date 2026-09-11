//! Oracle self-tests: proof that the fixture comparison can actually fail.
//!
//! A regression suite that cannot fail is worthless, so these are not optional.
//! The end-to-end replay lives in `tests/contract_fixtures.rs` and shares the
//! comparison rules through `tests/common/mod.rs`.

mod common;

use common::{apply_normalize, load_all, UNORDERED_MARKER};
use serde_json::{json, Value};

/// An in-memory fixture keeps these tests independent of the recorder, so they
/// pass both before and after the fixtures are written.
///
/// `response.result` holds the raw recorded value; normalization maps it to a
/// marker on BOTH sides of the comparison.
fn in_memory_fixture() -> (Value, Value) {
    let response = json!({ "error": 0, "result": "140234567890123", "id": 1 });
    let normalize = json!({ "result": "<opaque:handle>" });
    (response, normalize)
}

/// Both sides must be normalized before comparison.
fn compare(actual: Value, expected: &Value, normalize: &Value) -> bool {
    apply_normalize(actual, normalize) == apply_normalize(expected.clone(), normalize)
}

/// The oracle MUST be able to fail. If this test ever passes for mutated input,
/// the whole regression suite is meaningless.
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

    let reordered = json!({ "error": 0, "result": ["c", "a", "b"], "id": 1 });
    assert_eq!(
        apply_normalize(reordered, &normalize),
        apply_normalize(expected.clone(), &normalize),
        "declared unordered path must ignore element order"
    );

    let changed = json!({ "error": 0, "result": ["c", "a", "zzz"], "id": 1 });
    assert_ne!(
        apply_normalize(changed, &normalize),
        apply_normalize(expected.clone(), &normalize),
        "unordered comparison must not hide a changed element"
    );

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

    let mut volatile_changed = response.clone();
    volatile_changed["result"] = json!("987654321098765");
    assert!(
        compare(volatile_changed, &response, &normalize),
        "declared volatile path must normalize away the difference"
    );

    let mut undeclared_changed = response.clone();
    undeclared_changed["id"] = json!(42);
    assert!(
        !compare(undeclared_changed, &response, &normalize),
        "an undeclared change must not be normalized away"
    );
}

/// Structural check over whatever has been recorded so far. The count assertion
/// belongs to `tests/contract_fixtures.rs`; here an empty directory is tolerated
/// so this file can run before recording completes.
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
