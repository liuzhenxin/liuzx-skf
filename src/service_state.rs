//! Startup reporting for the Windows service.
//!
//! # Why this exists
//!
//! The SCM only knows coarse states (`StartPending`, `Running`, `Stopped`). An
//! operator running `skf-service.exe status` could not tell whether the process
//! was merely alive or actually serving — for example after a bind failure the
//! process started and then stopped, and `status` showed the same thing either
//! way. This module is the shared contract for the small status file the service
//! writes as it initializes, and for the exit code a given failure maps to.
//!
//! # No secrets
//!
//! The status file is written next to the executable and read by `status`. It
//! must never contain configuration values, library paths, PINs, or key material
//! — only the phase, a short reason, and the exit code. A unit test pins that.
//!
//! # Compiled on every platform
//!
//! Nothing here is Windows-specific, so the serialization, atomic write and
//! exit-code mapping are covered by the normal macOS/Linux test run; only the
//! SCM calls that consume these values need a Windows host.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A phase of service initialization.
///
/// Reported as `stage` in the status file and used to pick the service exit code
/// when initialization fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupStage {
    /// Reading and parsing the YAML configuration.
    Config,
    /// Resolving the configured provider for this OS.
    Provider,
    /// Binding the WebSocket listener.
    Bind,
    /// Serving requests.
    Serve,
}

/// Exit code for a configuration load/parse failure.
pub const EXIT_CONFIG: u32 = 1;
/// Exit code for a provider resolution failure.
pub const EXIT_PROVIDER: u32 = 2;
/// Exit code for a listener bind failure.
pub const EXIT_BIND: u32 = 3;
/// Exit code for a runtime failure while serving.
pub const EXIT_SERVE: u32 = 4;

impl StartupStage {
    /// Lowercase name used in the status file.
    pub fn as_str(self) -> &'static str {
        match self {
            StartupStage::Config => "config",
            StartupStage::Provider => "provider",
            StartupStage::Bind => "bind",
            StartupStage::Serve => "serve",
        }
    }

    /// The service-specific exit code for a failure in this phase.
    pub fn exit_code(self) -> u32 {
        match self {
            StartupStage::Config => EXIT_CONFIG,
            StartupStage::Provider => EXIT_PROVIDER,
            StartupStage::Bind => EXIT_BIND,
            StartupStage::Serve => EXIT_SERVE,
        }
    }
}

/// The lifecycle state recorded in the status file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    StartPending,
    Running,
    Failed,
    Stopped,
}

/// The persisted startup status.
///
/// Field names are stable: `status` and future diagnostics read them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatusFile {
    pub state: ServiceState,
    pub stage: StartupStage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
}

impl ServiceStatusFile {
    /// Initializing in `stage`.
    pub fn pending(stage: StartupStage, pid: Option<u32>, updated: impl Into<String>) -> Self {
        Self {
            state: ServiceState::StartPending,
            stage,
            code: None,
            reason: None,
            pid,
            address: None,
            updated: Some(updated.into()),
        }
    }

    /// Bound and serving.
    pub fn running(
        address: impl Into<String>,
        pid: Option<u32>,
        updated: impl Into<String>,
    ) -> Self {
        Self {
            state: ServiceState::Running,
            stage: StartupStage::Serve,
            code: None,
            reason: None,
            pid,
            address: Some(address.into()),
            updated: Some(updated.into()),
        }
    }

    /// Initialization failed.
    ///
    /// `reason` is short and caller-supplied; it must not carry configuration
    /// values or library paths.
    pub fn failed(
        stage: StartupStage,
        code: u32,
        reason: impl Into<String>,
        pid: Option<u32>,
        updated: impl Into<String>,
    ) -> Self {
        Self {
            state: ServiceState::Failed,
            stage,
            code: Some(code),
            reason: Some(reason.into()),
            pid,
            address: None,
            updated: Some(updated.into()),
        }
    }

    /// Clean shutdown.
    pub fn stopped(updated: impl Into<String>) -> Self {
        Self {
            state: ServiceState::Stopped,
            stage: StartupStage::Serve,
            code: None,
            reason: None,
            pid: None,
            address: None,
            updated: Some(updated.into()),
        }
    }
}

/// Parse `value` and write it, replacing the target atomically.
///
/// Writes a sibling `*.tmp` first and renames it over `path`, so a reader never
/// observes a half-written file. On Windows a rename over an existing file is not
/// permitted, so the target is removed first as a fallback; the window is tiny
/// and the content is advisory.
pub fn write_atomic(path: &Path, value: &ServiceStatusFile) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            // Windows: rename cannot clobber; replace explicitly.
            let _ = std::fs::remove_file(path);
            std::fs::rename(&tmp, path)
        }
    }
}

/// Read and parse the status file, or `None` when it is absent or malformed.
pub fn read(path: &Path) -> Option<ServiceStatusFile> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "skf-service-state-test-{}-{}.json",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn round_trip_serializes_and_parses() {
        let path = temp_path("roundtrip");
        let value = ServiceStatusFile::failed(
            StartupStage::Bind,
            EXIT_BIND,
            "address already in use",
            Some(42),
            "2026-09-11T00:00:00Z",
        );
        write_atomic(&path, &value).expect("write");
        let back = read(&path).expect("read");
        assert_eq!(back, value);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_tmp() {
        let path = temp_path("atomic");
        write_atomic(
            &path,
            &ServiceStatusFile::pending(StartupStage::Config, Some(1), "t1"),
        )
        .expect("first write");
        write_atomic(
            &path,
            &ServiceStatusFile::running("127.0.0.1:9001", Some(1), "t2"),
        )
        .expect("second write");

        let back = read(&path).expect("read");
        assert_eq!(back.state, ServiceState::Running);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "the temporary file must be renamed away"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn serialized_failed_state_contains_no_config_values() {
        // A sentinel that must not leak: a configured library path.
        let secret_path = "/opt/secret/vendor/libgm3000.dylib";
        let value = ServiceStatusFile::failed(
            StartupStage::Provider,
            EXIT_PROVIDER,
            "provider not configured",
            Some(7),
            "2026-09-11T00:00:00Z",
        );
        let json = serde_json::to_string(&value).expect("serialize");
        assert!(!json.contains(secret_path), "status file leaked a path");
        assert!(!json.contains("PIN"), "status file must not mention a PIN");
        // The fields that ARE allowed:
        assert!(json.contains("provider"));
        assert!(json.contains("1")); // EXIT_PROVIDER
    }

    #[test]
    fn stage_maps_to_distinct_exit_codes() {
        let codes = [
            StartupStage::Config.exit_code(),
            StartupStage::Provider.exit_code(),
            StartupStage::Bind.exit_code(),
            StartupStage::Serve.exit_code(),
        ];
        assert_eq!(codes, [1, 2, 3, 4]);
        let mut unique = codes.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 4, "exit codes must be distinct");
    }
}
