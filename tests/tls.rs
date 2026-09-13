//! TLS termination (Phase 7: TLS-01..TLS-04).
//!
//! Certificates are generated in-process with `rcgen`, so the suite is hermetic:
//! no checked-in private key, no `openssl` CLI, and nothing that depends on the
//! machine's OpenSSL version. The service is the real binary, started with a
//! temporary config, because the JSON-RPC dispatcher lives in the binary crate.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::pki_types::{CertificateDer, ServerName};
use serde_json::json;
use tokio_rustls::TlsConnector;

mod session_support;

use session_support::{connect, request, send_and_receive, start_service, start_service_with_env};

/// Generated certificate material for one test.
struct Material {
    dir: PathBuf,
    cert_der: CertificateDer<'static>,
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("skf-tls-it-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn generate_material(name: &str) -> Material {
    let dir = temp_dir(name);
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).expect("generate cert");
    std::fs::write(dir.join("server.crt"), cert.pem()).expect("write cert");
    std::fs::write(dir.join("server.key"), signing_key.serialize_pem()).expect("write key");
    Material {
        dir,
        cert_der: cert.der().clone(),
    }
}

impl Material {
    fn cert_path(&self) -> PathBuf {
        self.dir.join("server.crt")
    }

    fn key_path(&self) -> PathBuf {
        self.dir.join("server.key")
    }

    fn key_pem(&self) -> String {
        std::fs::read_to_string(self.key_path()).expect("read key")
    }
}

/// Write a config for this OS, optionally with a TLS block.
fn write_config(dir: &Path, material: Option<&Material>) -> PathBuf {
    let os = std::env::consts::OS;
    let tls = match material {
        Some(material) => format!(
            "tls:\n  cert_file: {}\n  key_file: {}\n",
            material.cert_path().display(),
            material.key_path().display()
        ),
        None => String::new(),
    };
    let yaml = format!(
        "default: GM3000\nvendor: {{}}\n{}GM3000:\n  {}: native/missing-lib\n",
        tls, os
    );
    let path = dir.join("skf.yaml");
    std::fs::write(&path, yaml).expect("write config");
    path
}

fn start_with_config(path: &Path) -> session_support::ServiceProcess {
    start_service_with_env(&[("SKF_CONFIG", path.to_str().expect("utf8"))])
}

type TlsSocket =
    tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>;

/// Attempt a TLS handshake and WebSocket upgrade. Returns the raw error string
/// on failure so callers can assert on refusal as well as success.
async fn try_tls_connect(port: u16, material: &Material) -> Result<TlsSocket, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(material.cert_der.clone()).expect("add test root");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));

    let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .map_err(|e| e.to_string())?;
    let domain = ServerName::try_from("localhost".to_string()).map_err(|e| e.to_string())?;
    let tls = connector
        .connect(domain, tcp)
        .await
        .map_err(|e| e.to_string())?;
    let (ws, _) = tokio_tungstenite::client_async("ws://localhost/", tls)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ws)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plaintext_default_still_works() {
    // No TLS block: the listener must behave exactly as it did before Phase 7.
    let service = start_service();
    let mut socket = connect(service.port()).await;
    let response = send_and_receive(&mut socket, &request("SetLanguage", json!(["EN"]), 1)).await;
    assert_eq!(
        response["error"], 0,
        "the default plaintext listener must keep working, got {}",
        response
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_client_handshake_and_call() {
    let service_dir = temp_dir("happy");
    let material = generate_material("happy");
    let config = write_config(&service_dir, Some(&material));

    let service = start_with_config(&config);
    let mut socket = try_tls_connect(service.port(), &material)
        .await
        .expect("TLS handshake and WebSocket upgrade must succeed");
    let response = send_and_receive(&mut socket, &request("SetLanguage", json!(["EN"]), 1)).await;
    assert_eq!(
        response["error"], 0,
        "a method call over TLS must succeed, got {}",
        response
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tls_not_configured_means_plaintext() {
    // The listener is plaintext, so a TLS client must be refused. This is the
    // negative half of "TLS is opt-in".
    let material = generate_material("negative");
    let service = start_service();

    let outcome = tokio::time::timeout(
        Duration::from_secs(10),
        try_tls_connect(service.port(), &material),
    )
    .await;
    if let Ok(Ok(_)) = outcome {
        panic!("a TLS client must not be served by a plaintext listener");
    }
}

#[test]
fn tls_key_never_appears_in_diagnose_or_logs() {
    let service_dir = temp_dir("secret");
    let material = generate_material("secret");
    let config = write_config(&service_dir, Some(&material));
    let cert_path = material.cert_path().to_string_lossy().into_owned();
    let key_path = material.key_path().to_string_lossy().into_owned();
    let exe = env!("CARGO_BIN_EXE_skf-service");

    // 1. `diagnose` reports booleans and nothing secret.
    let output = Command::new(exe)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "diagnose",
            "--config",
            config.to_str().expect("utf8"),
            "--json",
        ])
        .output()
        .expect("run diagnose");
    assert!(output.status.success(), "diagnose must succeed");
    let json = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        json.contains("\"tls_enabled\": true"),
        "diagnose must report tls_enabled, got {json}"
    );
    assert!(
        json.contains("\"tls_cert_loaded\": true"),
        "diagnose must report tls_cert_loaded, got {json}"
    );
    assert!(
        !json.contains(&cert_path),
        "diagnose leaked the certificate path"
    );
    assert!(
        !json.contains(&key_path),
        "diagnose leaked the private-key path"
    );
    assert!(
        !json.contains("PRIVATE KEY"),
        "diagnose leaked PEM material"
    );

    // 2. The startup log names the mode only, never the material or its path.
    let mut child = Command::new(exe)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SKF_WS_ADDR", "127.0.0.1:0")
        .env("SKF_HTTP_ADDR", "127.0.0.1:0")
        .env("RUST_LOG", "info")
        .env("SKF_CONFIG", config.to_str().expect("utf8"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start service");

    let stderr = child.stderr.take().expect("stderr pipe");
    let mut reader = BufReader::new(stderr);
    let mut captured = String::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                captured.push_str(&line);
                if line.contains("tls=") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        captured.contains("tls=enabled"),
        "expected a TLS startup log line, got: {captured}"
    );
    assert!(
        !captured.contains(&key_path),
        "the log leaked the private-key path"
    );
    assert!(
        !captured.contains(&cert_path),
        "the log leaked the certificate path"
    );
    assert!(
        !captured.contains("PRIVATE KEY"),
        "the log leaked PEM key material"
    );
    // Guard against an accidental future change that logs the whole block.
    let fingerprint = material
        .key_pem()
        .lines()
        .find(|l| l.len() > 20 && !l.starts_with("-----"))
        .unwrap_or_default()
        .to_string();
    if !fingerprint.is_empty() {
        assert!(
            !captured.contains(&fingerprint),
            "the log leaked private-key material"
        );
    }
}
