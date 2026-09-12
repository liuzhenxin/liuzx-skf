//! Windows Service (SCM) integration for skf-service.
//!
//! This module is only compiled on Windows targets. It turns the plain TCP
//! gateway into a first-class Windows service so that the service:
//!
//! * starts automatically when Windows boots (`ServiceStartType::AutoStart`),
//! * is managed through the Service Control Manager (`services.msc`,
//!   `sc.exe`, `net start/stop`) instead of PID files or scheduled tasks,
//! * starts immediately after `install`,
//! * stops cleanly when the SCM sends STOP / SHUTDOWN.
//!
//! The same binary provides three roles:
//!
//! | invocation                     | role                                        |
//! |--------------------------------|---------------------------------------------|
//! | `skf-service.exe install`      | register + start the SCM service (AutoStart)|
//! | `skf-service.exe uninstall`    | stop + remove the SCM service               |
//! | `skf-service.exe start/stop/status` | service management helpers             |
//! | `skf-service.exe --service`    | entry point used by the SCM (no console)    |
//! | `skf-service.exe` (no args)    | foreground console mode (development)       |
//!
//! The SCM entry point (`service_dispatcher::start`) must be called from the
//! main thread, which is why `run()` is invoked directly from `main()`.

use std::ffi::OsString;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context as _, Result};
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
        ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
};
use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};

use crate::service_state::{self, ServiceState as FileState, ServiceStatusFile, StartupStage};

pub const SERVICE_NAME: &str = "LiuZXSKFService";
pub const SERVICE_DISPLAY_NAME: &str = "LiuZX SKF Service";
const SERVICE_DESCRIPTION: &str =
    "Unified SKF (USB Key / HSM) JSON-RPC gateway. WebSocket ws://127.0.0.1:9001, HTTP demo http://127.0.0.1:8000";
const WAIT_DELETE_TIMEOUT: Duration = Duration::from_secs(8);

// Macro generating the low-level `extern "system"` service entry function that
// the SCM invokes. It converts the raw arguments and calls `service_main`.
define_windows_service!(ffi_service_main, service_main);

/// `install` – register the service with AutoStart and start it right away.
pub fn install() -> Result<()> {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(|e| anyhow!("unable to connect to the Service Control Manager: {e}"))?;

    let exe = std::env::current_exe().context("unable to locate skf-service.exe")?;

    // Make install idempotent: drop any previous registration first.
    delete_if_present(&manager)?;

    let service_info = ServiceInfo {
        name: OsString::from(SERVICE_NAME),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        // Boot-time auto start: service comes up together with Windows.
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.clone(),
        // The SCM launches the executable with this argument; see main().
        launch_arguments: vec![OsString::from("--service")],
        dependencies: Vec::new(),
        account_name: None, // LocalSystem
        account_password: None,
    };

    let handle = manager
        .create_service(&service_info, ServiceAccess::CHANGE_CONFIG)
        .map_err(|e| anyhow!("failed to register service '{}': {e}", SERVICE_NAME))?;
    if let Err(e) = handle.set_description(OsString::from(SERVICE_DESCRIPTION)) {
        log::warn!("unable to set service description: {e}");
    }
    drop(handle);

    // Start immediately after install so no reboot is required.
    let svc = manager
        .open_service(SERVICE_NAME, ServiceAccess::START)
        .map_err(|e| anyhow!("service registered but could not be opened for start: {e}"))?;
    svc.start(&[] as &[&str])
        .map_err(|e| anyhow!("service registered but failed to start: {e}"))?;

    println!(
        "Service '{}' installed (AutoStart) and started. Manage with: sc query {}",
        SERVICE_NAME, SERVICE_NAME
    );
    Ok(())
}

/// `uninstall` – stop and remove the service.
pub fn uninstall() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| anyhow!("unable to connect to the Service Control Manager: {e}"))?;

    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let svc = manager
        .open_service(SERVICE_NAME, access)
        .map_err(|_| anyhow!("service '{}' is not installed", SERVICE_NAME))?;

    if svc.query_status()?.current_state != ServiceState::Stopped {
        let _ = svc.stop();
    }
    let _ = svc.delete();
    drop(svc);

    wait_until_removed(&manager, WAIT_DELETE_TIMEOUT)?;
    println!("Service '{}' removed.", SERVICE_NAME);
    Ok(())
}

/// `start` – start the service (helper, equivalent to `net start`).
pub fn start() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| anyhow!("unable to connect to the Service Control Manager: {e}"))?;
    let svc = manager
        .open_service(SERVICE_NAME, ServiceAccess::START)
        .map_err(|_| anyhow!("service '{}' is not installed", SERVICE_NAME))?;
    svc.start(&[] as &[&str])
        .map_err(|e| anyhow!("failed to start '{}': {e}", SERVICE_NAME))?;
    println!("Service '{}' started.", SERVICE_NAME);
    Ok(())
}

/// `stop` – stop the service (helper, equivalent to `net stop`).
pub fn stop() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| anyhow!("unable to connect to the Service Control Manager: {e}"))?;
    let svc = manager
        .open_service(
            SERVICE_NAME,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP,
        )
        .map_err(|_| anyhow!("service '{}' is not installed", SERVICE_NAME))?;
    if svc.query_status()?.current_state != ServiceState::Stopped {
        svc.stop()
            .map_err(|e| anyhow!("failed to stop '{}': {e}", SERVICE_NAME))?;
        println!("Service '{}' stopped.", SERVICE_NAME);
    } else {
        println!("Service '{}' is already stopped.", SERVICE_NAME);
    }
    Ok(())
}

/// `status` – print the service state and executable path.
pub fn status() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| anyhow!("unable to connect to the Service Control Manager: {e}"))?;

    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG;
    match manager.open_service(SERVICE_NAME, access) {
        Ok(svc) => {
            let st = svc
                .query_status()
                .context("unable to query service status")?;
            // Prefer the startup status file: it distinguishes "the process is
            // alive" from "the service is usable" by naming the phase
            // (SVC-05). Absent file falls back to the SCM-only output below.
            if let Some(file) = service_state::read(&state_file_path()) {
                match file.state {
                    FileState::Running => println!(
                        "Startup: Running (stage={}, address={})",
                        file.stage.as_str(),
                        file.address.as_deref().unwrap_or("unknown")
                    ),
                    FileState::Failed => println!(
                        "Startup: Failed (stage={}, code={})",
                        file.stage.as_str(),
                        file.code.unwrap_or(0)
                    ),
                    FileState::StartPending => {
                        println!("Startup: StartPending (stage={})", file.stage.as_str())
                    }
                    FileState::Stopped => println!("Startup: Stopped"),
                }
                if let Some(reason) = file.reason.as_deref() {
                    println!("Reason: {}", reason);
                }
            }

            let state = match st.current_state {
                ServiceState::Stopped => "Stopped",
                ServiceState::StartPending => "StartPending",
                ServiceState::StopPending => "StopPending",
                ServiceState::Running => "Running",
                ServiceState::ContinuePending => "ContinuePending",
                ServiceState::PausePending => "PausePending",
                ServiceState::Paused => "Paused",
            };
            println!(
                "Service '{}': {} (pid={})",
                SERVICE_NAME,
                state,
                st.process_id.unwrap_or(0)
            );
            match svc.query_config() {
                Ok(cfg) => println!("Executable: {}", cfg.executable_path.display()),
                Err(_) => {}
            }
            Ok(())
        }
        Err(_) => {
            println!("Service '{}' is not installed.", SERVICE_NAME);
            Ok(())
        }
    }
}

/// `--service` entry point called from `main()` on the main thread.
///
/// stdout/stderr are redirected to `skf-service.log` next to the executable
/// (a service process has no console), then the SCM dispatcher is started.
/// `service_dispatcher::start` blocks until the service stops.
pub fn run() -> Result<()> {
    redirect_stdio_to_log();
    // Structured, rotating logs under <exe dir>/logs. A failure here must not stop
    // the service; the console log still catches panics and println!.
    if let Err(e) = crate::logging::init_service_file(&exe_dir()) {
        eprintln!("[skf-service] structured logging unavailable: {e}");
    }

    match service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
        Ok(()) => Ok(()),
        Err(e) => Err(anyhow!(
            "cannot run as a Windows service: {e}. \
             If you started this process manually, omit '--service' to run in foreground mode."
        )),
    }
}

// ---------------------------------------------------------------------------
// Service worker
// ---------------------------------------------------------------------------

/// Called by the generated dispatcher on a background thread once the SCM has
/// launched the process. No console is available on this thread.
fn service_main(_arguments: Vec<OsString>) {
    if let Err(e) = run_service() {
        // The process state has already been reported to the SCM as Stopped by
        // run_service(); log the failure so it lands in skf-service.log.
        log::error!("service main error: {e:#}");
    }
}

fn run_service() -> Result<()> {
    // Watch channel used to signal the async server that it must shut down.
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);

    let event_handler = move |control_event: ServiceControl| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                log::info!("received {control_event:?} control event");
                let _ = stop_tx.send(true);
                ServiceControlHandlerResult::NoError
            }
            // All services must answer Interrogate even when it is a no-op.
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)
        .map_err(|e| anyhow!("unable to register service control handler: {e}"))?;

    let pid = std::process::id();
    let state_path = state_file_path();

    // Report the current phase to the SCM. StartPending uses an increasing
    // checkpoint so the SCM knows initialization is progressing; Running is only
    // reported once the listener is bound (SVC-01).
    let report = |state: ServiceState, checkpoint: u32, exit: ServiceExitCode| {
        let controls = if state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        };
        let _ = status_handle.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: controls,
            exit_code: exit,
            checkpoint,
            wait_hint: Duration::from_secs(10),
            process_id: None,
        });
    };

    report(ServiceState::StartPending, 0, ServiceExitCode::NO_ERROR);
    let _ = service_state::write_atomic(
        &state_path,
        &ServiceStatusFile::pending(StartupStage::Config, Some(pid), now_seconds()),
    );

    let checkpoint = std::cell::Cell::new(0u32);
    let opts = crate::server::ServerOptions::from_env(crate::server::RunMode::Service);
    crate::server::prepare_working_directory(&opts.config_path);

    // The phase callback writes the status file and advances the SCM checkpoint.
    let on_stage = |stage: StartupStage| {
        checkpoint.set(checkpoint.get() + 1);
        let _ = service_state::write_atomic(
            &state_path,
            &ServiceStatusFile::pending(stage, Some(pid), now_seconds()),
        );
        report(
            ServiceState::StartPending,
            checkpoint.get(),
            ServiceExitCode::NO_ERROR,
        );
    };

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| anyhow!("unable to create Tokio runtime: {e}"))?;

    let server_result: Result<()> = rt.block_on(async {
        let (bound, listener, prepared) = match crate::server::bind_with_progress(
            &opts,
            &crate::server::NativeProviderFactory,
            on_stage,
        )
        .await
        {
            Ok(value) => value,
            Err(e) => {
                // A distinct non-zero exit code makes the installer's
                // restart-on-failure action fire (SVC-02).
                let code = e.exit_code();
                let stage = e.stage();
                let _ = service_state::write_atomic(
                    &state_path,
                    &ServiceStatusFile::failed(
                        stage,
                        code,
                        short_reason(&e.to_string()),
                        Some(pid),
                        now_seconds(),
                    ),
                );
                report(
                    ServiceState::Stopped,
                    0,
                    ServiceExitCode::ServiceSpecific(code),
                );
                return Err(anyhow!(e.to_string()));
            }
        };

        let address = bound.ws_addr.to_string();
        let _ = service_state::write_atomic(
            &state_path,
            &ServiceStatusFile::running(&address, Some(pid), now_seconds()),
        );
        report(ServiceState::Running, 0, ServiceExitCode::NO_ERROR);
        log::info!(
            "skf-service running as Windows service '{}' on {}",
            SERVICE_NAME,
            address
        );

        let builder = crate::server::installed_session_builder()
            .ok_or_else(|| anyhow!("no session builder installed"))?;
        let sessions = builder(&prepared.config, &prepared.provider);

        match crate::server::serve(listener, prepared, bound, sessions, Some(stop_rx)).await {
            Ok(()) => {
                let _ = service_state::write_atomic(
                    &state_path,
                    &ServiceStatusFile::stopped(now_seconds()),
                );
                report(ServiceState::Stopped, 0, ServiceExitCode::NO_ERROR);
                Ok(())
            }
            Err(e) => {
                let _ = service_state::write_atomic(
                    &state_path,
                    &ServiceStatusFile::failed(
                        StartupStage::Serve,
                        service_state::EXIT_SERVE,
                        short_reason(&e.to_string()),
                        Some(pid),
                        now_seconds(),
                    ),
                );
                report(
                    ServiceState::Stopped,
                    0,
                    ServiceExitCode::ServiceSpecific(service_state::EXIT_SERVE),
                );
                Err(e)
            }
        }
    });
    drop(rt);

    server_result
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Path of the startup-status file written next to the executable.
fn state_file_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("service-state.json")
}

/// Timestamp recorded in the status file (seconds since the Unix epoch).
fn now_seconds() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// Keep a failure reason short and free of configuration values.
fn short_reason(message: &str) -> String {
    message
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .chars()
        .take(200)
        .collect()
}

/// Directory containing the executable (the service's working root).
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Where stdout/stderr are redirected. Separate from the structured
/// `logs/skf-service.log` so the two writers never interleave.
fn console_log_path() -> PathBuf {
    exe_dir().join("skf-service.console.log")
}

/// Point stdout/stderr at an append-only log file next to the executable.
/// The File is intentionally leaked (`mem::forget`) because the process keeps
/// writing to those standard handles until it exits.
fn redirect_stdio_to_log() {
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(console_log_path())
    {
        let raw = file.as_raw_handle();
        unsafe {
            SetStdHandle(STD_OUTPUT_HANDLE, raw);
            SetStdHandle(STD_ERROR_HANDLE, raw);
        }
        std::mem::forget(file);
        println!(
            "[skf-service] stdout/stderr redirected to {}",
            console_log_path().display()
        );
    }
}

/// Stop (if running) and delete an existing service registration. Used by
/// `install` so that re-running the installer does not fail.
fn delete_if_present(manager: &ServiceManager) -> Result<()> {
    let access = ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE;
    let svc = match manager.open_service(SERVICE_NAME, access) {
        Ok(svc) => svc,
        Err(_) => return Ok(()), // not installed yet
    };

    let already_stopped = svc
        .query_status()
        .map(|s| s.current_state == ServiceState::Stopped)
        .unwrap_or(false);
    if !already_stopped {
        let _ = svc.stop();
    }
    let _ = svc.delete();
    drop(svc);
    wait_until_removed(manager, WAIT_DELETE_TIMEOUT)
}

/// Poll until the service disappears from the SCM database.
fn wait_until_removed(manager: &ServiceManager, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if manager
            .open_service(SERVICE_NAME, ServiceAccess::QUERY_STATUS)
            .is_err()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Ok(())
}
