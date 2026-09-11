//! Structural invariants for the provider boundary.
//!
//! These are asserted against the source text rather than the type system,
//! because the properties being protected are about *where* things are allowed
//! to appear:
//!
//! * exactly one `unsafe impl Send`/`Sync` may exist in the crate;
//! * no native handle type may cross the provider boundary;
//! * the vendor library must be loaded exactly once.
//!
//! Each of these is load-bearing for Phase 2: the opaque-identifier work
//! (RES-01) is only meaningful if no raw pointer reaches the trait surface, and
//! deterministic release (RES-03/RES-04) is only possible if handles do not
//! outlive the library that created them.

use std::fs;
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Collect every `.rs` file under `src/`, sorted for stable failure output.
fn source_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {}", dir.display(), e));
        for entry in entries {
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

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// Count occurrences of `needle`, ignoring lines that are comments.
fn count_code_occurrences(path: &Path, needle: &str) -> usize {
    read(path)
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
        })
        .filter(|line| line.contains(needle))
        .count()
}

#[test]
fn only_one_unsafe_send_sync_impl_exists() {
    let mut send_sites = Vec::new();
    let mut sync_sites = Vec::new();

    for path in source_files() {
        let relative = path
            .strip_prefix(manifest_dir())
            .unwrap_or(&path)
            .display()
            .to_string();
        let sends = count_code_occurrences(&path, "unsafe impl Send");
        let syncs = count_code_occurrences(&path, "unsafe impl Sync");
        for _ in 0..sends {
            send_sites.push(relative.clone());
        }
        for _ in 0..syncs {
            sync_sites.push(relative.clone());
        }
    }

    assert_eq!(
        send_sites.len(),
        1,
        "exactly one `unsafe impl Send` is allowed in the crate, found {:?}. \
         New guard types must store handles as integers instead of asserting \
         Send/Sync on raw pointers.",
        send_sites
    );
    assert_eq!(
        sync_sites.len(),
        1,
        "exactly one `unsafe impl Sync` is allowed in the crate, found {:?}",
        sync_sites
    );
    assert_eq!(
        send_sites[0], "src/skf/types.rs",
        "the sole Send assertion must remain the SKF handle wrapper"
    );
    assert_eq!(
        sync_sites[0], "src/skf/types.rs",
        "the sole Sync assertion must remain the SKF handle wrapper"
    );
}

#[test]
fn provider_boundary_exposes_no_native_pointers() {
    let boundary = manifest_dir().join("src/provider/mod.rs");
    let text = read(&boundary);

    for forbidden in [
        "HANDLE",
        "DEVHANDLE",
        "HAPPLICATION",
        "HCONTAINER",
        "SendHandle",
        "*mut c_void",
    ] {
        // `Operation::CloseDigest` and similar identifiers legitimately contain
        // uppercase words, so match the exact token with word boundaries.
        let hits: Vec<(usize, &str)> = text
            .lines()
            .enumerate()
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
            .filter(|(_, line)| contains_token(line, forbidden))
            .collect();
        assert!(
            hits.is_empty(),
            "src/provider/mod.rs must not name `{}`; found on line(s) {:?}. \
             Native types belong to src/provider/native.rs only.",
            forbidden,
            hits.iter().map(|(n, _)| n + 1).collect::<Vec<_>>()
        );
    }
}

/// Token-aware containment: `HANDLE` must not match `HANDLE_SIZE` or `SomeHANDLE`.
fn contains_token(haystack: &str, needle: &str) -> bool {
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

#[test]
fn native_provider_loads_library_exactly_once() {
    let native = manifest_dir().join("src/provider/native.rs");
    let count = count_code_occurrences(&native, "Library::new");
    assert_eq!(
        count, 1,
        "src/provider/native.rs must load the vendor library exactly once. \
         Re-loading per call invalidates handles handed out by earlier calls; \
         the pre-refactor code did this on ten request paths."
    );
}

#[test]
fn provider_guards_implement_drop() {
    let native = read(&manifest_dir().join("src/provider/native.rs"));
    for guard in [
        "NativeDevice",
        "NativeApplication",
        "NativeContainer",
        "NativeDigest",
    ] {
        let needle = format!("impl Drop for {}", guard);
        assert!(
            native.contains(&needle),
            "{} must release its native handle in Drop, otherwise RES-03/RES-04 \
             cannot be satisfied in Phase 2",
            guard
        );
    }
}

#[test]
fn fake_provider_is_not_compiled_into_a_release_build() {
    // The fake is feature-gated so test doubles never ship. This asserts the gate
    // is still present in source; `cargo build` (without the feature) is the
    // runtime proof and runs in CI.
    let boundary = read(&manifest_dir().join("src/provider/mod.rs"));
    assert!(
        boundary.contains(r#"#[cfg(any(test, feature = "test-provider"))]"#),
        "the fake provider must stay behind cfg(test)/feature(test-provider)"
    );
}

#[test]
fn source_tree_has_no_leftover_raw_handle_fields_in_guards() {
    // Guards must store handles as integers so they are Send + Sync without a new
    // `unsafe impl`. A raw-pointer field here would silently reintroduce the
    // problem the invariant test above forbids.
    let native = read(&manifest_dir().join("src/provider/native.rs"));
    for forbidden in [
        "raw: DEVHANDLE,",
        "raw: HAPPLICATION,",
        "raw: HCONTAINER,",
        "raw: HANDLE,",
    ] {
        assert!(
            !native.contains(forbidden),
            "guard field `{}` must be stored as usize, not a raw pointer",
            forbidden
        );
    }
}

/// `WaitForDevEvent` and `CancelWaitForDevEvent` must bypass the FFI gate.
///
/// They are the one process-global pair that is designed to run concurrently:
/// cancel has to reach the library while a wait is parked. If either took the
/// per-provider lock, cancel would queue behind wait and the pair would
/// deadlock (TRANS-05, decision D-02).
#[test]
fn wait_and_cancel_do_not_take_the_ffi_gate() {
    let native = read(&manifest_dir().join("src/provider/native.rs"));
    for method in ["fn wait_for_event(", "fn cancel_wait_for_event("] {
        let start = native
            .find(method)
            .unwrap_or_else(|| panic!("{} not found", method));
        let rest = &native[start..];
        // Slice up to the next method definition.
        let end = rest[1..]
            .find("\n    fn ")
            .map(|i| i + 1)
            .unwrap_or(rest.len());
        let block = &rest[..end];
        assert!(
            !block.contains("gate_lock"),
            "{} must not take the FFI gate; cancel would deadlock behind wait",
            method
        );
    }
}
