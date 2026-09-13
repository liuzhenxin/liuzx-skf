//! Client authentication end-to-end (Phase 8: AUTH-01..AUTH-04).
//!
//! All certificates are generated in-process with `rcgen` (a self-signed server
//! certificate, a client CA, and a leaf signed by it), so the suite is hermetic:
//! no checked-in keys, no external PKI, no OpenSSL CLI.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use serde_json::json;
use tokio_rustls::TlsConnector;

mod session_support;

use session_support::{request, start_service_with_env};

type TlsSocket =
    tokio_tungstenite::WebSocketStream<tokio_rustls::client::TlsStream<tokio::net::TcpStream>>;

/// Generated material for one test.
struct ServerMaterial {
    dir: PathBuf,
    server_ca: CertificateDer<'static>,
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("skf-client-auth-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// A CA plus a leaf certificate signed by it.
struct CaAndLeaf {
    ca_pem: String,
    leaf_der: CertificateDer<'static>,
    leaf_key: PrivateKeyDer<'static>,
}

fn generate_ca_and_leaf(name: &str) -> CaAndLeaf {
    use rcgen::{BasicConstraints, CertificateParams, CertifiedIssuer, DnType, IsCa, KeyPair};

    let ca_key = KeyPair::generate().expect("ca key");
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).expect("ca params");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, format!("{name}-ca"));
    let ca = CertifiedIssuer::self_signed(ca_params, ca_key).expect("ca cert");

    let leaf_key = KeyPair::generate().expect("leaf key");
    let mut leaf_params = CertificateParams::new(Vec::<String>::new()).expect("leaf params");
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, "test-client");
    let leaf = leaf_params.signed_by(&leaf_key, &ca).expect("leaf cert");

    CaAndLeaf {
        ca_pem: ca.pem(),
        leaf_der: leaf.der().clone(),
        leaf_key: PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
    }
}

/// Write a self-signed server certificate for `localhost` plus a client CA file.
fn generate_server(name: &str, ca: &CaAndLeaf) -> ServerMaterial {
    let dir = temp_dir(name);
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).expect("server cert");
    std::fs::write(dir.join("server.crt"), cert.pem()).expect("write cert");
    std::fs::write(dir.join("server.key"), signing_key.serialize_pem()).expect("write key");
    std::fs::write(dir.join("ca.crt"), &ca.ca_pem).expect("write ca");
    ServerMaterial {
        dir,
        server_ca: cert.der().clone(),
    }
}

fn write_config(dir: &Path, server: &ServerMaterial, mode: &str, with_tls: bool) -> PathBuf {
    let os = std::env::consts::OS;
    let tls = if with_tls {
        format!(
            "tls:\n  cert_file: {}\n  key_file: {}\n  client_auth: {}\n  client_ca_file: {}\n",
            server.dir.join("server.crt").display(),
            server.dir.join("server.key").display(),
            mode,
            server.dir.join("ca.crt").display()
        )
    } else {
        format!("tls:\n  client_auth: {}\n", mode)
    };
    let yaml = format!(
        "default: GM3000\nvendor: {{}}\n{}GM3000:\n  {}: native/missing-lib\n",
        tls, os
    );
    let path = dir.join("skf.yaml");
    std::fs::write(&path, yaml).expect("write config");
    path
}

fn start_with_config(path: &Path, extra_env: &[(&str, &str)]) -> session_support::ServiceProcess {
    let mut env: Vec<(&str, &str)> = vec![("SKF_CONFIG", path.to_str().expect("utf8"))];
    env.extend_from_slice(extra_env);
    start_service_with_env(&env)
}

fn client_config(
    server_ca: &CertificateDer<'static>,
    client: Option<(&CertificateDer<'static>, &PrivateKeyDer<'static>)>,
) -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(server_ca.clone()).expect("add server root");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_root_certificates(roots);
    match client {
        Some((cert, key)) => builder
            .with_client_auth_cert(vec![cert.clone()], key.clone_key())
            .expect("client auth cert"),
        None => builder.with_no_client_auth(),
    }
}

async fn tls_upgrade(port: u16, config: rustls::ClientConfig) -> Result<TlsSocket, String> {
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

/// Drive a full request; returns whether a JSON response arrived.
async fn tls_request_succeeds(port: u16, config: rustls::ClientConfig) -> bool {
    let Ok(mut ws) = tls_upgrade(port, config).await else {
        return false;
    };
    let payload = request("SetLanguage", json!(["EN"]), 1).to_string();
    if ws
        .send(tokio_tungstenite::tungstenite::Message::Text(payload))
        .await
        .is_err()
    {
        return false;
    }
    matches!(
        tokio::time::timeout(Duration::from_secs(5), ws.next()).await,
        Ok(Some(Ok(_)))
    )
}

/// Plaintext request with an optional bearer header; returns whether it succeeded.
async fn plaintext_request_succeeds(port: u16, bearer: Option<&'static str>) -> bool {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::tungstenite::http::HeaderValue;

    let mut request = format!("ws://127.0.0.1:{}/", port)
        .into_client_request()
        .expect("request");
    if let Some(token) = bearer {
        request
            .headers_mut()
            .insert("Authorization", HeaderValue::from_static(token));
    }
    let Ok((mut ws, _)) = tokio_tungstenite::connect_async(request).await else {
        return false;
    };
    let payload = request_message().to_string();
    if ws
        .send(tokio_tungstenite::tungstenite::Message::Text(payload))
        .await
        .is_err()
    {
        return false;
    }
    matches!(
        tokio::time::timeout(Duration::from_secs(5), ws.next()).await,
        Ok(Some(Ok(_)))
    )
}

fn request_message() -> serde_json::Value {
    request("SetLanguage", json!(["EN"]), 1)
}

// -- mTLS -------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mtls_without_client_certificate_is_refused() {
    let ca = generate_ca_and_leaf("no-cert");
    let server = generate_server("no-cert", &ca);
    let service_dir = temp_dir("no-cert-service");
    let config = write_config(&service_dir, &server, "mtls", true);
    let service = start_with_config(&config, &[]);

    let client = client_config(&server.server_ca, None);
    assert!(
        !tls_request_succeeds(service.port(), client).await,
        "a client without a certificate must be refused"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mtls_with_valid_client_certificate_works() {
    let ca = generate_ca_and_leaf("valid");
    let server = generate_server("valid", &ca);
    let service_dir = temp_dir("valid-service");
    let config = write_config(&service_dir, &server, "mtls", true);
    let service = start_with_config(&config, &[]);

    let client = client_config(&server.server_ca, Some((&ca.leaf_der, &ca.leaf_key)));
    assert!(
        tls_request_succeeds(service.port(), client).await,
        "a CA-signed client certificate must be accepted"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mtls_with_untrusted_client_certificate_is_refused() {
    // The server trusts `trusted`; the client presents a leaf from `rogue`.
    let trusted = generate_ca_and_leaf("trusted");
    let rogue = generate_ca_and_leaf("rogue");
    let server = generate_server("untrusted", &trusted);
    let service_dir = temp_dir("untrusted-service");
    let config = write_config(&service_dir, &server, "mtls", true);
    let service = start_with_config(&config, &[]);

    let client = client_config(&server.server_ca, Some((&rogue.leaf_der, &rogue.leaf_key)));
    assert!(
        !tls_request_succeeds(service.port(), client).await,
        "a certificate from an untrusted CA must be refused"
    );
}

// -- Bearer token ------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn token_is_required_and_checked() {
    let service_dir = temp_dir("token-service");
    // A dummy server material is only needed for the `tls:` shape; token mode
    // does not require TLS.
    let ca = generate_ca_and_leaf("token");
    let server = generate_server("token", &ca);
    let config = write_config(&service_dir, &server, "token", false);
    let service = start_with_config(&config, &[("SKF_TLS_TOKEN", "secret")]);

    assert!(
        !plaintext_request_succeeds(service.port(), None).await,
        "a request without a token must be refused"
    );
    assert!(
        !plaintext_request_succeeds(service.port(), Some("Bearer wrong")).await,
        "a request with the wrong token must be refused"
    );
    assert!(
        plaintext_request_succeeds(service.port(), Some("Bearer secret")).await,
        "a request with the correct token must succeed"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn token_mode_works_without_tls_on_loopback() {
    // Explicitly the same positive path as above, named for the AUTH-02 rule
    // that a token is sufficient on loopback even without TLS.
    let service_dir = temp_dir("token-plain");
    let ca = generate_ca_and_leaf("token-plain");
    let server = generate_server("token-plain", &ca);
    let config = write_config(&service_dir, &server, "token", false);
    let service = start_with_config(&config, &[("SKF_TLS_TOKEN", "loopback-token")]);

    assert!(
        plaintext_request_succeeds(service.port(), Some("Bearer loopback-token")).await,
        "loopback + token must work without TLS"
    );
}

// -- Secret safety -----------------------------------------------------------

#[test]
fn token_never_appears_in_diagnose_or_logs() {
    let service_dir = temp_dir("token-secret");
    let ca = generate_ca_and_leaf("token-secret");
    let server = generate_server("token-secret", &ca);
    let config = write_config(&service_dir, &server, "token", false);
    let exe = env!("CARGO_BIN_EXE_skf-service");
    let token = "diagnose-must-not-leak-me";

    let output = Command::new(exe)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SKF_TLS_TOKEN", token)
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
    assert!(!json.contains(token), "diagnose leaked the token: {json}");

    let mut child = Command::new(exe)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SKF_WS_ADDR", "127.0.0.1:0")
        .env("SKF_HTTP_ADDR", "127.0.0.1:0")
        .env("RUST_LOG", "info")
        .env("SKF_TLS_TOKEN", token)
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
                if line.contains("client_auth=") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        captured.contains("client_auth=token"),
        "expected a client_auth startup log line, got: {captured}"
    );
    assert!(!captured.contains(token), "the log leaked the token");
}
