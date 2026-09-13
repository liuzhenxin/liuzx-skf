//! Non-loopback bind exposure rule (BIND-01..BIND-03).
//!
//! `bind` is exercised through its public seam with a factory that never loads a
//! vendor library, so the address decision is isolated from provider behaviour.
//!
//! Since Phase 9 a non-loopback bind requires **all** of: remote opt-in, TLS, and
//! client authentication. The positive cases therefore carry generated TLS
//! material (rcgen) and a token.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use skf_service::config::SkfConfig;
use skf_service::provider::SkfProvider;
use skf_service::server::{self, ProviderFactory, RunMode, ServerOptions, UnavailableProvider};

/// Factory that never loads a library; the tests only care about the address gate.
struct TestFactory;

impl ProviderFactory for TestFactory {
    fn build(&self, config: &SkfConfig) -> Arc<dyn SkfProvider> {
        Arc::new(UnavailableProvider::new(
            config.default.clone(),
            "test factory".to_string(),
        ))
    }
}

static CONFIG_SEQ: AtomicU64 = AtomicU64::new(0);

/// Generated server certificate/key, kept alive for the test's duration.
struct TlsMaterial {
    _dir: std::path::PathBuf,
    tls_block: String,
}

/// Generate a self-signed `localhost` certificate and return a YAML `tls:` block.
fn tls_material(seq: u64) -> TlsMaterial {
    let dir = std::env::temp_dir().join(format!(
        "skf-loopback-gate-tls-{}-{}",
        std::process::id(),
        seq
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).expect("cert");
    let cert_path = dir.join("server.crt");
    let key_path = dir.join("server.key");
    std::fs::write(&cert_path, cert.pem()).expect("write cert");
    std::fs::write(&key_path, signing_key.serialize_pem()).expect("write key");
    TlsMaterial {
        _dir: dir,
        tls_block: format!(
            "  cert_file: {}\n  key_file: {}\n",
            cert_path.display(),
            key_path.display()
        ),
    }
}

/// Write a config file and return its path. `extra` is spliced in before the
/// provider map.
fn write_config(extra: &str) -> String {
    let seq = CONFIG_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "skf-loopback-gate-{}-{}.yaml",
        std::process::id(),
        seq
    ));
    // The path must exist for the *current* OS: `bind_with_progress` treats a
    // missing OS path as a fatal configuration error (phase 5 D-08), so a
    // macOS-only config would fail on Linux before reaching the loopback check.
    let body = format!(
        "default: GM3000\nvendor: {{}}\n{}GM3000:\n  {}: native/x-lib\n",
        extra,
        std::env::consts::OS
    );
    std::fs::write(&path, body).expect("write config");
    path.to_string_lossy().into_owned()
}

fn options(config_path: String, ws_addr: &str) -> ServerOptions {
    ServerOptions {
        config_path,
        ws_addr: ws_addr.to_string(),
        http_addr: None,
        mode: RunMode::Console,
    }
}

/// Environment state is process-global; serialise the tests that touch it.
///
/// An async mutex, not a blocking one: these tests hold the guard across
/// `bind(...).await`, and holding a blocking guard across an await point is the
/// hazard `clippy::await_holding_lock` exists to catch.
fn env_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    &LOCK
}

/// Clear every environment variable the bind decision reads.
fn clear_env() {
    std::env::remove_var(SkfConfig::REMOTE_ENV);
    std::env::remove_var(SkfConfig::TLS_TOKEN_ENV);
    std::env::remove_var(SkfConfig::TLS_CERT_ENV);
    std::env::remove_var(SkfConfig::TLS_KEY_ENV);
    std::env::remove_var(SkfConfig::TLS_CLIENT_AUTH_ENV);
    std::env::remove_var(SkfConfig::TLS_CLIENT_CA_ENV);
    std::env::remove_var(SkfConfig::TLS_TOKEN_FILE_ENV);
}

#[tokio::test]
async fn non_loopback_without_opt_in_is_refused() {
    let _guard = env_lock().lock().await;
    clear_env();
    let opts = options(write_config(""), "0.0.0.0:0");

    let error = match server::bind(&opts, &TestFactory).await {
        Ok(_) => panic!("a wildcard bind must be refused without opt-in"),
        Err(e) => e,
    };
    let text = error.to_string();
    assert!(text.contains("non-loopback"), "message: {}", text);
    assert!(text.contains("allow_remote"), "message: {}", text);
}

#[tokio::test]
async fn loopback_is_allowed_without_opt_in() {
    let _guard = env_lock().lock().await;
    clear_env();
    let opts = options(write_config(""), "127.0.0.1:0");

    let (bound, _listener, _prepared) = server::bind(&opts, &TestFactory)
        .await
        .expect("loopback must bind");
    assert!(
        bound.ws_addr.ip().is_loopback(),
        "bound address must be loopback: {}",
        bound.ws_addr
    );
}

#[tokio::test]
async fn non_loopback_is_allowed_with_env_opt_in() {
    let _guard = env_lock().lock().await;
    clear_env();
    let material = tls_material(1);
    std::env::set_var(SkfConfig::REMOTE_ENV, "1");
    std::env::set_var(SkfConfig::TLS_TOKEN_ENV, "test-token");
    let extra = format!("tls:\n{}  client_auth: token\n", material.tls_block);
    let opts = options(write_config(&extra), "0.0.0.0:0");

    let result = server::bind(&opts, &TestFactory).await;
    clear_env();

    let (_bound, _listener, _prepared) = result.expect("opt-in + TLS + auth must allow the bind");
}

#[tokio::test]
async fn non_loopback_is_allowed_with_yaml_opt_in() {
    let _guard = env_lock().lock().await;
    clear_env();
    let material = tls_material(2);
    std::env::set_var(SkfConfig::TLS_TOKEN_ENV, "test-token");
    let extra = format!(
        "allow_remote: true\ntls:\n{}  client_auth: token\n",
        material.tls_block
    );
    let opts = options(write_config(&extra), "0.0.0.0:0");

    let result = server::bind(&opts, &TestFactory).await;
    clear_env();

    let (_bound, _listener, _prepared) = result.expect("yaml opt-in must allow the bind");
}

#[tokio::test]
async fn non_loopback_without_tls_is_refused() {
    let _guard = env_lock().lock().await;
    clear_env();
    // Opt-in and client auth are present, but there is no TLS: still refused.
    std::env::set_var(SkfConfig::TLS_TOKEN_ENV, "test-token");
    let extra = "allow_remote: true\ntls:\n  client_auth: token\n";
    let opts = options(write_config(extra), "0.0.0.0:0");

    let error = match server::bind(&opts, &TestFactory).await {
        Ok(_) => panic!("remote bind without TLS must be refused"),
        Err(e) => e,
    };
    clear_env();
    let text = error.to_string();
    assert!(text.contains("TLS"), "message: {}", text);
}

#[tokio::test]
async fn non_loopback_without_client_auth_is_refused() {
    let _guard = env_lock().lock().await;
    clear_env();
    let material = tls_material(3);
    std::env::set_var(SkfConfig::REMOTE_ENV, "1");
    let extra = format!("tls:\n{}", material.tls_block);
    let opts = options(write_config(&extra), "0.0.0.0:0");

    let error = match server::bind(&opts, &TestFactory).await {
        Ok(_) => panic!("remote bind without client auth must be refused"),
        Err(e) => e,
    };
    clear_env();
    let text = error.to_string();
    assert!(text.contains("client authentication"), "message: {}", text);
}

#[tokio::test]
async fn loopback_without_client_auth_still_binds() {
    let _guard = env_lock().lock().await;
    clear_env();
    let opts = options(write_config(""), "127.0.0.1:0");
    let (bound, _listener, _prepared) = server::bind(&opts, &TestFactory)
        .await
        .expect("loopback must bind without client auth");
    assert!(bound.ws_addr.ip().is_loopback());
}
