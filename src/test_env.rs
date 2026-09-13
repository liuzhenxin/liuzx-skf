//! Shared process-environment lock for unit tests.
//!
//! Environment variables are process-global, so any test that sets or reads an
//! env-driven configuration value must hold this lock. Without it, tests that run
//! in parallel threads observe each other's overrides.

use std::sync::{Mutex, MutexGuard};

/// Acquire the process-wide test environment lock.
pub fn lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
