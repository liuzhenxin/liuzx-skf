//! Local, non-sensitive diagnostics (OBS-03, OBS-04).
//!
//! An operator can run `skf-service diagnose` on any platform, whether or not the
//! service is running, to learn which startup stage succeeded. The report contains
//! only stage booleans, the provider alias, the WebSocket address, an error class,
//! and (when the service is running) the recorded state and uptime.
//!
//! It deliberately never prints the resolved library path, configuration values,
//! PINs, or key material. The library path is reduced to presence/load booleans so
//! a support transcript cannot leak the install layout.

use std::net::{SocketAddr, ToSocketAddrs};
use std::path::Path;

use crate::service_state;

/// The structured result of a diagnostic run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticReport {
    pub config_loaded: bool,
    pub config_error_class: Option<String>,
    pub provider_alias: String,
    pub provider_resolved: bool,
    pub provider_error_class: Option<String>,
    pub library_file_present: bool,
    pub library_loaded: bool,
    pub library_error_class: Option<String>,
    pub listener_bound: bool,
    pub listener_bindable: bool,
    pub ws_addr: String,
    pub state: Option<String>,
    pub stage: Option<String>,
    pub code: Option<u32>,
    pub address: Option<String>,
    pub uptime_seconds: Option<u64>,
}

fn config_error_class(message: &str) -> String {
    if message.starts_with("failed to read config") {
        "io".to_string()
    } else {
        "parse".to_string()
    }
}

/// Resolve `ws_addr` to a socket address, if it names one.
fn resolve_addr(addr: &str) -> Option<SocketAddr> {
    addr.to_socket_addrs().ok().and_then(|mut it| it.next())
}

/// Probe whether the configured listener address can currently be bound.
///
/// A non-loopback address is never probed (it would bind an externally reachable
/// socket as a side effect) and reports `false`.
fn listener_bindable(addr: &str) -> bool {
    match resolve_addr(addr) {
        Some(socket) if socket.ip().is_loopback() => std::net::TcpListener::bind(socket).is_ok(),
        _ => false,
    }
}

/// Run every diagnostic check and build the report.
pub fn run(config_path: &str, ws_addr: &str, state_file: Option<&Path>) -> DiagnosticReport {
    let mut report = DiagnosticReport {
        config_loaded: false,
        config_error_class: None,
        provider_alias: String::new(),
        provider_resolved: false,
        provider_error_class: None,
        library_file_present: false,
        library_loaded: false,
        library_error_class: None,
        listener_bound: false,
        listener_bindable: listener_bindable(ws_addr),
        ws_addr: ws_addr.to_string(),
        state: None,
        stage: None,
        code: None,
        address: None,
        uptime_seconds: None,
    };

    let config = match crate::config::load(config_path) {
        Ok(config) => {
            report.config_loaded = true;
            config
        }
        Err(e) => {
            report.config_error_class = Some(config_error_class(&e.to_string()));
            return report;
        }
    };

    report.provider_alias = config.default.clone();
    let resolved = match crate::config::resolve_lib_path(
        &config.libs,
        &config.default,
        std::env::consts::OS,
    ) {
        Ok(path) => {
            report.provider_resolved = true;
            path
        }
        Err(_) => {
            report.provider_error_class = Some("not_configured".to_string());
            return report;
        }
    };

    report.library_file_present = Path::new(&resolved).exists();
    // The resolved path is intentionally not recorded in the report.
    report.library_loaded =
        crate::provider::native::NativeSkfProvider::new(&config.default, &resolved).is_ok();
    if !report.library_loaded {
        report.library_error_class = Some("load_failed".to_string());
    }

    if let Some(path) = state_file {
        if let Some(file) = service_state::read(path) {
            report.listener_bound = file.state == service_state::ServiceState::Running;
            report.state = Some(
                match file.state {
                    service_state::ServiceState::StartPending => "start_pending",
                    service_state::ServiceState::Running => "running",
                    service_state::ServiceState::Failed => "failed",
                    service_state::ServiceState::Stopped => "stopped",
                }
                .to_string(),
            );
            report.stage = Some(file.stage.as_str().to_string());
            report.code = file.code;
            report.address = file.address.clone();
            if let Some(started) = file
                .started_at
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
            {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                report.uptime_seconds = Some(now.saturating_sub(started));
            }
        }
    }

    report
}

/// Render the report as human-readable lines.
pub fn render_text(report: &DiagnosticReport) -> String {
    let mut out = String::new();
    out.push_str(&format!("config_loaded:      {}\n", report.config_loaded));
    if let Some(class) = &report.config_error_class {
        out.push_str(&format!("config_error_class: {}\n", class));
    }
    out.push_str(&format!("provider_alias:     {}\n", report.provider_alias));
    out.push_str(&format!(
        "provider_resolved:  {}\n",
        report.provider_resolved
    ));
    if let Some(class) = &report.provider_error_class {
        out.push_str(&format!("provider_error:     {}\n", class));
    }
    out.push_str(&format!(
        "library_file:       {}\n",
        report.library_file_present
    ));
    out.push_str(&format!("library_loaded:     {}\n", report.library_loaded));
    if let Some(class) = &report.library_error_class {
        out.push_str(&format!("library_error:      {}\n", class));
    }
    out.push_str(&format!("listener_bound:     {}\n", report.listener_bound));
    out.push_str(&format!(
        "listener_bindable:  {}\n",
        report.listener_bindable
    ));
    out.push_str(&format!("ws_addr:            {}\n", report.ws_addr));
    if let Some(state) = &report.state {
        out.push_str(&format!("state:              {}\n", state));
    }
    if let Some(stage) = &report.stage {
        out.push_str(&format!("stage:              {}\n", stage));
    }
    if let Some(code) = report.code {
        out.push_str(&format!("code:               {}\n", code));
    }
    if let Some(address) = &report.address {
        out.push_str(&format!("address:            {}\n", address));
    }
    if let Some(uptime) = report.uptime_seconds {
        out.push_str(&format!("uptime_seconds:     {}\n", uptime));
    }
    out
}

/// Render the report as one JSON object.
pub fn render_json(report: &DiagnosticReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(name: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "skf-diagnostic-test-{}-{}.yaml",
            std::process::id(),
            name
        ));
        std::fs::write(&path, body).expect("write config");
        path
    }

    #[test]
    fn missing_config_reports_not_loaded() {
        let report = run("/definitely/not/a/config.yaml", "127.0.0.1:9001", None);
        assert!(!report.config_loaded);
        assert_eq!(report.config_error_class.as_deref(), Some("io"));
    }

    #[test]
    fn report_never_contains_the_library_path() {
        let secret = "/opt/secret/vendor/libgm3000.dylib";
        let path = temp_config(
            "leak",
            &format!(
                "default: GM3000\nvendor: {{}}\nGM3000:\n  {}: {}\n",
                std::env::consts::OS,
                secret
            ),
        );
        let report = run(path.to_str().expect("path"), "127.0.0.1:9001", None);
        let json = render_json(&report);
        assert!(!json.contains(secret), "diagnostic leaked the library path");
        assert!(report.provider_resolved);
        // The file does not exist, so presence is false and load is false.
        assert!(!report.library_file_present);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn render_json_parses_and_has_required_keys() {
        let report = run("/definitely/not/a/config.yaml", "127.0.0.1:9001", None);
        let value: serde_json::Value = serde_json::from_str(&render_json(&report)).expect("json");
        assert!(value.get("config_loaded").is_some());
        assert!(value.get("provider_alias").is_some());
        assert!(value.get("listener_bindable").is_some());
    }
}
