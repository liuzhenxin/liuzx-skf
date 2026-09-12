//! Classification coverage invariants (TRANS-07).
//!
//! The classification is only trustworthy if it covers exactly the methods the
//! service exposes. `EXPECTED_METHODS` is the existing authority for that set
//! (used by the fixture completeness test), so this compares against it.

mod common;

use std::collections::HashSet;

use skf_service::domain::classification::{classify, OperationClass, CLASSIFIED_METHODS};

#[test]
fn every_expected_method_is_classified() {
    let missing: Vec<&str> = common::EXPECTED_METHODS
        .iter()
        .copied()
        .filter(|m| classify(m).is_none())
        .collect();

    assert!(
        missing.is_empty(),
        "every method must have an OperationClass; missing: {:?}",
        missing
    );
}

#[test]
fn no_method_is_classified_twice() {
    let mut seen = HashSet::new();
    for (method, _) in CLASSIFIED_METHODS {
        assert!(
            seen.insert(*method),
            "method classified more than once: {}",
            method
        );
    }
}

#[test]
fn the_classified_set_equals_the_expected_set() {
    let classified: HashSet<&str> = CLASSIFIED_METHODS.iter().map(|(m, _)| *m).collect();
    let expected: HashSet<&str> = common::EXPECTED_METHODS.iter().copied().collect();

    let extra: Vec<&&str> = classified.difference(&expected).collect();
    assert!(
        extra.is_empty(),
        "the classification names methods the service does not expose: {:?}",
        extra
    );
    assert_eq!(
        classified.len(),
        expected.len(),
        "classification and EXPECTED_METHODS must have the same size"
    );
}

#[test]
fn the_documented_destructive_methods_are_destructive() {
    for method in [
        "DeleteContainer",
        "ImportCertificate",
        "LockDev",
        "SetLabel",
        "CheckPIN",
        "GenECCKeyPair",
    ] {
        assert_eq!(
            classify(method),
            Some(OperationClass::Destructive),
            "{} must be Destructive",
            method
        );
    }
}
