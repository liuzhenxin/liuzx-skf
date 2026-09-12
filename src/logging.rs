//! Structured, rotating service logging.
//!
//! # Why this module exists
//!
//! Before Phase 6 the Windows service redirected stdout/stderr to one unbounded
//! `skf-service.log` and used `env_logger`'s plain format. That file grows without
//! limit and is not machine-parseable, so an operator cannot rotate it, retain a
//! bounded history, or feed it to a log processor (OBS-01).
//!
//! This module adds a small `log::Log` implementation that writes one JSON object
//! per line and rotates by size with a bounded number of retained files.
//!
//! # No new dependency
//!
//! CONTEXT D-01 preferred an external logger crate, but the build host's registry
//! is not reachable, so adding a dependency would break the build. The
//! implementation here uses only `log` and `serde_json` (both already
//! dependencies) and provides the same feature set: structured lines, level
//! filtering, size rotation, bounded retention.
//!
//! # Redaction
//!
//! [`sanitize`] removes control characters and truncates. Callers must not log
//! request parameters, PINs, keys, or decrypted payloads; a source-scan test
//! (`tests/log_redaction.rs`) enforces that discipline.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Rotate once the active file exceeds this many bytes.
pub const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
/// Number of rotated files kept in addition to the active one.
pub const KEEP_LOG_FILES: usize = 7;
/// Active log file name, written under `<install>/logs/`.
pub const LOG_FILE_NAME: &str = "skf-service.log";

/// Maximum length of a sanitized value.
const SANITIZE_MAX_CHARS: usize = 512;

/// Make a value safe to place in a log line.
///
/// Control characters (including newlines and NULs) are removed so a single log
/// record can never span lines, and the result is truncated.
pub fn sanitize(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| !c.is_control())
        .take(SANITIZE_MAX_CHARS)
        .collect();
    if value.chars().count() > SANITIZE_MAX_CHARS {
        format!("{}…", cleaned)
    } else {
        cleaned
    }
}

/// Initialize the foreground (console) logger.
///
/// Preserves the pre-Phase-6 behaviour: stderr, `RUST_LOG` (default `info`).
pub fn init_console() {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
}

/// Initialize the Windows-service file logger under `<dir>/logs/`.
///
/// Errors are returned rather than panicking; the caller logs a warning and keeps
/// the service running.
pub fn init_service_file(dir: &Path) -> std::io::Result<()> {
    let logs_dir = dir.join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    let logger =
        RotatingFileLogger::new(logs_dir.join(LOG_FILE_NAME), MAX_LOG_BYTES, KEEP_LOG_FILES)?;
    log::set_boxed_logger(Box::new(logger)).map_err(std::io::Error::other)?;
    log::set_max_level(level_from_env());
    Ok(())
}

/// The maximum level requested through `RUST_LOG`, defaulting to `Info`.
fn level_from_env() -> log::LevelFilter {
    match std::env::var("RUST_LOG") {
        Err(_) => log::LevelFilter::Info,
        Ok(raw) => {
            let lower = raw.to_ascii_lowercase();
            if lower.contains("trace") {
                log::LevelFilter::Trace
            } else if lower.contains("debug") {
                log::LevelFilter::Debug
            } else if lower.contains("warn") {
                log::LevelFilter::Warn
            } else if lower.contains("error") {
                log::LevelFilter::Error
            } else {
                log::LevelFilter::Info
            }
        }
    }
}

struct Inner {
    file: Option<File>,
    path: PathBuf,
    written: u64,
    keep: usize,
}

/// A `log::Log` that writes JSON lines and rotates by size.
pub struct RotatingFileLogger {
    inner: Mutex<Inner>,
    max_bytes: u64,
}

impl RotatingFileLogger {
    /// Open (or create) `path`, continuing the current file's size count.
    pub fn new(path: PathBuf, max_bytes: u64, keep: usize) -> std::io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            inner: Mutex::new(Inner {
                file: Some(file),
                path,
                written,
                keep,
            }),
            max_bytes,
        })
    }

    /// Path of the `i`-th rotated file (`skf-service.log.1`, …).
    fn numbered(path: &Path, i: usize) -> PathBuf {
        PathBuf::from(format!("{}.{}", path.display(), i))
    }

    /// Shift `base -> .1 -> .2 …`, dropping anything past `keep`, then reopen.
    fn rotate(inner: &mut Inner) {
        // Close the active file before renaming on Windows.
        inner.file = None;

        let _ = std::fs::remove_file(Self::numbered(&inner.path, inner.keep));
        for i in (1..inner.keep).rev() {
            let from = Self::numbered(&inner.path, i);
            let to = Self::numbered(&inner.path, i + 1);
            if from.exists() {
                let _ = std::fs::rename(&from, &to);
            }
        }
        if inner.path.exists() {
            let _ = std::fs::rename(&inner.path, Self::numbered(&inner.path, 1));
        }

        match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&inner.path)
        {
            Ok(file) => {
                inner.file = Some(file);
                inner.written = 0;
            }
            Err(_) => {
                inner.file = None;
            }
        }
    }

    /// Build the JSON line for a record. `serde_json` escapes newlines, so the
    /// result is always exactly one line.
    fn line(record: &log::Record<'_>) -> String {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let value = serde_json::json!({
            "ts": ts,
            "level": record.level().as_str(),
            "target": record.target(),
            "message": record.args().to_string(),
        });
        value.to_string()
    }
}

impl log::Log for RotatingFileLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        let line = Self::line(record);
        let mut inner = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if inner.written + line.len() as u64 + 1 > self.max_bytes {
            Self::rotate(&mut inner);
        }

        if let Some(file) = inner.file.as_mut() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.write_all(b"\n");
            let _ = file.flush();
            inner.written += line.len() as u64 + 1;
        }
    }

    fn flush(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(file) = inner.file.as_mut() {
                let _ = file.flush();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("skf-logging-test-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn read_all(path: &Path) -> Vec<String> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn json_line_has_expected_fields() {
        let dir = temp_dir("fields");
        let path = dir.join(LOG_FILE_NAME);
        let logger =
            RotatingFileLogger::new(path.clone(), MAX_LOG_BYTES, KEEP_LOG_FILES).expect("logger");
        log::Log::log(
            &logger,
            &log::Record::builder()
                .args(format_args!("hello"))
                .level(log::Level::Info)
                .target("test")
                .build(),
        );

        let lines = read_all(&path);
        assert_eq!(lines.len(), 1);
        let value: serde_json::Value = serde_json::from_str(&lines[0]).expect("valid JSON");
        assert!(value.get("ts").is_some());
        assert_eq!(value["level"], "INFO");
        assert_eq!(value["target"], "test");
        assert_eq!(value["message"], "hello");
    }

    #[test]
    fn rotation_creates_numbered_files_and_bounds_retention() {
        let dir = temp_dir("rotate");
        let path = dir.join(LOG_FILE_NAME);
        // Tiny max size so every few records rotates.
        let logger = RotatingFileLogger::new(path.clone(), 200, 3).expect("logger");
        for i in 0..80 {
            log::Log::log(
                &logger,
                &log::Record::builder()
                    .args(format_args!("message number {}", i))
                    .level(log::Level::Info)
                    .target("test")
                    .build(),
            );
        }

        assert!(path.exists(), "active file must exist");
        assert!(
            RotatingFileLogger::numbered(&path, 1).exists(),
            "at least one rotated file must exist"
        );
        let count = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("skf-service.log"))
            .count();
        assert!(
            count <= 4,
            "retention must keep at most KEEP+1 files, found {}",
            count
        );
    }

    #[test]
    fn message_with_newline_stays_one_line() {
        let dir = temp_dir("newline");
        let path = dir.join(LOG_FILE_NAME);
        let logger =
            RotatingFileLogger::new(path.clone(), MAX_LOG_BYTES, KEEP_LOG_FILES).expect("logger");
        log::Log::log(
            &logger,
            &log::Record::builder()
                .args(format_args!("a\nb\nc"))
                .level(log::Level::Info)
                .target("test")
                .build(),
        );

        let lines = read_all(&path);
        assert_eq!(lines.len(), 1, "a JSON message must not span lines");
        let value: serde_json::Value = serde_json::from_str(&lines[0]).expect("valid JSON");
        assert_eq!(value["message"], "a\nb\nc");
    }

    #[test]
    fn sanitize_removes_control_chars_and_truncates() {
        let cleaned = sanitize("start\nmiddle\u{0}end");
        assert!(!cleaned.contains('\n'));
        assert!(!cleaned.contains('\u{0}'));
        assert_eq!(cleaned, "startmiddleend");

        let long = "x".repeat(SANITIZE_MAX_CHARS + 50);
        let truncated = sanitize(&long);
        assert!(truncated.chars().count() <= SANITIZE_MAX_CHARS + 1);
        assert!(truncated.ends_with('…'));
    }
}
