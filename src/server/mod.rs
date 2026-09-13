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

use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;
use tungstenite::protocol::WebSocketConfig;

/// Largest WebSocket message the service accepts.
///
/// Applied to both the assembled message and a single frame, so an oversized
/// payload is rejected by tungstenite while reading the frame rather than after
/// the service has materialised it. 1 MiB is ~2.5x the largest expected payload
/// (a 256 KiB operation payload base64-encodes to ~350 KiB).
pub const MAX_WS_MESSAGE_BYTES: usize = 1024 * 1024;

/// Largest number of concurrent WebSocket connections the service serves.
///
/// A connection that cannot acquire a permit is refused before the WebSocket
/// handshake rather than queued, so a connection storm cannot grow the task set
/// without bound (TRANS-02, D-14).
pub const MAX_CONNECTIONS: usize = 64;

use crate::config::SkfConfig;
use crate::provider::{native::NativeSkfProvider, SkfProvider};

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
        let ws_addr = std::env::var("SKF_WS_ADDR").unwrap_or_else(|_| "127.0.0.1:9001".to_string());
        let default_http = match mode {
            RunMode::Console => "0.0.0.0:8000",
            RunMode::Service => "127.0.0.1:8000",
        };
        let http_addr =
            Some(std::env::var("SKF_HTTP_ADDR").unwrap_or_else(|_| default_http.to_string()));

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

    fn wait_for_event(&self, _buf_len: usize) -> crate::provider::ProviderResult<(String, u32)> {
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
    /// Server TLS acceptor, or `None` when the listener is plaintext.
    pub tls: Option<tokio_rustls::TlsAcceptor>,
}

/// Read and parse the PEM certificate chain and private key.
///
/// Error strings name a class only: a log line or support transcript must never
/// reveal the install layout or the key location (TLS-04).
#[allow(clippy::type_complexity)]
fn load_tls_material(
    cert_path: &str,
    key_path: &str,
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), String> {
    let cert_file = std::fs::File::open(cert_path)
        .map_err(|_| "TLS certificate could not be read".to_string())?;
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut BufReader::new(cert_file))
        .collect::<Result<_, _>>()
        .map_err(|_| "TLS certificate could not be parsed".to_string())?;
    if certs.is_empty() {
        return Err("TLS certificate file contained no certificate".to_string());
    }

    let key_file = std::fs::File::open(key_path)
        .map_err(|_| "TLS private key could not be read".to_string())?;
    let key = rustls_pemfile::private_key(&mut BufReader::new(key_file))
        .map_err(|_| "TLS private key could not be parsed".to_string())?
        .ok_or_else(|| "TLS private key was not found in the configured file".to_string())?;

    Ok((certs, key))
}

/// Build the server TLS acceptor for the configured material.
///
/// `Ok(None)` means TLS is disabled. Any failure is a fatal configuration error:
/// the service must never fall back to plaintext when TLS was requested
/// (CONTEXT D-04..D-06).
fn build_tls_acceptor(config: &SkfConfig) -> Result<Option<tokio_rustls::TlsAcceptor>, String> {
    config.validate_tls()?;
    let (cert_path, key_path) = match (config.tls_cert_path(), config.tls_key_path()) {
        (Some(cert), Some(key)) => (cert, key),
        _ => return Ok(None),
    };
    let (certs, key) = load_tls_material(&cert_path, &key_path)?;

    // Pin the `ring` provider explicitly: the crate-feature default
    // (`aws-lc-rs`) breaks the i686-pc-windows-gnu cross-build (research STACK.md).
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let tls_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| "TLS configuration error".to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|_| "TLS certificate and private key could not be used together".to_string())?;

    Ok(Some(tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(
        tls_config,
    ))))
}

/// Whether the configured certificate and key can both be loaded.
///
/// Exposed for `diagnose`, which reports the boolean without exposing the path.
pub fn tls_cert_loads(config: &SkfConfig) -> bool {
    match (config.tls_cert_path(), config.tls_key_path()) {
        (Some(cert), Some(key)) => load_tls_material(&cert, &key).is_ok(),
        _ => false,
    }
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
pub type SessionBuilder = fn(&SkfConfig, &Arc<dyn SkfProvider>) -> Box<dyn SessionFactory>;

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
    bind_with_progress(opts, factory, |_| {})
        .await
        .map_err(anyhow::Error::from)
}

/// A startup failure that maps to a distinct service exit code.
///
/// The Windows service reports `ServiceSpecific(exit_code())` so `sc failure`
/// restart actions fire and an operator can tell which phase failed.
#[derive(Debug)]
pub enum StartupError {
    /// Configuration could not be read or parsed.
    Config(String),
    /// The default provider has no usable path for this OS.
    Provider(String),
    /// The WebSocket listener could not be bound.
    Bind(String),
}

impl StartupError {
    /// The phase this failure belongs to.
    pub fn stage(&self) -> crate::service_state::StartupStage {
        use crate::service_state::StartupStage;
        match self {
            StartupError::Config(_) => StartupStage::Config,
            StartupError::Provider(_) => StartupStage::Provider,
            StartupError::Bind(_) => StartupStage::Bind,
        }
    }

    /// The service-specific exit code for this failure.
    pub fn exit_code(&self) -> u32 {
        self.stage().exit_code()
    }
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartupError::Config(detail) => write!(f, "configuration error: {}", detail),
            StartupError::Provider(detail) => write!(f, "provider resolution error: {}", detail),
            StartupError::Bind(detail) => write!(f, "bind error: {}", detail),
        }
    }
}

impl std::error::Error for StartupError {}

/// Like [`bind`], but reports each startup phase and classifies failures.
///
/// The Windows service uses this to report `StartPending` per phase and to fail
/// with a distinct exit code. The classification boundary follows D-08/D-09:
/// a **missing path for this OS** is fatal, but a **configured path whose library
/// is missing or fails to load** is not — the factory returns
/// `UnavailableProvider` and the service still starts (Linux CI depends on this).
pub async fn bind_with_progress<F>(
    opts: &ServerOptions,
    factory: &F,
    on_stage: impl Fn(crate::service_state::StartupStage),
) -> Result<(BoundServer, TcpListener, PreparedServer), StartupError>
where
    F: ProviderFactory,
{
    use crate::service_state::StartupStage;

    on_stage(StartupStage::Config);
    let config =
        crate::config::load(&opts.config_path).map_err(|e| StartupError::Config(e.to_string()))?;
    // A bad or half-configured TLS block is a configuration error before any
    // listener exists; there is no plaintext fallback (D-04..D-06).
    let tls = build_tls_acceptor(&config).map_err(StartupError::Config)?;
    log::info!("tls={}", if tls.is_some() { "enabled" } else { "disabled" });

    on_stage(StartupStage::Provider);
    // Fatal only when the default provider has no path for this OS. The file may
    // still be missing; that degrades to `UnavailableProvider` (D-09).
    crate::config::resolve_lib_path(&config.libs, &config.default, std::env::consts::OS)
        .map_err(|e| StartupError::Provider(e.to_string()))?;
    let provider = factory.build(&config);

    let http_addr = match &opts.http_addr {
        Some(addr) => Some(addr.parse::<SocketAddr>().map_err(|e| {
            StartupError::Config(format!("invalid HTTP address '{}': {}", addr, e))
        })?),
        None => None,
    };

    on_stage(StartupStage::Bind);
    // Refuse a non-loopback WebSocket bind before the socket is opened unless the
    // operator explicitly opted in (TRANS-06). The HTTP demo keeps its defaults.
    let ws_socket =
        resolve_bind_addr(&opts.ws_addr).map_err(|e| StartupError::Bind(e.to_string()))?;
    if !ws_socket.ip().is_loopback() && !config.allows_remote() {
        return Err(StartupError::Bind(format!(
            "refusing to bind WebSocket listener to non-loopback address {} without explicit opt-in; set allow_remote: true in the config or SKF_ALLOW_REMOTE=1",
            ws_socket
        )));
    }

    let listener = TcpListener::bind(ws_socket).await.map_err(|e| {
        StartupError::Bind(format!(
            "failed to bind WebSocket listener on {}: {}",
            ws_socket, e
        ))
    })?;
    // The resolved address, not the requested one: with port 0 the OS chooses.
    let bound_ws = listener.local_addr().map_err(|e| {
        StartupError::Bind(format!("failed to read bound WebSocket address: {}", e))
    })?;

    Ok((
        BoundServer {
            ws_addr: bound_ws,
            http_addr,
        },
        listener,
        PreparedServer {
            config,
            provider,
            tls,
        },
    ))
}

/// Point the process at the executable's directory when the configured path is
/// not present.
///
/// Services start with the working directory set to `%SystemRoot%\System32`.
/// Falling back to the executable's directory keeps `config/`, `api/`, and
/// relative `native/` paths from the YAML resolvable. Exposed so the Windows
/// service can reuse it when it orchestrates `bind_with_progress` + `serve`
/// itself rather than calling `run_server`.
pub fn prepare_working_directory(config_path: &str) {
    if !std::path::Path::new(config_path).exists() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                log::info!("changing working directory to {}", dir.display());
                let _ = std::env::set_current_dir(dir);
            }
        }
    }
}

/// Resolve a `host:port` string to the first socket address it names.
///
/// Resolving before binding is what lets the loopback gate reason about the
/// address that will actually be used rather than about a literal string.
fn resolve_bind_addr(addr: &str) -> anyhow::Result<SocketAddr> {
    use std::net::ToSocketAddrs;
    addr.to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("invalid WS address '{}': {}", addr, e))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("WS address '{}' resolved to no address", addr))
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
    let tls = prepared.tls.clone();
    let sessions = Arc::new(sessions);
    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

    match shutdown {
        Some(mut stop) => loop {
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
                        Ok((stream, peer)) => {
                            admit(Arc::clone(&sessions), Arc::clone(&permits), tls.clone(), stream, peer)
                        }
                        Err(e) => {
                            log::error!("WebSocket accept error: {}", e);
                            break;
                        }
                    }
                }
            }
        },
        None => {
            while let Ok((stream, peer)) = listener.accept().await {
                admit(
                    Arc::clone(&sessions),
                    Arc::clone(&permits),
                    tls.clone(),
                    stream,
                    peer,
                );
            }
        }
    }
    Ok(())
}

/// Admit one accepted TCP connection if a slot is free, otherwise refuse it.
///
/// Refusing means dropping the socket before the WebSocket handshake, so an
/// over-limit client never reaches the session factory or the vendor library.
/// The semaphore permit is acquired here and held across the (optional) TLS
/// handshake, so a handshake flood cannot bypass `MAX_CONNECTIONS` (D-11).
fn admit<S>(
    sessions: Arc<S>,
    permits: Arc<tokio::sync::Semaphore>,
    tls: Option<tokio_rustls::TlsAcceptor>,
    stream: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
) where
    S: SessionFactory,
{
    match Arc::clone(&permits).try_acquire_owned() {
        Ok(permit) => {
            tokio::spawn(async move {
                match tls {
                    Some(acceptor) => match acceptor.accept(stream).await {
                        Ok(tls_stream) => run_connection(sessions, tls_stream, permit).await,
                        Err(error) => {
                            // A failed handshake drops the connection and the permit
                            // with it; the accept loop keeps running (D-12).
                            log::warn!("tls handshake failed: {}", tls_error_class(&error));
                        }
                    },
                    None => run_connection(sessions, stream, permit).await,
                }
            });
        }
        Err(_) => log::warn!(
            "connection limit {} reached; refusing {}",
            MAX_CONNECTIONS,
            peer
        ),
    }
}

/// Map a TLS handshake I/O error to a non-sensitive class name for logging.
fn tls_error_class(error: &std::io::Error) -> &'static str {
    use std::io::ErrorKind;
    match error.kind() {
        ErrorKind::InvalidData => "invalid_data",
        ErrorKind::UnexpectedEof => "unexpected_eof",
        ErrorKind::ConnectionReset => "connection_reset",
        ErrorKind::TimedOut => "timeout",
        ErrorKind::WouldBlock => "would_block",
        _ => "handshake_error",
    }
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
    prepare_working_directory(&opts.config_path);

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
/// misbehaving client must not stop the accept loop. Generic over the stream so a
/// plaintext `TcpStream` and a `TlsStream<TcpStream>` share one code path (D-11).
async fn run_connection<S, T>(
    sessions: Arc<S>,
    stream: T,
    _permit: tokio::sync::OwnedSemaphorePermit,
) where
    S: SessionFactory,
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    // Held for the connection's lifetime; dropping it frees the slot.
    let _keep = _permit;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFactory;

    impl ProviderFactory for TestFactory {
        fn build(&self, config: &SkfConfig) -> Arc<dyn SkfProvider> {
            Arc::new(UnavailableProvider::new(
                config.default.clone(),
                "test factory".to_string(),
            ))
        }
    }

    fn write_config(name: &str, body: &str) -> String {
        let path = std::env::temp_dir().join(format!(
            "skf-server-test-{}-{}.yaml",
            std::process::id(),
            name
        ));
        std::fs::write(&path, body).expect("write config");
        path.to_string_lossy().into_owned()
    }

    /// A config that resolves for this OS (path may be nonexistent).
    fn config_for_this_os() -> String {
        let os = std::env::consts::OS;
        format!(
            "default: GM3000\nvendor: {{}}\nGM3000:\n  {}: native/missing-lib\n",
            os
        )
    }

    fn options(config_path: String, ws_addr: &str) -> ServerOptions {
        ServerOptions {
            config_path,
            ws_addr: ws_addr.to_string(),
            http_addr: None,
            mode: RunMode::Console,
        }
    }

    /// Extract a startup failure without requiring `PreparedServer: Debug`.
    #[allow(clippy::type_complexity)]
    fn startup_error(
        result: Result<(BoundServer, TcpListener, PreparedServer), StartupError>,
    ) -> StartupError {
        match result {
            Ok(_) => panic!("expected a startup failure"),
            Err(error) => error,
        }
    }

    #[tokio::test]
    async fn bind_with_progress_reports_config_provider_bind_in_order() {
        use crate::service_state::StartupStage;
        let opts = options(write_config("order", &config_for_this_os()), "127.0.0.1:0");
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);

        let result = bind_with_progress(&opts, &TestFactory, move |stage| {
            sink.lock().expect("mutex").push(stage);
        })
        .await;

        assert!(
            result.is_ok(),
            "expected bind to succeed: {:?}",
            result.err()
        );
        let stages = seen.lock().expect("mutex").clone();
        assert_eq!(
            stages,
            vec![
                StartupStage::Config,
                StartupStage::Provider,
                StartupStage::Bind
            ]
        );
    }

    #[tokio::test]
    async fn missing_os_path_is_a_provider_error() {
        // A config that has a path for some OS other than this one.
        let other = if std::env::consts::OS == "windows" {
            "linux"
        } else {
            "windows"
        };
        let body = format!(
            "default: GM3000\nvendor: {{}}\nGM3000:\n  {}: native/missing-lib\n",
            other
        );
        let opts = options(write_config("no-os", &body), "127.0.0.1:0");

        let error = startup_error(bind_with_progress(&opts, &TestFactory, |_| {}).await);
        assert!(matches!(error, StartupError::Provider(_)), "{:?}", error);
        assert_eq!(error.exit_code(), crate::service_state::EXIT_PROVIDER);
    }

    #[tokio::test]
    async fn unparseable_config_is_a_config_error() {
        let opts = options(
            write_config("bad-yaml", "default: [unclosed\n"),
            "127.0.0.1:0",
        );

        let error = startup_error(bind_with_progress(&opts, &TestFactory, |_| {}).await);
        assert!(matches!(error, StartupError::Config(_)), "{:?}", error);
        assert_eq!(error.exit_code(), crate::service_state::EXIT_CONFIG);
    }

    #[tokio::test]
    async fn bind_failure_is_a_bind_error() {
        // Hold a listener so the next bind to the same port fails.
        let held = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("hold a port");
        let port = held.local_addr().expect("addr").port();
        let opts = options(
            write_config("bind-fail", &config_for_this_os()),
            &format!("127.0.0.1:{}", port),
        );

        let error = startup_error(bind_with_progress(&opts, &TestFactory, |_| {}).await);
        assert!(matches!(error, StartupError::Bind(_)), "{:?}", error);
        assert_eq!(error.exit_code(), crate::service_state::EXIT_BIND);
    }

    #[tokio::test]
    async fn configured_but_missing_library_still_binds() {
        // The path is configured but the file does not exist. This must NOT be a
        // startup failure: the factory degrades to UnavailableProvider and the
        // Linux CI relies on the service starting without the vendor library
        // (D-09).
        let opts = options(
            write_config("missing-lib", &config_for_this_os()),
            "127.0.0.1:0",
        );

        let result = bind_with_progress(&opts, &NativeProviderFactory, |_| {}).await;
        assert!(
            result.is_ok(),
            "a missing library file must degrade, not fail startup: {:?}",
            result.err()
        );
    }
}
