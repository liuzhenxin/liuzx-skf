//! Structural invariants for the session layer.
//!
//! These protect properties that the type system cannot express, so they are
//! asserted against source text. Every check is **comment-aware**: a doc comment
//! that names a forbidden construct in order to forbid it must not fail the check.
//! (Phase 1's review found three criteria that counted doc comments; this file is
//! the comment-aware replacement for that class of assertion.)

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

fn read(relative: &str) -> String {
    let path = manifest_dir().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e))
}

/// Count occurrences of `needle` on lines that are real code, not comments.
fn count_code_occurrences(text: &str, needle: &str) -> usize {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//")
        })
        .filter(|line| line.contains(needle))
        .count()
}

/// Token-aware containment so `HANDLE` does not match `HANDLE_SIZE`.
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

fn code_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with("//"))
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
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
        for _ in 0..count_code_occurrences(&text, "unsafe impl Send") {
            send_sites.push(relative.clone());
        }
        for _ in 0..count_code_occurrences(&text, "unsafe impl Sync") {
            sync_sites.push(relative.clone());
        }
    }

    assert_eq!(
        send_sites.len(),
        1,
        "exactly one `unsafe impl Send` is allowed in the crate, found {:?}. \
         Session state must stay by-value so no new assertion is needed.",
        send_sites
    );
    assert_eq!(
        sync_sites.len(),
        1,
        "exactly one `unsafe impl Sync` is allowed in the crate, found {:?}",
        sync_sites
    );
    assert_eq!(send_sites[0], "src/skf/types.rs");
    assert_eq!(sync_sites[0], "src/skf/types.rs");
}

/// Session state is reached through `&mut self`, so it must not carry locks of its
/// own. A lock here would mean session state is being shared, which is the defect
/// this phase removes.
#[test]
fn session_state_has_no_interior_mutability() {
    let module = read("src/session/mod.rs");
    for forbidden in ["Mutex", "RwLock", "RefCell", "Cell<"] {
        let hits: Vec<usize> = code_lines(&module)
            .enumerate()
            .filter(|(_, line)| line.contains(forbidden))
            .map(|(n, _)| n + 1)
            .collect();
        assert!(
            hits.is_empty(),
            "src/session/mod.rs must not use `{}` (line(s) {:?}); session state is \
             exclusive per connection and needs no lock",
            forbidden,
            hits
        );
    }
}

/// The handle table is owned by the session and stored inside `HandleTable`, so it
/// must be free of native pointer types.
#[test]
fn handle_table_exposes_no_native_pointers() {
    // The handle table lands in plan 02-03; tolerate its absence so this file can
    // run from the moment the session layer exists.
    if !manifest_dir().join("src/session/handles.rs").exists() {
        println!("no handle table yet");
        return;
    }
    let handles = read("src/session/handles.rs");
    for forbidden in [
        "HANDLE",
        "DEVHANDLE",
        "HAPPLICATION",
        "HCONTAINER",
        "SendHandle",
        "*mut c_void",
    ] {
        let hits: Vec<usize> = code_lines(&handles)
            .enumerate()
            .filter(|(_, line)| contains_token(line, forbidden))
            .map(|(n, _)| n + 1)
            .collect();
        assert!(
            hits.is_empty(),
            "src/session/handles.rs must not name `{}` (line(s) {:?})",
            forbidden,
            hits
        );
    }
}

/// A request may only reach its own session's state, so the registry must not offer
/// a lookup and must not modify session state.
#[test]
fn registry_exposes_no_session_lookup_or_mutation() {
    let registry = read("src/session/registry.rs");
    for forbidden in [
        "pub fn get(",
        "pub fn list(",
        "pub fn iter(",
        "pub fn sessions(",
        "invalidate_for_device",
    ] {
        let hits: Vec<usize> = code_lines(&registry)
            .enumerate()
            .filter(|(_, line)| line.contains(forbidden))
            .map(|(n, _)| n + 1)
            .collect();
        assert!(
            hits.is_empty(),
            "src/session/registry.rs must not expose `{}` (line(s) {:?}); the registry \
             tracks liveness only",
            forbidden,
            hits
        );
    }
}

/// Session ids must never be handed to clients: a distributed id becomes a
/// credential, and one that survives reconnection would contradict clear-on-disconnect.
#[test]
fn session_ids_are_not_sent_to_clients() {
    for relative in ["src/main.rs", "src/server/mod.rs"] {
        let text = read(relative);
        let hits: Vec<usize> = code_lines(&text)
            .enumerate()
            .filter(|(_, line)| line.contains("RpcResponse::ok"))
            .filter(|(_, line)| {
                let lowered = line.to_lowercase();
                lowered.contains("session") || lowered.contains("guard.id")
            })
            .map(|(n, _)| n + 1)
            .collect();
        assert!(
            hits.is_empty(),
            "{} must not return a session identifier to the client (line(s) {:?})",
            relative,
            hits
        );
    }
}

/// The session state must stay reachable through a plain `&mut`, which is what
/// keeps session ownership lock-free. This asserts the accessor shape rather than
/// the type, because the interesting failure is replacing it with a lock.
#[test]
fn session_guard_exposes_mutable_state_by_reference() {
    let registry = read("src/session/registry.rs");
    assert!(
        count_code_occurrences(&registry, "pub fn state_mut(&mut self) -> &mut SessionState") == 1,
        "SessionGuard must expose `&mut SessionState`; anything else implies shared state"
    );
}
