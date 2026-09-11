//! Server bootstrap: bind, serve, and the provider injection point.
//!
//! Split into three parts so the same code serves production and tests:
//!
//! | Function | Responsibility |
//! |----------|----------------|
//! | [`bind`] | Resolve configuration, construct the provider, bind listeners, report the **resolved** addresses |
//! | [`serve`] | Run the accept loop and the static HTTP demo until shutdown |
//! | [`run_server`] | `bind` + `serve`, reporting the bound address through a channel |
//!
//! The split exists because the pre-refactor `run_server` printed the
//! *requested* address and never exposed `listener.local_addr()`. Passing
//! `127.0.0.1:0` therefore produced an unconnectable test: the caller could not
//! learn which port the OS had chosen.

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tungstenite::protocol::WebSocketConfig;

/// Largest WebSocket message the service accepts.
///
/// Applied to both the assembled message and a single frame, so an oversized
/// payload is rejected by tungstenite while reading the frame rather than after
/// the service has materialised it. 1 MiB is ~2.5x the largest expected payload
/// (a 256 KiB operation payload base64-encodes to ~350 KiB).
pub const MAX_WS_MESSAGE_BYTES: usize = 1024 * 1024;

use crate::config::SkfConfig;
use crate::provider::{SkfProvider, native::NativeSkfProvider};

/// Whether the process runs attached to a console or under the Windows SCM.
///
/// The mode used to be inferred from `shutdown.is_some()`, which made the
/// default HTTP bind address depend on an unrelated parameter. Making it explicit
/// keeps the two defaults from drifting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Console,
    Service,
}

/// Everything `bind` needs, with environment overrides already applied.
#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Path to the YAML configuration file.
    pub config_path: String,
    /// `host:port` for the WebSocket listener.
    pub ws_addr: String,
    /// `host:port` for the static API demo, or `None` to skip the HTTP server.
    pub http_addr: Option<String>,
    /// Console or service.
    pub mode: RunMode,
}

impl ServerOptions {
    /// Build options from the environment, preserving the pre-refactor rules:
    ///
    /// * `SKF_CONFIG` overrides `config/skf.yaml`
    /// * `SKF_WS_ADDR` overrides `127.0.0.1:9001`
    /// * `SKF_HTTP_ADDR` overrides the mode default: `0.0.0.0:8000` for console,
    ///   `127.0.0.1:8000` for service (loopback avoids a firewall prompt on first
    ///   launch of an installed service)
    pub fn from_env(mode: RunMode) -> Self {
        let config_path =
            std::env::var("SKF_CONFIG").unwrap_or_else(|_| "config/skf.yaml".to_string());
        let ws_addr =
            std::env::var("SKF_WS_ADDR").unwrap_or_else(|_| "127.0.0.1:9001".to_string());
        let default_http = match mode {
            RunMode::Console => "0.0.0.0:8000",
            RunMode::Service => "127.0.0.1:8000",
        };
        let http_addr = Some(std::env::var("SKF_HTTP_ADDR").unwrap_or_else(|_| default_http.to_string()));

        Self {
            config_path,
            ws_addr,
            http_addr,
            mode,
        }
    }

    /// Options with no HTTP server and the given WebSocket address.
    ///
    /// Used by tests: an ephemeral WebSocket port avoids both a fixed-port
    /// collision and a stray static-file server during the suite.
    pub fn for_test(config_path: impl Into<String>) -> Self {
        Self {
            config_path: config_path.into(),
            ws_addr: "127.0.0.1:0".to_string(),
            http_addr: None,
            mode: RunMode::Console,
        }
    }
}

/// Addresses actually bound by [`bind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundServer {
    /// Resolved WebSocket address (`listener.local_addr()`), not the requested one.
    pub ws_addr: SocketAddr,
    /// Configured HTTP address, when an HTTP server was requested.
    pub http_addr: Option<SocketAddr>,
}

/// Builds the provider a server should use.
///
/// Injected rather than constructed inline so tests can substitute the fake
/// provider without touching production code paths.
pub trait ProviderFactory: Send + Sync + 'static {
    fn build(&self, config: &SkfConfig) -> Arc<dyn SkfProvider>;
}

/// The real factory: vendors the configured library for the default alias.
pub struct NativeProviderFactory;

impl ProviderFactory for NativeProviderFactory {
    fn build(&self, config: &SkfConfig) -> Arc<dyn SkfProvider> {
        match NativeSkfProvider::from_config(config, &config.default) {
            Ok(provider) => Arc::new(provider),
            Err(err) => {
                // Do not panic: the loading failure is surfaced per request, exactly
                // as the pre-refactor `Load Lib Failed` path did, so a missing
                // library degrades to an error response instead of a dead service.
                log::error!(
                    "default provider '{}' could not be prepared: {}",
                    config.default,
                    err
                );
                Arc::new(UnavailableProvider::new(
                    config.default.clone(),
                    err.to_string(),
                ))
            }
        }
    }
}

/// Provider that reports the same failure for every operation.
///
/// Created when the configured default provider cannot be prepared at startup.
/// It keeps the service answering with the established `Load Lib Failed`
/// contract rather than exiting.
pub struct UnavailableProvider {
    alias: String,
    detail: String,
}

impl UnavailableProvider {
    pub fn new(alias: String, detail: String) -> Self {
        Self { alias, detail }
    }

    fn error(&self) -> crate::provider::ProviderError {
        crate::provider::ProviderError::LibraryLoadFailed {
            path: self.detail.clone(),
            arch: std::env::consts::ARCH,
            detail: self.detail.clone(),
        }
    }
}

impl SkfProvider for UnavailableProvider {
    fn alias(&self) -> &str {
        &self.alias
    }

    fn enum_devices(&self, _present_only: bool) -> crate::provider::ProviderResult<Vec<String>> {
        Err(self.error())
    }

    fn device_state(&self, _name: &str) -> crate::provider::ProviderResult<u32> {
        Err(self.error())
    }

    fn wait_for_event(
        &self,
        _buf_len: usize,
    ) -> crate::provider::ProviderResult<(String, u32)> {
        Err(self.error())
    }

    fn cancel_wait_for_event(&self) -> crate::provider::ProviderResult<()> {
        Err(self.error())
    }

    fn open_device(
        &self,
        _name: &str,
    ) -> crate::provider::ProviderResult<Box<dyn crate::provider::DeviceGuard>> {
        Err(self.error())
    }
}

/// Configuration plus provider produced by [`bind`].
pub struct PreparedServer {
    pub config: SkfConfig,
    pub provider: Arc<dyn SkfProvider>,
}

/// One client connection.
///
/// The library owns transport but not request semantics, so the dispatcher stays
/// in the binary crate (decision D-06) and is reached through this seam. It also
/// gives Phase 2 the per-connection object its session state will live on.
pub trait Session: Send {
    /// Handle one request frame and return the response frame.
    fn handle<'a>(&'a mut self, text: &'a str) -> BoxFuture<'a, String>;
}

/// Creates a [`Session`] for each accepted connection.
pub trait SessionFactory: Send + Sync + 'static {
    fn create(&self) -> Box<dyn Session>;
}

impl SessionFactory for Box<dyn SessionFactory> {
    fn create(&self) -> Box<dyn Session> {
        (**self).create()
    }
}

/// Builds a session factory from the loaded configuration and provider.
///
/// A plain function pointer rather than a generic so it can be stored in a
/// process-wide slot: the Windows SCM entry point is a C-style callback declared
/// by `define_windows_service!` and cannot receive user data, so the binary
/// installs its builder at startup instead of threading it through.
pub type SessionBuilder =
    fn(&SkfConfig, &Arc<dyn SkfProvider>) -> Box<dyn SessionFactory>;

static SESSION_BUILDER: std::sync::OnceLock<SessionBuilder> = std::sync::OnceLock::new();

/// Register the binary's session builder. Idempotent per process.
pub fn install_session_builder(builder: SessionBuilder) {
    let _ = SESSION_BUILDER.set(builder);
}

/// The registered session builder, when the binary has installed one.
pub fn installed_session_builder() -> Option<SessionBuilder> {
    SESSION_BUILDER.get().copied()
}

/// Resolve configuration, build the provider, and bind the listeners.
///
/// Returns the resolved addresses together with the WebSocket listener. The HTTP
/// listener is not returned because warp owns its socket; when `http_addr` is
/// `None` no HTTP server is started at all.
pub async fn bind<F>(
    opts: &ServerOptions,
    factory: &F,
) -> anyhow::Result<(BoundServer, TcpListener, PreparedServer)>
where
    F: ProviderFactory,
{
    let config = crate::config::load(&opts.config_path)?;
    let provider = factory.build(&config);

    let http_addr = match &opts.http_addr {
        Some(addr) => Some(
            addr.parse::<SocketAddr>()
                .map_err(|e| anyhow::anyhow!("invalid HTTP address '{}': {}", addr, e))?,
        ),
        None => None,
    };

    let listener = TcpListener::bind(&opts.ws_addr).await.map_err(|e| {
        anyhow::anyhow!(
            "failed to bind WebSocket listener on {}: {}",
            opts.ws_addr,
            e
        )
    })?;
    // The resolved address, not the requested one: with port 0 the OS chooses.
    let bound_ws = listener
        .local_addr()
        .map_err(|e| anyhow::anyhow!("failed to read bound WebSocket address: {}", e))?;

    Ok((
        BoundServer {
            ws_addr: bound_ws,
            http_addr,
        },
        listener,
        PreparedServer { config, provider },
    ))
}

/// Serve until `shutdown` flips (service mode) or forever (console mode).
pub async fn serve<S>(
    listener: TcpListener,
    prepared: PreparedServer,
    bound: BoundServer,
    sessions: S,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
) -> anyhow::Result<()>
where
    S: SessionFactory,
{
    if let Some(http_addr) = bound.http_addr {
        let api_dir = std::env::var("SKF_API_DIR").unwrap_or_else(|_| "api".to_string());
        tokio::spawn(async move {
            let api_route = warp::fs::dir(api_dir);
            println!(
                "HTTP Server serving 'api' directory on http://{}",
                http_addr
            );
            warp::serve(api_route).run(http_addr).await;
        });
    }

    println!("SKF Service listening on ws://{}", bound.ws_addr);

    // The provider is built by `bind` and handed to the session factory by the
    // caller; `serve` only needs it to keep it alive for the process lifetime.
    let _keep_provider = Arc::clone(&prepared.provider);
    let sessions = Arc::new(sessions);

    match shutdown {
        Some(mut stop) => {
            loop {
                tokio::select! {
                    changed = stop.changed() => {
                        match changed {
                            Ok(()) => log::info!("service stop requested, shutting down"),
                            Err(_) => log::info!("service stop channel closed, shutting down"),
                        }
                        break;
                    }
                    accepted = listener.accept() => {
                        match accepted {
                            Ok((stream, _)) => spawn_client(Arc::clone(&sessions), stream),
                            Err(e) => {
                                log::error!("WebSocket accept error: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        }
        None => {
            while let Ok((stream, _)) = listener.accept().await {
                spawn_client(Arc::clone(&sessions), stream);
            }
        }
    }
    Ok(())
}

/// Bind, then serve, reporting the bound address before entering the loop.
///
/// The report arrives through `bound_tx` because `serve` only returns when the
/// server stops, whereas callers (and tests) need the address immediately.
pub async fn run_server<F>(
    opts: ServerOptions,
    provider_factory: F,
    shutdown: Option<tokio::sync::watch::Receiver<bool>>,
    bound_tx: Option<tokio::sync::oneshot::Sender<BoundServer>>,
) -> anyhow::Result<()>
where
    F: ProviderFactory,
{
    // Services start with the working directory set to %SystemRoot%\System32.
    // Fall back to the executable's directory so `config/`, `api/`, and relative
    // `native/` paths from the YAML still resolve.
    if !std::path::Path::new(&opts.config_path).exists() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                log::info!("changing working directory to {}", dir.display());
                let _ = std::env::set_current_dir(dir);
            }
        }
    }

    let (bound, listener, prepared) = bind(&opts, &provider_factory).await?;
    if let Some(tx) = bound_tx {
        let _ = tx.send(bound);
    }
    let builder = installed_session_builder().ok_or_else(|| {
        anyhow::anyhow!(
            "no session builder installed; call install_session_builder() during startup"
        )
    })?;
    let sessions = builder(&prepared.config, &prepared.provider);
    serve(listener, prepared, bound, sessions, shutdown).await
}

/// Upgrade one accepted connection and drive its request loop.
///
/// Transport concerns only: the frame is handed to the session, and the session's
/// reply is written back. Failures are logged rather than propagated because one
/// misbehaving client must not stop the accept loop.
fn spawn_client<S>(sessions: Arc<S>, stream: tokio::net::TcpStream)
where
    S: SessionFactory,
{
    tokio::spawn(async move {
        let ws_config = WebSocketConfig {
            max_message_size: Some(MAX_WS_MESSAGE_BYTES),
            max_frame_size: Some(MAX_WS_MESSAGE_BYTES),
            ..Default::default()
        };
        let mut ws = match tokio_tungstenite::accept_async_with_config(stream, Some(ws_config)).await {
            Ok(ws) => ws,
            Err(e) => {
                log::debug!("websocket upgrade failed: {}", e);
                return;
            }
        };
        let mut session = sessions.create();
        while let Some(msg) = ws.next().await {
            match msg {
                Ok(msg) if msg.is_text() => {
                    let text = match msg.to_text() {
                        Ok(text) => text,
                        Err(e) => {
                            log::debug!("ignoring non-utf8 text frame: {}", e);
                            continue;
                        }
                    };
                    let reply = session.handle(text).await;
                    if let Err(e) = ws.send(tungstenite::Message::Text(reply)).await {
                        log::debug!("failed to send response: {}", e);
                        break;
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    log::debug!("websocket receive error: {}", e);
                    break;
                }
            }
        }
    });
}
