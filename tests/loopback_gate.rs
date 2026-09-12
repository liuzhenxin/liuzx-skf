//! Non-loopback bind opt-in (TRANS-06).
//!
//! `bind` is exercised through its public seam with a factory that never loads a
//! vendor library, so the address decision is isolated from provider behaviour.

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

/// Write a config file and return its path.
fn write_config(extra: &str) -> String {
    let seq = CONFIG_SEQ.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "skf-loopback-gate-{}-{}.yaml",
        std::process::id(),
        seq
    ));
    let body = format!(
        "default: GM3000\nvendor: {{}}\n{}GM3000:\n  macos: native/x.dylib\n",
        extra
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

#[tokio::test]
async fn non_loopback_is_refused_without_opt_in() {
    let _guard = env_lock().lock().await;
    std::env::remove_var(SkfConfig::REMOTE_ENV);
    let opts = options(write_config(""), "0.0.0.0:0");

    let error = match server::bind(&opts, &TestFactory).await {
        Ok(_) => panic!("a wildcard bind must be refused without opt-in"),
        Err(e) => e,
    };
    let text = error.to_string();
    assert!(text.contains("non-loopback"), "message: {}", text);
    assert!(text.contains("SKF_ALLOW_REMOTE"), "message: {}", text);
    assert!(text.contains("allow_remote"), "message: {}", text);
}

#[tokio::test]
async fn loopback_is_allowed_without_opt_in() {
    let _guard = env_lock().lock().await;
    std::env::remove_var(SkfConfig::REMOTE_ENV);
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
    std::env::set_var(SkfConfig::REMOTE_ENV, "1");
    let opts = options(write_config(""), "0.0.0.0:0");

    let result = server::bind(&opts, &TestFactory).await;
    std::env::remove_var(SkfConfig::REMOTE_ENV);

    let (_bound, _listener, _prepared) = result.expect("opt-in must allow the bind");
}

#[tokio::test]
async fn non_loopback_is_allowed_with_yaml_opt_in() {
    let _guard = env_lock().lock().await;
    std::env::remove_var(SkfConfig::REMOTE_ENV);
    let opts = options(write_config("allow_remote: true\n"), "0.0.0.0:0");

    let result = server::bind(&opts, &TestFactory).await;
    let (_bound, _listener, _prepared) = result.expect("yaml opt-in must allow the bind");
}
