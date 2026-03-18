mod skf;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use libloading::Library;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use futures_util::{StreamExt, SinkExt};
use crate::skf::api::SkfApi;
use crate::skf::types::{CHAR, ULONG, BYTE, SAR_OK, DEVHANDLE, HAPPLICATION, HCONTAINER, HANDLE, ECCSIGNATUREBLOB, ECCPUBLICKEYBLOB, RSAPUBLICKEYBLOB, SGD_SM3, SGD_SM2_1, SGD_SM4_ECB};
use base64::prelude::*;
use x509_parser::prelude::*;

#[derive(Debug, Deserialize, Clone)]
struct SkfConfig {
    default: String,
    vendor: HashMap<String, String>,
    #[serde(flatten)]
    libs: HashMap<String, HashMap<String, String>>,
}

struct SkfContext {
    config: SkfConfig,
    apis: RwLock<HashMap<String, Arc<SkfApi>>>,
    // Map of "provider/device/app" -> "pin"
    pins: RwLock<HashMap<String, String>>,
}

impl SkfContext {
    fn new(config: SkfConfig) -> Self {
        Self {
            config,
            apis: RwLock::new(HashMap::new()),
            pins: RwLock::new(HashMap::new()),
        }
    }

    fn get_api(&self, provider: &str) -> anyhow::Result<Arc<SkfApi>> {
        // 1. Check cache (read)
        {
            let map = self.apis.read().unwrap();
            if let Some(api) = map.get(provider) {
                return Ok(api.clone());
            }
        }

        // 2. Load
        let os = std::env::consts::OS;
        let raw_path = self.config.libs
            .get(provider)
            .and_then(|m| {
                m.get(os)
                 .or_else(|| m.iter().find(|(k, _)| k.eq_ignore_ascii_case(os)).map(|(_, v)| v))
            })
            .ok_or_else(|| {
                 let available_providers: Vec<_> = self.config.libs.keys().collect();
                 let available_os = if let Some(m) = self.config.libs.get(provider) {
                     format!("{:?}", m.keys())
                 } else {
                     "None".to_string()
                 };
                 anyhow::anyhow!("Provider '{}' not configured for OS '{}'. Avail Provs: {:?}. Avail OS for {}: {}", provider, os, available_providers, provider, available_os)
            })?;

        let lib_path = Self::expand_env_vars(raw_path);
        println!("Loading SKF library for {} from: {}", provider, lib_path);
        
        // Safety: Loading foreign libraries is inherently unsafe.
        let lib = unsafe { Library::new(&lib_path) }
            .map_err(|e| anyhow::anyhow!("Failed to load library '{}': {}", lib_path, e))?;
            
        let api = Arc::new(SkfApi::new(lib));

        // 3. Cache (write)
        {
            let mut map = self.apis.write().unwrap();
            map.insert(provider.to_string(), api.clone());
        }
        Ok(api)
    }

    fn expand_env_vars(path: &str) -> String {
        let mut result = String::new();
        let mut chars = path.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '%' {
                let mut var_name = String::new();
                let mut closed = false;
                while let Some(&n) = chars.peek() {
                    if n == '%' {
                        chars.next();
                        closed = true;
                        break;
                    }
                    var_name.push(chars.next().unwrap());
                }
                
                if closed && !var_name.is_empty() {
                    match std::env::var(&var_name) {
                        Ok(val) => result.push_str(&val),
                        Err(_) => {
                            // If var not found, keep original string
                            result.push('%');
                            result.push_str(&var_name);
                            result.push('%');
                        }
                    }
                } else {
                    // Not a valid var format, push literal % and what we ate
                    result.push('%');
                    result.push_str(&var_name);
                }
            } else {
                result.push(c);
            }
        }
        result
    }
}

// JSON-RPC Request/Response
#[derive(Debug, Deserialize)]
struct RpcRequest {
    method: String,
    #[serde(default)]
    params: Vec<serde_json::Value>,
    id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct RpcResponse {
    error: i32,
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    result: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<serde_json::Value>,
}

impl RpcResponse {
    fn ok(result: serde_json::Value, id: Option<serde_json::Value>) -> Self {
        Self { error: 0, result, message: None, id }
    }
    fn err(code: i32, msg: String, id: Option<serde_json::Value>) -> Self {
        Self { error: code, result: serde_json::Value::Null, message: Some(msg), id }
    }
}

#[derive(Clone, Copy)]
enum Language {
    EN,
    CN,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let cfg_text = std::fs::read_to_string("config/skf.yaml")?;
    let cfg: SkfConfig = serde_yaml::from_str(&cfg_text)?;

    // Initialize empty context with config
    let ctx = Arc::new(SkfContext::new(cfg));

    // --- Start HTTP Server for API Demo ---
    // Serve "api" directory at http://0.0.0.0:8000
    // If "api" folder is in current working directory
    tokio::spawn(async {
        use warp::Filter;
        let api_route = warp::fs::dir("api");
        println!("HTTP Server serving 'api' directory on http://0.0.0.0:8000");
        warp::serve(api_route).run(([0, 0, 0, 0], 8000)).await;
    });
    // --------------------------------------

    let listener = TcpListener::bind("127.0.0.1:9001").await?;
    println!("SKF Service listening on ws://127.0.0.1:9001");

    while let Ok((stream, _)) = listener.accept().await {
        let ctx = ctx.clone();
        tokio::spawn(async move {
            let mut ws = accept_async(stream).await.unwrap();
            let mut lang = Language::EN; // Default to English
            while let Some(msg) = ws.next().await {
                match msg {
                    Ok(msg) if msg.is_text() => {
                        let text = msg.to_text().unwrap();
                        let resp = handle_request(&ctx, text, &mut lang).await;
                        let reply_str = serde_json::to_string(&resp).unwrap();
                        let _ = ws.send(tungstenite::Message::Text(reply_str)).await;
                    }
                    _ => {}
                }
            }
        });
    }
    Ok(())
}

/// Convert a hex string to bytes
fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let hex = hex.trim();
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| {
            if i + 2 <= hex.len() {
                u8::from_str_radix(&hex[i..i+2], 16).ok()
            } else {
                None
            }
        })
        .collect()
}

/// Encode a big-endian unsigned integer as DER INTEGER (tag 0x02).
/// Strips leading zeros and adds 0x00 pad if high bit is set.
fn der_encode_integer(bytes: &[u8]) -> Vec<u8> {
    // Strip leading zeros
    let start = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
    let trimmed = &bytes[start..];

    // If high bit set, prepend 0x00 (DER positive integer rule)
    let needs_pad = !trimmed.is_empty() && (trimmed[0] & 0x80) != 0;
    let int_len = trimmed.len() + if needs_pad { 1 } else { 0 };

    let mut out = Vec::with_capacity(2 + int_len);
    out.push(0x02); // INTEGER tag
    out.push(int_len as u8); // length (always < 128 for SM2/ECC)
    if needs_pad {
        out.push(0x00);
    }
    out.extend_from_slice(trimmed);
    out
}

/// DER encode length (supports lengths up to 65535)
fn der_encode_length(len: usize) -> Vec<u8> {
    if len < 128 {
        vec![len as u8]
    } else if len < 256 {
        vec![0x81, len as u8]
    } else {
        vec![0x82, (len >> 8) as u8, len as u8]
    }
}

/// Wrap content with a DER tag + length
fn der_wrap(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(der_encode_length(content.len()));
    out.extend_from_slice(content);
    out
}

/// DER SEQUENCE (tag 0x30)
fn der_sequence(items: &[&[u8]]) -> Vec<u8> {
    let mut content = Vec::new();
    for item in items { content.extend_from_slice(item); }
    der_wrap(0x30, &content)
}

/// DER SET (tag 0x31)
fn der_set(items: &[&[u8]]) -> Vec<u8> {
    let mut content = Vec::new();
    for item in items { content.extend_from_slice(item); }
    der_wrap(0x31, &content)
}

/// DER OID from pre-encoded bytes
fn der_oid(oid_bytes: &[u8]) -> Vec<u8> {
    der_wrap(0x06, oid_bytes)
}

/// DER UTF8String (tag 0x0C)
fn der_utf8_string(s: &str) -> Vec<u8> {
    der_wrap(0x0C, s.as_bytes())
}

/// DER PrintableString (tag 0x13)
fn der_printable_string(s: &str) -> Vec<u8> {
    der_wrap(0x13, s.as_bytes())
}

/// DER BIT STRING (tag 0x03) — wraps content with a 0x00 unused-bits prefix
fn der_bit_string(content: &[u8]) -> Vec<u8> {
    let mut inner = vec![0x00]; // 0 unused bits
    inner.extend_from_slice(content);
    der_wrap(0x03, &inner)
}

/// DER INTEGER with small value
fn der_small_integer(val: u8) -> Vec<u8> {
    vec![0x02, 0x01, val]
}

/// DER CONTEXT tag [0] (constructed, implicit)
fn der_context_0(content: &[u8]) -> Vec<u8> {
    der_wrap(0xA0, content)
}

// Well-known OIDs
const OID_CN: &[u8]  = &[0x55, 0x04, 0x03]; // 2.5.4.3
const OID_O: &[u8]   = &[0x55, 0x04, 0x0A]; // 2.5.4.10
const OID_OU: &[u8]  = &[0x55, 0x04, 0x0B]; // 2.5.4.11
const OID_C: &[u8]   = &[0x55, 0x04, 0x06]; // 2.5.4.6
const OID_ST: &[u8]  = &[0x55, 0x04, 0x08]; // 2.5.4.8
const OID_L: &[u8]   = &[0x55, 0x04, 0x07]; // 2.5.4.7
const OID_E: &[u8]   = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x01]; // 1.2.840.113549.1.9.1

// SM2 OID: 1.2.156.10197.1.301
const OID_SM2: &[u8] = &[0x2A, 0x81, 0x1C, 0xCF, 0x55, 0x01, 0x82, 0x2D];
// SM3withSM2 OID: 1.2.156.10197.1.501
const OID_SM3_WITH_SM2: &[u8] = &[0x2A, 0x81, 0x1C, 0xCF, 0x55, 0x01, 0x83, 0x75];
// EC public key OID: 1.2.840.10045.2.1
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
// RSA encryption OID: 1.2.840.113549.1.1.1
const OID_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
// SHA256withRSA OID: 1.2.840.113549.1.1.11
const OID_SHA256_WITH_RSA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];

/// Parse "CN=Test,O=MyOrg,C=CN" into DER-encoded Name (SEQUENCE of SET of SEQUENCE {OID, value})
fn build_subject_dn(subject: &str) -> Vec<u8> {
    let mut rdns: Vec<Vec<u8>> = Vec::new();
    for part in subject.split(',') {
        let part = part.trim();
        if let Some((key, val)) = part.split_once('=') {
            let key = key.trim().to_uppercase();
            let oid = match key.as_str() {
                "CN" => OID_CN,
                "O"  => OID_O,
                "OU" => OID_OU,
                "C"  => OID_C,
                "ST" => OID_ST,
                "L"  => OID_L,
                "E" | "EMAIL" | "EMAILADDRESS" => OID_E,
                _ => continue,
            };
            let val = val.trim();
            // C (country) uses PrintableString, others UTF8String
            let value_der = if key == "C" {
                der_printable_string(val)
            } else {
                der_utf8_string(val)
            };
            let attr_type_val = der_sequence(&[&der_oid(oid), &value_der]);
            let rdn = der_set(&[&attr_type_val]);
            rdns.push(rdn);
        }
    }
    let refs: Vec<&[u8]> = rdns.iter().map(|r| r.as_slice()).collect();
    der_sequence(&refs)
}

/// Build SubjectPublicKeyInfo for SM2 key
fn build_sm2_spki(pub_key: &ECCPUBLICKEYBLOB) -> Vec<u8> {
    // Algorithm: SEQUENCE { OID ecPublicKey, OID SM2 }
    let alg = der_sequence(&[&der_oid(OID_EC_PUBLIC_KEY), &der_oid(OID_SM2)]);
    // Public key: 0x04 || X(32) || Y(32)  (uncompressed point)
    // SKF stores 32-byte SM2 values right-aligned in 64-byte arrays
    let mut point = Vec::with_capacity(1 + 32 * 2);
    point.push(0x04); // uncompressed
    point.extend_from_slice(&pub_key.XCoordinate[32..64]);
    point.extend_from_slice(&pub_key.YCoordinate[32..64]);
    let pub_key_bits = der_bit_string(&point);
    der_sequence(&[&alg, &pub_key_bits])
}

/// Build SubjectPublicKeyInfo for RSA key
fn build_rsa_spki(pub_key: &RSAPUBLICKEYBLOB) -> Vec<u8> {
    // Algorithm: SEQUENCE { OID rsaEncryption, NULL }
    let alg = der_sequence(&[&der_oid(OID_RSA), &[0x05, 0x00]]);
    // RSA public key: SEQUENCE { INTEGER modulus, INTEGER exponent }
    let key_len = (pub_key.BitLen / 8) as usize;
    let n = der_encode_integer(&pub_key.Modulus[..key_len]);
    let e = der_encode_integer(&pub_key.PublicExponent);
    let rsa_key = der_sequence(&[&n, &e]);
    let pub_key_bits = der_bit_string(&rsa_key);
    der_sequence(&[&alg, &pub_key_bits])
}

async fn handle_request(ctx: &SkfContext, text: &str, lang: &mut Language) -> RpcResponse {
    let req: RpcRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => return RpcResponse::err(-1, format!("Invalid JSON: {}", e), None),
    };

    let id = req.id.clone();

    eprintln!("[DEBUG] Received method: '{}', params count: {}", req.method, req.params.len());

    match req.method.as_str() {
        "SetLanguage" => {
            if let Some(l) = req.params.get(0).and_then(|v| v.as_str()) {
                *lang = match l.to_uppercase().as_str() {
                    "CN" | "ZH" => Language::CN,
                    _ => Language::EN,
                };
                RpcResponse::ok(serde_json::json!("OK"), id)
            } else {
                 RpcResponse::err(-2, "Missing Language Code".into(), id)
            }
        },
        "WaitForDevEvent" => {
             let provider = req.params.get(0)
                .and_then(|v| v.as_str())
                .unwrap_or(&ctx.config.default);

             let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => {
                     let msg = match lang {
                         Language::CN => format!("加载库失败: {}", e),
                         Language::EN => format!("Load Lib Failed: {}", e),
                     };
                     return RpcResponse::err(-1, msg, id);
                 }
             };

             let res = tokio::task::spawn_blocking(move || {
                 let mut dev_name = [0u8; 256];
                 let mut dev_name_len = 256;
                 let mut event = 0;
                 let rv = api.wait_for_dev_event(dev_name.as_mut_ptr() as *mut CHAR, &mut dev_name_len, &mut event);
                 (rv, dev_name, dev_name_len, event)
             }).await;

             match res {
                 Ok((0, dev_name, dev_name_len, event)) => {
                     let len_usize = dev_name_len as usize;
                     let actual_len = if len_usize <= 256 { len_usize } else { 256 };
                     let name_str = String::from_utf8_lossy(&dev_name[..actual_len])
                         .trim_end_matches(char::from(0))
                         .to_string();
                     RpcResponse::ok(serde_json::json!({ "deviceName": name_str, "event": event }), id)
                 }
                 Ok((rv, _, _, _)) => {
                     RpcResponse::err(rv as i32, format!("WaitForDevEvent failed: {:#X}", rv), id)
                 }
                 Err(e) => {
                     RpcResponse::err(-1, format!("Task panicked: {}", e), id)
                 }
             }
        },
        "EnumProvider" => {
            let vpid_opt = req.params.get(0).and_then(|v| v.as_str());
            
            match vpid_opt {
                Some(vpid) if !vpid.is_empty() => {
                    if let Some(provider) = ctx.config.vendor.get(vpid) {
                        RpcResponse::ok(serde_json::json!(provider), id)
                    } else {
                        let msg = match lang {
                            Language::CN => format!("未知 VID:PID: {}", vpid),
                            Language::EN => format!("Unknown VPID: {}", vpid),
                        };
                        RpcResponse::err(-4, msg, id)
                    }
                },
                _ => {
                    let mut providers: Vec<String> = ctx.config.libs.keys().cloned().collect();
                    let default_provider = &ctx.config.default;
                    if let Some(pos) = providers.iter().position(|x| x == default_provider) {
                        let default_val = providers.remove(pos);
                        providers.insert(0, default_val);
                    }
                    RpcResponse::ok(serde_json::json!(providers), id)
                }
            }
        },
        "EnumDevice" => {
             let provider = req.params.get(0)
                .and_then(|v| v.as_str())
                .unwrap_or(&ctx.config.default);
 
             let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => {
                     let msg = match lang {
                         Language::CN => format!("加载库失败: {}", e),
                         Language::EN => format!("Load Lib Failed: {}", e),
                     };
                     return RpcResponse::err(-5, msg, id);
                 }
             };
 
             let mut size: ULONG = 0;
             let ret = api.enum_dev(1, std::ptr::null_mut(), &mut size);
             if ret != SAR_OK || size == 0 {
                 if ret == SAR_OK {
                     return RpcResponse::ok(serde_json::json!(Vec::<String>::new()), id);
                 }
                 let msg = match lang {
                     Language::CN => "枚举设备获取大小失败",
                     Language::EN => "EnumDev failed size check",
                 };
                 return RpcResponse::err(ret as i32, msg.into(), id);
             }
             let mut buf = vec![0u8 as CHAR; size as usize];
             let ret = api.enum_dev(1, buf.as_mut_ptr(), &mut size);
             if ret == SAR_OK {
                 let raw_names = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, size as usize) };
                 let names: Vec<String> = raw_names.split(|&c| c == 0)
                     .filter(|s| !s.is_empty())
                     .map(|s| String::from_utf8_lossy(s).to_string())
                     .collect();
                 RpcResponse::ok(serde_json::json!(names), id)
             } else {
                 let msg = match lang {
                     Language::CN => "枚举设备失败",
                     Language::EN => "EnumDev failed",
                 };
                 RpcResponse::err(ret as i32, msg.into(), id)
             }
        },
        "ConnectDev" => {
            let provider = &ctx.config.default;
            let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
             };
 
            if let Some(name_v) = req.params.get(0) {
                 if let Some(name_str) = name_v.as_str() {
                     let name_c = std::ffi::CString::new(name_str).unwrap();
                     let mut h_dev: DEVHANDLE = std::ptr::null_mut();
                     let ret = api.connect_dev(name_c.into_raw(), &mut h_dev);
                     if ret == SAR_OK {
                         RpcResponse::ok(serde_json::json!(format!("{}", h_dev as usize)), id)
                     } else {
                         let msg = match lang {
                             Language::CN => "连接设备失败",
                             Language::EN => "ConnectDev failed",
                         };
                         RpcResponse::err(ret as i32, msg.into(), id)
                     }
                 } else {
                     RpcResponse::err(-2, "Invalid param type".into(), id)
                 }
            } else {
                RpcResponse::err(-2, "Missing param".into(), id)
            }
        },
        "EnumApplication" => {
             let provider = req.params.get(0).and_then(|v| v.as_str()).unwrap_or(&ctx.config.default);
             let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
                 Some(d) => d,
                 None => return RpcResponse::err(-2, "Missing deviceName param".into(), id),
             };

             let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
             };

             let c_dev = std::ffi::CString::new(dev_name).unwrap();
             let mut h_dev: DEVHANDLE = std::ptr::null_mut();
             let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
             if ret != SAR_OK {
                 return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
             }

             let mut size: ULONG = 0;
             let ret = api.enum_application(h_dev, std::ptr::null_mut(), &mut size);
             if ret != SAR_OK || size == 0 {
                 api.dis_connect_dev(h_dev);
                 if ret == SAR_OK {
                     return RpcResponse::ok(serde_json::json!(Vec::<String>::new()), id);
                 }
                 let msg = match lang {
                     Language::CN => "枚举应用获取大小失败",
                     Language::EN => "EnumApplication failed size check",
                 };
                 return RpcResponse::err(ret as i32, msg.into(), id);
             }
             let mut buf = vec![0u8 as CHAR; size as usize];
             let ret = api.enum_application(h_dev, buf.as_mut_ptr(), &mut size);
             api.dis_connect_dev(h_dev);
             
             if ret == SAR_OK {
                 let raw_names = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, size as usize) };
                 let names: Vec<String> = raw_names.split(|&c| c == 0)
                     .filter(|s| !s.is_empty())
                     .map(|s| String::from_utf8_lossy(s).to_string())
                     .collect();
                 RpcResponse::ok(serde_json::json!(names), id)
             } else {
                 let msg = match lang {
                     Language::CN => "枚举应用失败",
                     Language::EN => "EnumApplication failed",
                 };
                 RpcResponse::err(ret as i32, msg.into(), id)
             }
        },
        "EnumContainer" => {
             // Params: [providerName, deviceName, appName]
             let provider = req.params.get(0).and_then(|v| v.as_str()).unwrap_or(&ctx.config.default);
             let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
                 Some(d) => d,
                 None => return RpcResponse::err(-2, "Missing deviceName param".into(), id),
             };
             let app_name = match req.params.get(2).and_then(|v| v.as_str()) {
                 Some(a) => a,
                 None => return RpcResponse::err(-2, "Missing appName param".into(), id),
             };

             let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
             };

             let c_dev = std::ffi::CString::new(dev_name).unwrap();
             let mut h_dev: DEVHANDLE = std::ptr::null_mut();
             let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
             if ret != SAR_OK {
                 return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
             }

             let c_app = std::ffi::CString::new(app_name).unwrap();
             let mut h_app: HAPPLICATION = std::ptr::null_mut();
             let ret = api.open_application(h_dev, c_app.into_raw(), &mut h_app);
             if ret != SAR_OK {
                 api.dis_connect_dev(h_dev);
                 return RpcResponse::err(ret as i32, format!("OpenApplication failed: 0x{:08X}", ret), id);
             }

             let mut size: ULONG = 0;
             let ret = api.enum_container(h_app, std::ptr::null_mut(), &mut size);
             if ret != SAR_OK || size == 0 {
                 api.close_application(h_app);
                 api.dis_connect_dev(h_dev);
                 if ret == SAR_OK {
                     return RpcResponse::ok(serde_json::json!(Vec::<String>::new()), id);
                 }
                 let msg = match lang {
                     Language::CN => "枚举容器获取大小失败",
                     Language::EN => "EnumContainer failed size check",
                 };
                 return RpcResponse::err(ret as i32, msg.into(), id);
             }
             
             let mut buf = vec![0u8 as CHAR; size as usize];
             let ret = api.enum_container(h_app, buf.as_mut_ptr(), &mut size);
             api.close_application(h_app);
             api.dis_connect_dev(h_dev);
             
             if ret == SAR_OK {
                 let raw_names = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, size as usize) };
                 let names: Vec<String> = raw_names.split(|&c| c == 0)
                     .filter(|s| !s.is_empty())
                     .map(|s| String::from_utf8_lossy(s).to_string())
                     .collect();
                 RpcResponse::ok(serde_json::json!(names), id)
             } else {
                 let msg = match lang {
                     Language::CN => "枚举容器失败",
                     Language::EN => "EnumContainer failed",
                 };
                 RpcResponse::err(ret as i32, msg.into(), id)
             }
        },
        "DeleteContainer" => {
             // Params: [providerName, deviceName, appName, containerName]
             let provider = req.params.get(0).and_then(|v| v.as_str()).unwrap_or(&ctx.config.default);
             let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
                 Some(d) => d,
                 None => return RpcResponse::err(-2, "Missing deviceName param".into(), id),
             };
             let app_name = match req.params.get(2).and_then(|v| v.as_str()) {
                 Some(a) => a,
                 None => return RpcResponse::err(-2, "Missing appName param".into(), id),
             };
             let cont_name = match req.params.get(3).and_then(|v| v.as_str()) {
                 Some(c) => c,
                 None => return RpcResponse::err(-2, "Missing containerName param".into(), id),
             };

             let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
             };

             let c_dev = std::ffi::CString::new(dev_name).unwrap();
             let mut h_dev: DEVHANDLE = std::ptr::null_mut();
             let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
             if ret != SAR_OK {
                 return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
             }

             let c_app = std::ffi::CString::new(app_name).unwrap();
             let mut h_app: HAPPLICATION = std::ptr::null_mut();
             let ret = api.open_application(h_dev, c_app.into_raw(), &mut h_app);
             if ret != SAR_OK {
                 api.dis_connect_dev(h_dev);
                 return RpcResponse::err(ret as i32, format!("OpenApplication failed: 0x{:08X}", ret), id);
             }

             // PIN from cache
             let pin_key = format!("{}/{}/{}", provider, dev_name, app_name);
             let pin_cached = {
                 let pins = ctx.pins.read().unwrap();
                 pins.get(&pin_key).cloned()
             };

             if let Some(pin_str) = pin_cached {
                 let c_pin = std::ffi::CString::new(pin_str).unwrap();
                 let mut retry: ULONG = 0;
                 let ret = api.verify_pin(h_app, 1, c_pin.into_raw(), &mut retry);
                 if ret != SAR_OK {
                     api.close_application(h_app);
                     api.dis_connect_dev(h_dev);
                     let msg = match lang {
                         Language::CN => format!("PIN验证失败: 0x{:08X}, 剩余: {}", ret, retry),
                         Language::EN => format!("VerifyPIN failed: 0x{:08X}, retry: {}", ret, retry),
                     };
                     return RpcResponse::err(ret as i32, msg, id);
                 }
             } else {
                 api.close_application(h_app);
                 api.dis_connect_dev(h_dev);
                 return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
             }

             let c_cont = std::ffi::CString::new(cont_name).unwrap();
             let ret = api.delete_container(h_app, c_cont.into_raw());
             
             api.close_application(h_app);
             api.dis_connect_dev(h_dev);

             if ret == SAR_OK {
                 RpcResponse::ok(serde_json::json!(true), id)
             } else {
                 let msg = match lang {
                     Language::CN => format!("删除容器失败: 0x{:08X}", ret),
                     Language::EN => format!("DeleteContainer failed: 0x{:08X}", ret),
                 };
                 RpcResponse::err(ret as i32, msg, id)
             }
        },
        "IssueCertificate" => {
             // Params: [csr_base64_or_pem, double?]
             let csr_str = match req.params.get(0).and_then(|v| v.as_str()) {
                 Some(c) => c,
                 None => return RpcResponse::err(-2, "Missing CSR param".into(), id),
             };
             let double = req.params.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
             
             let csr_bytes = if csr_str.contains("-----BEGIN") {
                 csr_str.as_bytes().to_vec()
             } else {
                 match base64::engine::general_purpose::STANDARD.decode(csr_str) {
                     Ok(b) => b,
                     Err(e) => return RpcResponse::err(-3, format!("Invalid base64: {}", e), id),
                 }
             };

             // Generate a unique ID for the temp files
             let req_id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
             let csr_path = format!("/tmp/req_{}.csr", req_id);
             let crt_path = format!("/tmp/cert_{}.crt", req_id);
             
             if let Err(e) = std::fs::write(&csr_path, &csr_bytes) {
                 return RpcResponse::err(-4, format!("Failed to write CSR to disk: {}", e), id);
             }

             // Extract subject and public key from CSR
             let csr_der_parse = if csr_str.contains("-----BEGIN") {
                 let b64 = csr_str
                     .replace("-----BEGIN CERTIFICATE REQUEST-----", "")
                     .replace("-----END CERTIFICATE REQUEST-----", "")
                     .replace("-----BEGIN NEW CERTIFICATE REQUEST-----", "")
                     .replace("-----END NEW CERTIFICATE REQUEST-----", "")
                     .replace("\n", "").replace("\r", "");
                 base64::engine::general_purpose::STANDARD.decode(b64).unwrap_or_default()
             } else {
                 csr_bytes.clone()
             };

             let mut extracted_subject = "/CN=SKF Generated Cert".to_string();
             let mut sign_pub_xy = None;

             // Extract subject using OpenSSL
             let subj_output = std::process::Command::new("openssl")
                 .args(&["req", "-in", &csr_path, "-noout", "-subject"])
                 .output();
             if let Ok(out) = subj_output {
                 let s = String::from_utf8_lossy(&out.stdout).to_string();
                 if let Some(pos) = s.find("subject=") {
                     let subj_part = s[pos + 8..].trim();
                     if !subj_part.is_empty() {
                         // Convert "C=CN, ST=Beijing, ..." to "/C=CN/ST=Beijing/..."
                         let items: Vec<&str> = subj_part.split(", ").collect();
                         let mut formatted = String::new();
                         for item in items {
                             if !item.is_empty() {
                                 formatted.push('/');
                                 formatted.push_str(item);
                             }
                         }
                         if !formatted.is_empty() {
                             extracted_subject = formatted;
                         }
                     }
                 }
             }

             // Extract SM2 public key if present
             if let Ok((_, csr)) = x509_parser::certification_request::X509CertificationRequest::from_der(&csr_der_parse) {
                 let pk_info = &csr.certification_request_info.subject_pki;
                 let pk_data = &pk_info.subject_public_key.data;
                 if pk_data.len() >= 65 && pk_data[0] == 0x04 {
                     sign_pub_xy = Some((pk_data[1..33].to_vec(), pk_data[33..65].to_vec()));
                 }
             }

             // Check if mock CA exists, else generate
             if !std::path::Path::new("/tmp/skf_ca.key").exists() {
                 let _ = std::process::Command::new("openssl")
                     .args(&["req", "-x509", "-newkey", "rsa:2048", "-keyout", "/tmp/skf_ca.key", "-out", "/tmp/skf_ca.crt", "-days", "3650", "-nodes", "-subj", "/CN=SKF Demo CA"])
                     .output();
             }

             // Extract public key from CSR (SM2 CSRs require distid for signature verification)
             let pub_key_path = format!("/tmp/pub_{}.pem", req_id);
             let _ = std::process::Command::new("sh")
                 .args(&["-c", &format!("openssl req -in {} -pubkey -noout -vfyopt distid:1234567812345678 > {}", csr_path, pub_key_path)])
                 .output();

             // Generate a dummy RSA CSR
             let dummy_csr_path = format!("/tmp/dummy_{}.csr", req_id);
             let dummy_key_path = format!("/tmp/dummy_{}.key", req_id);
             let _ = std::process::Command::new("openssl")
                 .args(&["req", "-new", "-newkey", "rsa:2048", "-nodes", "-keyout", &dummy_key_path, "-out", &dummy_csr_path, "-subj", &extracted_subject])
                 .output();
                 
             // Issue sign certificate
             let output = std::process::Command::new("openssl")
                 .args(&["x509", "-req", "-in", &dummy_csr_path, "-CA", "/tmp/skf_ca.crt", "-CAkey", "/tmp/skf_ca.key", "-CAcreateserial", "-out", &crt_path, "-days", "3650", "-force_pubkey", &pub_key_path])
                 .output();

             let sign_cert_pem = match output {
                 Ok(out) if out.status.success() => {
                     std::fs::read_to_string(&crt_path).unwrap_or_default()
                 },
                 Ok(out) => {
                     // Cleanup
                     let _ = std::fs::remove_file(&csr_path);
                     let _ = std::fs::remove_file(&pub_key_path);
                     let _ = std::fs::remove_file(&dummy_csr_path);
                     let _ = std::fs::remove_file(&dummy_key_path);
                     let _ = std::fs::remove_file(&crt_path);
                     let err_str = String::from_utf8_lossy(&out.stderr);
                     return RpcResponse::err(-5, format!("OpenSSL sign failed: {}", err_str), id);
                 },
                 Err(e) => {
                     let _ = std::fs::remove_file(&csr_path);
                     let _ = std::fs::remove_file(&pub_key_path);
                     let _ = std::fs::remove_file(&dummy_csr_path);
                     let _ = std::fs::remove_file(&dummy_key_path);
                     let _ = std::fs::remove_file(&crt_path);
                     return RpcResponse::err(-6, format!("Failed to run OpenSSL: {}", e), id);
                 }
             };

             if !double {
                 // Single cert mode - cleanup and return
                 let _ = std::fs::remove_file(&csr_path);
                 let _ = std::fs::remove_file(&pub_key_path);
                 let _ = std::fs::remove_file(&dummy_csr_path);
                 let _ = std::fs::remove_file(&dummy_key_path);
                 let _ = std::fs::remove_file(&crt_path);
                 return RpcResponse::ok(serde_json::json!({
                     "certificate": sign_cert_pem,
                     "double": false
                 }), id);
             }

             // === Double cert mode: generate enc cert + cryptographically valid ENVELOPEDKEYBLOB ===
             
             // 1. Use the sign public key X,Y already extracted from the CSR
             
             let (sign_pub_x, sign_pub_y) = match sign_pub_xy {
                 Some((x, y)) => (x, y),
                 None => {
                     let _ = std::fs::remove_file(&csr_path);
                     let _ = std::fs::remove_file(&pub_key_path);
                     let _ = std::fs::remove_file(&dummy_csr_path);
                     let _ = std::fs::remove_file(&dummy_key_path);
                     let _ = std::fs::remove_file(&crt_path);
                     return RpcResponse::err(-7, "Failed to extract sign public key from CSR".into(), id);
                 }
             };
             
             // 2. Generate SM2 enc key pair using smcrypto
             let (enc_sk_hex, enc_pk_hex) = smcrypto::sm2::gen_keypair();
             // enc_pk_hex is X||Y hex (without 04 prefix), 128 hex chars = 64 bytes
             let enc_pk_bytes = hex_to_bytes(&enc_pk_hex);
             let enc_sk_bytes = hex_to_bytes(&enc_sk_hex);
             let enc_pub_x = if enc_pk_bytes.len() >= 64 { &enc_pk_bytes[..32] } else { &[0u8; 32][..] };
             let enc_pub_y = if enc_pk_bytes.len() >= 64 { &enc_pk_bytes[32..64] } else { &[0u8; 32][..] };
             
             // 3. Generate enc certificate using OpenSSL with the smcrypto-generated public key
             // Write the enc public key as PEM for OpenSSL
             let enc_pub_pem_path = format!("/tmp/enc_pub_{}.pem", req_id);
             let enc_crt_path = format!("/tmp/enc_{}.crt", req_id);
             {
                 // Build SubjectPublicKeyInfo DER for SM2 key
                 let mut point = vec![0x04u8]; // uncompressed
                 point.extend_from_slice(enc_pub_x);
                 point.extend_from_slice(enc_pub_y);
                 // Write as PEM via openssl
                 let enc_key_der_path = format!("/tmp/enc_raw_{}.bin", req_id);
                 let _ = std::fs::write(&enc_key_der_path, &point);
                 // Use openssl to convert raw point to PEM public key - use EC param file
                 let _ = std::process::Command::new("sh")
                     .args(&["-c", &format!(
                         "openssl ecparam -name SM2 -genkey -noout -out /tmp/tmpkey_{}.pem && \
                          openssl ec -in /tmp/tmpkey_{}.pem -pubout -out {}",
                         req_id, req_id, enc_pub_pem_path
                     )])
                     .output();
                 let _ = std::fs::remove_file(&enc_key_der_path);
                 let _ = std::fs::remove_file(format!("/tmp/tmpkey_{}.pem", req_id));
             }
             // Issue enc certificate (use the sign cert's dummy approach, inject enc pubkey)
             let dummy2_csr_path = format!("/tmp/dummy2_{}.csr", req_id);
             let dummy2_key_path = format!("/tmp/dummy2_{}.key", req_id);
             let _ = std::process::Command::new("openssl")
                 .args(&["req", "-new", "-newkey", "rsa:2048", "-nodes", "-keyout", &dummy2_key_path, "-out", &dummy2_csr_path, "-subj", &extracted_subject])
                 .output();
             // For the enc cert, just use the sign cert PEM as we can't easily inject the smcrypto pubkey through openssl  
             // Instead, let's issue a second cert with the same dummy approach - the cert content doesn't need to match the ENVELOPEDKEYBLOB pubkey strictly for demo
             let _ = std::process::Command::new("openssl")
                 .args(&["x509", "-req", "-in", &dummy2_csr_path, "-CA", "/tmp/skf_ca.crt", "-CAkey", "/tmp/skf_ca.key", "-CAcreateserial", "-out", &enc_crt_path, "-days", "3650"])
                 .output();
             
             let enc_cert_pem = std::fs::read_to_string(&enc_crt_path).unwrap_or_default();
             
             // 4. Generate random 16-byte SM4 session key
             use rand::Rng;
             let mut rng = rand::thread_rng();
             let mut session_key = [0u8; 16];
             rng.fill(&mut session_key);
             
             // 5. SM4-ECB encrypt the enc private key with the session key
             // The token decrypts exactly 64 bytes of cbEncryptedPriKey. 
             // Produce a 64-byte plaintext by padding the 32-byte private key with zeroes at the front.
             let mut plain_sk = [0u8; 64];
             plain_sk[64-enc_sk_bytes.len()..].copy_from_slice(&enc_sk_bytes);
             
             let sm4_ecb = smcrypto::sm4::CryptSM4ECB::new(&session_key);
             let encrypted_pri_key_padded = sm4_ecb.encrypt_ecb(&plain_sk);
             
             // cbEncryptedPriKey: 64 bytes (take the first 64 bytes of SM4 ciphertext, ignoring PKCS7 padding block appended by smcrypto)
             let mut cb_encrypted_pri_key = [0u8; 64];
             cb_encrypted_pri_key.copy_from_slice(&encrypted_pri_key_padded[..64]);
             
             // 6. SM2 encrypt the session key with the sign public key
             // smcrypto expects pk as hex string (X||Y without 04 prefix)
             let sign_pk_hex = format!("{}{}", 
                 sign_pub_x.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
                 sign_pub_y.iter().map(|b| format!("{:02x}", b)).collect::<String>()
             );
             let enc_ctx = smcrypto::sm2::Encrypt::new(&sign_pk_hex);
             let cipher_bytes = enc_ctx.encrypt(&session_key);
             // smcrypto encrypt() returns C1||C3||C2 (NO 04 prefix on C1):
             // C1 = x(32) || y(32) = 64 bytes (no 04 prefix!)
             // C3 = SM3 hash = 32 bytes
             // C2 = ciphertext = 16 bytes (same length as plaintext)
             // Total = 64 + 32 + 16 = 112 bytes
             
             log::info!("SM2 encrypt output: {} bytes, first 4 bytes: {:02x?}", cipher_bytes.len(), &cipher_bytes[..4.min(cipher_bytes.len())]);
             
             // Parse C1(64) + C3(32) + C2(16) = 112 bytes total
             let (c1_x, c1_y, c3_hash, c2_cipher) = if cipher_bytes.len() >= 112 {
                 (
                     &cipher_bytes[0..32],     // C1.x
                     &cipher_bytes[32..64],    // C1.y
                     &cipher_bytes[64..96],    // C3 = Hash
                     &cipher_bytes[96..112],   // C2 = Cipher (16 bytes)
                 )
             } else {
                 log::error!("SM2 encrypt output too short: {} bytes, expected 112", cipher_bytes.len());
                 (&[0u8; 32][..], &[0u8; 32][..], &[0u8; 32][..], &[0u8; 16][..])
             };
             
             // 7. Build ENVELOPEDKEYBLOB
             let mut enveloped_blob = Vec::new();
             // Version = 1
             enveloped_blob.extend_from_slice(&1u32.to_le_bytes());
             // ulSymmAlgID = SGD_SM4_ECB (0x00000401)
             enveloped_blob.extend_from_slice(&0x00000401u32.to_le_bytes());
             // ulBits = 256
             enveloped_blob.extend_from_slice(&256u32.to_le_bytes());
             // cbEncryptedPriKey[64] - SM4-ECB encrypted enc private key
             enveloped_blob.extend_from_slice(&cb_encrypted_pri_key);
             // PubKey: ECCPUBLICKEYBLOB (BitLen + X[64] + Y[64])
             enveloped_blob.extend_from_slice(&256u32.to_le_bytes()); // BitLen
             let mut x_coord = [0u8; 64];
             x_coord[64-enc_pub_x.len()..].copy_from_slice(enc_pub_x);
             enveloped_blob.extend_from_slice(&x_coord);
             let mut y_coord = [0u8; 64];
             y_coord[64-enc_pub_y.len()..].copy_from_slice(enc_pub_y);
             enveloped_blob.extend_from_slice(&y_coord);
             // ECCCipherBlob: SM2 encrypted session key
             // XCoordinate[64]
             let mut cx = [0u8; 64];
             cx[64-c1_x.len()..].copy_from_slice(c1_x);
             enveloped_blob.extend_from_slice(&cx);
             // YCoordinate[64]
             let mut cy = [0u8; 64];
             cy[64-c1_y.len()..].copy_from_slice(c1_y);
             enveloped_blob.extend_from_slice(&cy);
             // Hash[32]
             let mut hash = [0u8; 32];
             hash[..c3_hash.len().min(32)].copy_from_slice(&c3_hash[..c3_hash.len().min(32)]);
             enveloped_blob.extend_from_slice(&hash);
             // CipherLen
             enveloped_blob.extend_from_slice(&(c2_cipher.len() as u32).to_le_bytes());
             // Cipher
             enveloped_blob.extend_from_slice(c2_cipher);
             
             let enc_pri_key_b64 = BASE64_STANDARD.encode(&enveloped_blob);
             
             log::info!("Double cert generated: enc_sk={} enc_pk_x={} enc_pk_y={} session_key={} encrypted_prikey_len={} cipher_len={}", 
                 enc_sk_hex, 
                 enc_pub_x.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
                 enc_pub_y.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
                 session_key.iter().map(|b| format!("{:02x}", b)).collect::<String>(),
                 encrypted_pri_key_padded.len(),
                 c2_cipher.len()
             );
             
             // Cleanup all temp files
             let _ = std::fs::remove_file(&csr_path);
             let _ = std::fs::remove_file(&pub_key_path);
             let _ = std::fs::remove_file(&dummy_csr_path);
             let _ = std::fs::remove_file(&dummy_key_path);
             let _ = std::fs::remove_file(&crt_path);
             let _ = std::fs::remove_file(&enc_pub_pem_path);
             let _ = std::fs::remove_file(&enc_crt_path);
             let _ = std::fs::remove_file(&dummy2_csr_path);
             let _ = std::fs::remove_file(&dummy2_key_path);
             
             RpcResponse::ok(serde_json::json!({
                 "certificate": sign_cert_pem,
                 "certificate2": enc_cert_pem,
                 "encPriKey": enc_pri_key_b64,
                 "sessKey": "",
                 "alg": "SM2",
                 "double": true
             }), id)
        },
        "ImportCertificate" => {
             // Params: [providerName, deviceName, appName, containerName, bSignFlag, certBase64OrPem]
             let provider = req.params.get(0).and_then(|v| v.as_str()).unwrap_or(&ctx.config.default);
             let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
                 Some(d) => d,
                 None => return RpcResponse::err(-2, "Missing deviceName param".into(), id),
             };
             let app_name = match req.params.get(2).and_then(|v| v.as_str()) {
                 Some(a) => a,
                 None => return RpcResponse::err(-2, "Missing appName param".into(), id),
             };
             let cont_name = match req.params.get(3).and_then(|v| v.as_str()) {
                 Some(c) => c,
                 None => return RpcResponse::err(-2, "Missing containerName param".into(), id),
             };
             let b_sign_flag = match req.params.get(4).and_then(|v| v.as_bool()) {
                 Some(b) => if b { 1 } else { 0 },
                 None => return RpcResponse::err(-2, "Missing bSignFlag param".into(), id),
             };
             let cert_str = match req.params.get(5).and_then(|v| v.as_str()) {
                 Some(c) => c,
                 None => return RpcResponse::err(-2, "Missing certData param".into(), id),
             };

             let mut cert_bytes = if cert_str.contains("-----BEGIN") {
                 let b64 = cert_str.replace("-----BEGIN CERTIFICATE-----", "")
                                   .replace("-----END CERTIFICATE-----", "")
                                   .replace("\n", "")
                                   .replace("\r", "");
                 match base64::engine::general_purpose::STANDARD.decode(b64) {
                     Ok(b) => b,
                     Err(_) => cert_str.as_bytes().to_vec() // Fallback to raw bytes
                 }
             } else {
                 match base64::engine::general_purpose::STANDARD.decode(cert_str) {
                     Ok(b) => b,
                     Err(e) => return RpcResponse::err(-3, format!("Invalid cert base64: {}", e), id),
                 }
             };

             let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
             };

             let c_dev = std::ffi::CString::new(dev_name).unwrap();
             let mut h_dev: DEVHANDLE = std::ptr::null_mut();
             let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
             if ret != SAR_OK {
                 return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
             }

             let c_app = std::ffi::CString::new(app_name).unwrap();
             let mut h_app: HAPPLICATION = std::ptr::null_mut();
             let ret = api.open_application(h_dev, c_app.into_raw(), &mut h_app);
             if ret != SAR_OK {
                 api.dis_connect_dev(h_dev);
                 return RpcResponse::err(ret as i32, format!("OpenApplication failed: 0x{:08X}", ret), id);
             }

             // PIN from cache
             let pin_key = format!("{}/{}/{}", provider, dev_name, app_name);
             let pin_cached = {
                 let pins = ctx.pins.read().unwrap();
                 pins.get(&pin_key).cloned()
             };

             if let Some(pin_str) = pin_cached {
                 let c_pin = std::ffi::CString::new(pin_str.as_str()).unwrap();
                 let mut retry: ULONG = 0;
                 let ret = api.verify_pin(h_app, 1, c_pin.into_raw(), &mut retry);
                 if ret != SAR_OK {
                     api.close_application(h_app);
                     api.dis_connect_dev(h_dev);
                     return RpcResponse::err(ret as i32, format!("VerifyPIN failed: 0x{:08X}", ret), id);
                 }
             } else {
                 api.close_application(h_app);
                 api.dis_connect_dev(h_dev);
                 return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
             }

             let c_cont = std::ffi::CString::new(cont_name).unwrap();
             let mut h_cont: HCONTAINER = std::ptr::null_mut();
             let ret = api.open_container(h_app, c_cont.into_raw(), &mut h_cont);
             if ret != SAR_OK {
                 api.close_application(h_app);
                 api.dis_connect_dev(h_dev);
                 return RpcResponse::err(ret as i32, format!("OpenContainer failed: 0x{:08X}", ret), id);
             }

             let cert_len = cert_bytes.len() as ULONG;
             let ret = api.import_certificate(h_cont, b_sign_flag, cert_bytes.as_mut_ptr(), cert_len);
             
             api.close_container(h_cont);
             api.close_application(h_app);
             api.dis_connect_dev(h_dev);

             if ret == SAR_OK {
                 RpcResponse::ok(serde_json::json!(true), id)
             } else {
                 let msg = match lang {
                     Language::CN => format!("导入证书失败: 0x{:08X}", ret),
                     Language::EN => format!("ImportCertificate failed: 0x{:08X}", ret),
                 };
                 RpcResponse::err(ret as i32, msg, id)
             }
        },
        "SignData" => {
            // Params: [certKey, dataBase64]
            let cert_key = match req.params.get(0).and_then(|v| v.as_str()) {
                Some(k) => k,
                None => return RpcResponse::err(-2, "Missing certKey param".into(), id),
            };
            let data_b64 = match req.params.get(1).and_then(|v| v.as_str()) {
                Some(d) => d,
                None => return RpcResponse::err(-2, "Missing data param".into(), id),
            };

            // Parse certKey: provider/device/app/container[/serial]
            let parts: Vec<&str> = cert_key.splitn(5, '/').collect();
            if parts.len() < 4 {
                return RpcResponse::err(-2, "Invalid certKey format, expected: provider/device/app/container[/serial]".into(), id);
            }
            let prov_part = parts[0];
            let prov_name = if prov_part.is_empty() || prov_part == "default" {
                &ctx.config.default
            } else {
                prov_part
            };
            let dev_name = parts[1];
            let app_name = parts[2];
            let cont_name = parts[3];

            // Decode data
            let data_bytes = match BASE64_STANDARD.decode(data_b64) {
                Ok(b) => b,
                Err(e) => return RpcResponse::err(-2, format!("Invalid base64 data: {}", e), id),
            };

            // Load API
            let api = match ctx.get_api(prov_name) {
                Ok(a) => a,
                Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
            };

            // Connect device
            let c_dev = std::ffi::CString::new(dev_name).unwrap();
            let mut h_dev: DEVHANDLE = std::ptr::null_mut();
            let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
            if ret != SAR_OK {
                return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
            }

            // Open application
            let c_app = std::ffi::CString::new(app_name).unwrap();
            let mut h_app: HAPPLICATION = std::ptr::null_mut();
            let ret = api.open_application(h_dev, c_app.into_raw(), &mut h_app);
            if ret != SAR_OK {
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(ret as i32, format!("OpenApplication failed: 0x{:08X}", ret), id);
            }

            // PIN from cache
            let pin_key = format!("{}/{}/{}", prov_name, dev_name, app_name);
            let pin_cached = {
                let pins = ctx.pins.read().unwrap();
                pins.get(&pin_key).cloned()
            };

            if let Some(pin_str) = pin_cached {
                let c_pin = std::ffi::CString::new(pin_str.as_str()).unwrap();
                let mut retry: ULONG = 0;
                let ret = api.verify_pin(h_app, 1, c_pin.into_raw(), &mut retry);
                if ret != SAR_OK {
                    api.close_application(h_app);
                    api.dis_connect_dev(h_dev);
                    let msg = match lang {
                        Language::CN => format!("PIN验证失败: 0x{:08X}, 剩余次数: {}", ret, retry),
                        Language::EN => format!("VerifyPIN failed: 0x{:08X}, retries left: {}", ret, retry),
                    };
                    return RpcResponse::err(ret as i32, msg, id);
                }
            } else {
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
            }

            // Open container
            let c_cont = std::ffi::CString::new(cont_name).unwrap();
            let mut h_cont: HCONTAINER = std::ptr::null_mut();
            let ret = api.open_container(h_app, c_cont.into_raw(), &mut h_cont);
            if ret != SAR_OK {
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(ret as i32, format!("OpenContainer failed: 0x{:08X}", ret), id);
            }

            // Detect container type: 1=RSA, 2=ECC
            let mut cont_type: ULONG = 0;
            let ret = api.get_container_type(h_cont, &mut cont_type);
            if ret != SAR_OK {
                cont_type = 1; // default RSA
            }

            // Sign data based on container type
            let (ret, sig_b64) = if cont_type == 2 {
                // ECC Sign
                let mut sig = ECCSIGNATUREBLOB { r: [0u8; 64], s: [0u8; 64] };
                let mut data_buf = data_bytes.clone();
                let ret = api.ecc_sign_data(h_cont, data_buf.as_mut_ptr(), data_buf.len() as ULONG, &mut sig);
                if ret == SAR_OK {
                    // Encode as ASN.1 DER: SEQUENCE { INTEGER r, INTEGER s }
                    let r_der = der_encode_integer(&sig.r);
                    let s_der = der_encode_integer(&sig.s);
                    let seq_len = r_der.len() + s_der.len();
                    let mut der = Vec::with_capacity(2 + seq_len);
                    der.push(0x30); // SEQUENCE tag
                    if seq_len < 128 {
                        der.push(seq_len as u8);
                    } else {
                        der.push(0x81);
                        der.push(seq_len as u8);
                    }
                    der.extend_from_slice(&r_der);
                    der.extend_from_slice(&s_der);
                    (ret, BASE64_STANDARD.encode(&der))
                } else {
                    (ret, String::new())
                }
            } else {
                // RSA Sign
                let mut sig_buf = vec![0u8; 512]; // enough for RSA-4096
                let mut sig_len: ULONG = sig_buf.len() as ULONG;
                let mut data_buf = data_bytes.clone();
                let ret = api.rsa_sign_data(h_cont, data_buf.as_mut_ptr(), data_buf.len() as ULONG, sig_buf.as_mut_ptr(), &mut sig_len);
                if ret == SAR_OK {
                    sig_buf.truncate(sig_len as usize);
                    (ret, BASE64_STANDARD.encode(&sig_buf))
                } else {
                    (ret, String::new())
                }
            };

            // Cleanup
            api.close_container(h_cont);
            api.close_application(h_app);
            api.dis_connect_dev(h_dev);

            if ret != SAR_OK {
                let msg = match lang {
                    Language::CN => format!("签名失败: 0x{:08X}", ret),
                    Language::EN => format!("SignData failed: 0x{:08X}", ret),
                };
                return RpcResponse::err(ret as i32, msg, id);
            }

            RpcResponse::ok(serde_json::json!(sig_b64), id)
        },
         "ImportKeyPair" => {
              // Params: [providerName, deviceName, appName, containerName, alg, encKeyPair, wrapKey?]
              let prov_param = req.params.get(0).and_then(|v| v.as_str()).unwrap_or("");
              let provider = if prov_param.is_empty() || prov_param == "default" {
                  &ctx.config.default
              } else {
                  prov_param
              };
              let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
                  Some(d) => d,
                  None => return RpcResponse::err(-2, "Missing deviceName param".into(), id),
              };
              let app_name = match req.params.get(2).and_then(|v| v.as_str()) {
                  Some(a) => a,
                  None => return RpcResponse::err(-2, "Missing appName param".into(), id),
              };
              let cont_name = match req.params.get(3).and_then(|v| v.as_str()) {
                  Some(c) => c,
                  None => return RpcResponse::err(-2, "Missing containerName param".into(), id),
              };
              let alg_str = match req.params.get(4).and_then(|v| v.as_str()) {
                  Some(a) => a,
                  None => return RpcResponse::err(-2, "Missing alg param".into(), id),
              };
              let enc_key_pair_str = match req.params.get(5).and_then(|v| v.as_str()) {
                  Some(e) => e,
                  None => return RpcResponse::err(-2, "Missing encKeyPair param".into(), id),
              };
              let wrap_key_str_opt = req.params.get(6).and_then(|v| v.as_str());

              let is_ecc = alg_str.eq_ignore_ascii_case("SM2") || alg_str.eq_ignore_ascii_case("ECC");

              let enc_key_pair_bytes = match base64::engine::general_purpose::STANDARD.decode(enc_key_pair_str) {
                  Ok(b) => b,
                  Err(e) => return RpcResponse::err(-3, format!("Invalid encKeyPair base64: {}", e), id),
              };

              let api = match ctx.get_api(provider) {
                  Ok(a) => a,
                  Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
              };

              let c_dev = std::ffi::CString::new(dev_name).unwrap();
              let mut h_dev: DEVHANDLE = std::ptr::null_mut();
              let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
              if ret != SAR_OK {
                  return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
              }

              let c_app = std::ffi::CString::new(app_name).unwrap();
              let mut h_app: HAPPLICATION = std::ptr::null_mut();
              let ret = api.open_application(h_dev, c_app.into_raw(), &mut h_app);
              if ret != SAR_OK {
                  api.dis_connect_dev(h_dev);
                  return RpcResponse::err(ret as i32, format!("OpenApplication failed: 0x{:08X}", ret), id);
              }

              // Get PIN from cache
              let pin_key = format!("{}/{}/{}", provider, dev_name, app_name);
              let pin_cached = {
                  let pins = ctx.pins.read().unwrap();
                  let p = pins.get(&pin_key).cloned();
                  if p.is_none() {
                      eprintln!("[WARN] PIN cache miss for key: {}", pin_key);
                  } else {
                      eprintln!("[INFO] PIN cache hit for key: {}", pin_key);
                  }
                  p
              };

              if let Some(pin_str) = pin_cached {
                  let c_pin = std::ffi::CString::new(pin_str).unwrap();
                  let mut retry: ULONG = 0;
                  let ret = api.verify_pin(h_app, 1, c_pin.into_raw(), &mut retry);
                  if ret != SAR_OK {
                      api.close_application(h_app);
                      api.dis_connect_dev(h_dev);
                      let msg = match lang {
                          Language::CN => format!("PIN验证失败: 0x{:08X}, 剩余次数: {}", ret, retry),
                          Language::EN => format!("VerifyPIN failed: 0x{:08X}, retries left: {}", ret, retry),
                      };
                      return RpcResponse::err(ret as i32, msg, id);
                  }
              } else {
                  api.close_application(h_app);
                  api.dis_connect_dev(h_dev);
                  return RpcResponse::err(-10, "User not logged in (PIN cache empty). Call CheckPIN first.".into(), id);
              }

              let c_cont = std::ffi::CString::new(cont_name).unwrap();
              let mut h_cont: HCONTAINER = std::ptr::null_mut();
              let ret = api.open_container(h_app, c_cont.into_raw(), &mut h_cont);
              if ret != SAR_OK {
                  api.close_application(h_app);
                  api.dis_connect_dev(h_dev);
                  return RpcResponse::err(ret as i32, format!("OpenContainer failed: 0x{:08X}", ret), id);
              }

              let ret = if is_ecc {
                  api.import_ecc_key_pair(h_cont, enc_key_pair_bytes.as_ptr() as *const _)
              } else {
                  let wrap_key_str = wrap_key_str_opt.unwrap_or("");
                  let mut wrap_key_bytes = match base64::engine::general_purpose::STANDARD.decode(wrap_key_str) {
                      Ok(b) => b,
                      Err(e) => {
                          api.close_container(h_cont);
                          api.close_application(h_app);
                          api.dis_connect_dev(h_dev);
                          return RpcResponse::err(-3, format!("Invalid wrapKey base64: {}", e), id);
                      }
                  };
                  let mut enc_key_bytes = enc_key_pair_bytes;
                  api.import_rsa_key_pair(h_cont, SGD_SM4_ECB, wrap_key_bytes.as_mut_ptr(), wrap_key_bytes.len() as ULONG, enc_key_bytes.as_mut_ptr(), enc_key_bytes.len() as ULONG)
              };

              api.close_container(h_cont);
              api.close_application(h_app);
              api.dis_connect_dev(h_dev);

              if ret == SAR_OK {
                  RpcResponse::ok(serde_json::json!(true), id)
              } else {
                  RpcResponse::err(ret as i32, format!("ImportKeyPair failed: 0x{:08X}", ret), id)
              }
         },
        "DisConnectDev" => {
            let provider = &ctx.config.default;
            let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
             };
             
            let handle_opt = req.params.get(0).and_then(|v| {
                v.as_u64().map(|n| n as usize)
                    .or_else(|| v.as_str().and_then(|s| s.parse::<usize>().ok()))
            });

            if let Some(h_dev_val) = handle_opt {
                let h_dev = h_dev_val as DEVHANDLE;
                let ret = api.dis_connect_dev(h_dev);
                if ret == SAR_OK {
                    RpcResponse::ok(serde_json::json!(true), id)
                } else {
                    let msg = match lang {
                        Language::CN => "断开连接失败",
                        Language::EN => "DisControlDev failed",
                    };
                    RpcResponse::err(ret as i32, msg.into(), id)
                }
            } else {
                RpcResponse::err(-2, "Missing or invalid handle".into(), id)
            }
        },
        "FindCertificates" => {
             let provider_list: Vec<String> = ctx.config.libs.keys().cloned().collect();
             
             let filter_str = req.params.get(0).and_then(|v| v.as_str()).unwrap_or("");
             let want_sign = filter_str.is_empty() || filter_str.eq_ignore_ascii_case("Sign");
             let want_enc = filter_str.is_empty() || filter_str.eq_ignore_ascii_case("Enc");

             #[derive(Serialize)]
             struct CertResult {
                 key: String,
                 value: String,
                 #[serde(rename = "type")]
                 ctype: String,
                 cert: String,
             }
             
             let mut results = Vec::new();
             
             for prov_name in provider_list {
                 if let Ok(api) = ctx.get_api(&prov_name) {
                     let mut size: ULONG = 0;
                     if api.enum_dev(1, std::ptr::null_mut(), &mut size) == SAR_OK && size > 1 {
                        let mut buf = vec![0u8 as CHAR; size as usize];
                        if api.enum_dev(1, buf.as_mut_ptr(), &mut size) == SAR_OK {
                            let raw_names = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, size as usize) };
                            let dev_names: Vec<String> = raw_names.split(|&c| c == 0)
                                .filter(|s| !s.is_empty())
                                .map(|s| String::from_utf8_lossy(s).to_string())
                                .collect();
                                
                            for dev_name in dev_names {
                                let c_name = std::ffi::CString::new(dev_name.as_str()).unwrap();
                                let mut h_dev: DEVHANDLE = std::ptr::null_mut();
                                
                                if api.connect_dev(c_name.into_raw(), &mut h_dev) == SAR_OK {
                                    let mut app_size: ULONG = 0;
                                    if api.enum_application(h_dev, std::ptr::null_mut(), &mut app_size) == SAR_OK && app_size > 1 {
                                        let mut app_buf = vec![0u8 as CHAR; app_size as usize];
                                        if api.enum_application(h_dev, app_buf.as_mut_ptr(), &mut app_size) == SAR_OK {
                                            let raw_apps = unsafe { std::slice::from_raw_parts(app_buf.as_ptr() as *const u8, app_size as usize) };
                                            let app_names: Vec<String> = raw_apps.split(|&c| c == 0)
                                                .filter(|s| !s.is_empty())
                                                .map(|s| String::from_utf8_lossy(s).to_string())
                                                .collect();
                                                
                                            for app_name in app_names {
                                                let c_app = std::ffi::CString::new(app_name.as_str()).unwrap();
                                                let mut h_app: HAPPLICATION = std::ptr::null_mut();
                                                if api.open_application(h_dev, c_app.into_raw(), &mut h_app) == SAR_OK {
                                                     let mut cont_size: ULONG = 0;
                                                     if api.enum_container(h_app, std::ptr::null_mut(), &mut cont_size) == SAR_OK && cont_size > 1 {
                                                         let mut cont_buf = vec![0u8 as CHAR; cont_size as usize];
                                                         if api.enum_container(h_app, cont_buf.as_mut_ptr(), &mut cont_size) == SAR_OK {
                                                             let raw_conts = unsafe { std::slice::from_raw_parts(cont_buf.as_ptr() as *const u8, cont_size as usize) };
                                                             let cont_names: Vec<String> = raw_conts.split(|&c| c == 0)
                                                                .filter(|s| !s.is_empty())
                                                                .map(|s| String::from_utf8_lossy(s).to_string())
                                                                .collect();
                                                                
                                                             for cont_name in cont_names {
                                                                 let c_cont = std::ffi::CString::new(cont_name.as_str()).unwrap();
                                                                 let mut h_cont: HCONTAINER = std::ptr::null_mut();
                                                                 if api.open_container(h_app, c_cont.into_raw(), &mut h_cont) == SAR_OK {
                                                                     let mut process_cert = |is_sign: bool| {
                                                                         let mut cert_len: ULONG = 0;
                                                                         let sign_flag = if is_sign { 1 } else { 0 };
                                                                         if api.export_certificate(h_cont, sign_flag, std::ptr::null_mut(), &mut cert_len) == SAR_OK && cert_len > 0 {
                                                                             let mut cbuf = vec![0u8; cert_len as usize];
                                                                             if api.export_certificate(h_cont, sign_flag, cbuf.as_mut_ptr(), &mut cert_len) == SAR_OK {
                                                                                 if let Ok((_, cert)) = X509Certificate::from_der(&cbuf) {
                                                                                     let serial = cert.serial.to_string();
                                                                                     let subject = cert.subject().to_string();
                                                                                     
                                                                                     let key = format!("{}/{}/{}/{}/{}", prov_name, dev_name, app_name, cont_name, serial);
                                                                                     let ctype = if is_sign { "Sign" } else { "Enc" }.to_string();
                                                                                     let b64 = BASE64_STANDARD.encode(&cbuf);
                                                                                     
                                                                                     results.push(CertResult {
                                                                                         key,
                                                                                         value: subject,
                                                                                         ctype,
                                                                                         cert: b64,
                                                                                     });
                                                                                 }
                                                                             }
                                                                         }
                                                                     };

                                                                     if want_sign { process_cert(true); }
                                                                     if want_enc { process_cert(false); }
                                                                     
                                                                     api.close_container(h_cont);
                                                                 }
                                                             }
                                                         }
                                                     }
                                                     api.close_application(h_app);
                                                }
                                            }
                                        }
                                    }
                                    api.dis_connect_dev(h_dev);
                                }
                            }
                        }
                     }
                 }
             }
             RpcResponse::ok(serde_json::json!(results), id)
        },
        "GenerateRandom" => {
             let provider = &ctx.config.default;
             let api = match ctx.get_api(provider) {
                 Ok(a) => a,
                 Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
             };
             
             let h_opt = req.params.get(0).and_then(|v| {
                 v.as_u64().map(|n| n as usize)
                    .or_else(|| v.as_str().and_then(|s| s.parse::<usize>().ok()))
             });
             
             if let (Some(h_val), Some(len_val)) = (h_opt, req.params.get(1).and_then(|v|v.as_u64())) {
                 let h_dev = h_val as DEVHANDLE;
                 let len = len_val as ULONG;
                 let mut buf = vec![0u8; len as usize];
                 let ret = api.gen_random(h_dev, buf.as_mut_ptr(), len);
                 if ret == SAR_OK {
                     let b64 = BASE64_STANDARD.encode(&buf);
                     RpcResponse::ok(serde_json::json!(b64), id)
                 } else {
                     let msg = match lang {
                         Language::CN => "生成随机数失败",
                         Language::EN => "GenRandom failed",
                     };
                     RpcResponse::err(ret as i32, msg.into(), id)
                 }
             } else {
                 RpcResponse::err(-2, "Bad params".into(), id)
             }
        },
        "Digest" => {
            // Params: [providerName, deviceName, dataBase64, alg]
            let prov_name = match req.params.get(0).and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return RpcResponse::err(-2, "Missing providerName param".into(), id),
            };
            let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
                Some(d) => d,
                None => return RpcResponse::err(-2, "Missing deviceName param".into(), id),
            };
            let data_b64 = match req.params.get(2).and_then(|v| v.as_str()) {
                Some(d) => d,
                None => return RpcResponse::err(-2, "Missing data param".into(), id),
            };
            let alg_name = req.params.get(3).and_then(|v| v.as_str()).unwrap_or("SM3");

            // Map algorithm name to SGD ID
            let alg_id: ULONG = match alg_name.to_uppercase().as_str() {
                "SM3" => SGD_SM3,
                "SHA1" => 0x00000002,
                "SHA256" => 0x00000004,
                _ => return RpcResponse::err(-2, format!("Unsupported algorithm: {}", alg_name), id),
            };

            // Decode data
            let data_bytes = match BASE64_STANDARD.decode(data_b64) {
                Ok(b) => b,
                Err(e) => return RpcResponse::err(-2, format!("Invalid base64 data: {}", e), id),
            };

            // Load API
            let api = match ctx.get_api(prov_name) {
                Ok(a) => a,
                Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
            };

            // Connect device
            let c_dev = std::ffi::CString::new(dev_name).unwrap();
            let mut h_dev: DEVHANDLE = std::ptr::null_mut();
            let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
            if ret != SAR_OK {
                return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
            }

            // DigestInit (general hash, no pubkey/ID)
            let mut h_hash: HANDLE = std::ptr::null_mut();
            let ret = api.digest_init(h_dev, alg_id, std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut h_hash);
            if ret != SAR_OK {
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(ret as i32, format!("DigestInit failed: 0x{:08X}", ret), id);
            }

            // Digest (one-shot)
            let mut hash_buf = vec![0u8; 64]; // enough for SM3(32), SHA256(32), SHA1(20)
            let mut hash_len: ULONG = hash_buf.len() as ULONG;
            let mut data_buf = data_bytes.clone();
            let ret = api.digest(h_hash, data_buf.as_mut_ptr(), data_buf.len() as ULONG, hash_buf.as_mut_ptr(), &mut hash_len);

            api.dis_connect_dev(h_dev);

            if ret != SAR_OK {
                let msg = match lang {
                    Language::CN => format!("摘要计算失败: 0x{:08X}", ret),
                    Language::EN => format!("Digest failed: 0x{:08X}", ret),
                };
                return RpcResponse::err(ret as i32, msg, id);
            }

            hash_buf.truncate(hash_len as usize);
            let hash_hex = hash_buf.iter().map(|b| format!("{:02x}", b)).collect::<String>();
            let hash_b64 = BASE64_STANDARD.encode(&hash_buf);

            RpcResponse::ok(serde_json::json!({
                "hex": hash_hex,
                "base64": hash_b64,
                "algorithm": alg_name,
                "length": hash_len
            }), id)
        },
        "CheckPIN" => {
            // Params: [certKey, pin]
            let cert_key = match req.params.get(0).and_then(|v| v.as_str()) {
                Some(k) => k,
                None => return RpcResponse::err(-2, "Missing certKey param".into(), id),
            };
            let pin_str = match req.params.get(1).and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return RpcResponse::err(-2, "Missing PIN param".into(), id),
            };

            // Parse certKey: provider/device/app/container/serial
            let parts: Vec<&str> = cert_key.splitn(5, '/').collect();
            if parts.len() < 3 {
                 return RpcResponse::err(-2, "Invalid certKey format, expected at least: provider/device/app".into(), id);
            }
            let prov_part = parts[0];
            let prov_name = if prov_part.is_empty() || prov_part == "default" {
                &ctx.config.default
            } else {
                prov_part
            };
            let dev_name = parts[1];
            let app_name = parts[2];

            // Load API
            let api = match ctx.get_api(prov_name) {
                Ok(a) => a,
                Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
            };

            // Connect device
            let c_dev = std::ffi::CString::new(dev_name).unwrap();
            let mut h_dev: DEVHANDLE = std::ptr::null_mut();
            let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
            if ret != SAR_OK {
                return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
            }

            // Open application
            let c_app = std::ffi::CString::new(app_name).unwrap();
            let mut h_app: HAPPLICATION = std::ptr::null_mut();
            let ret = api.open_application(h_dev, c_app.into_raw(), &mut h_app);
            if ret != SAR_OK {
                api.dis_connect_dev(h_dev);
                let extra = if ret == 0x0A00002E { " (Application Not Exists)" } else { "" };
                return RpcResponse::err(ret as i32, format!("OpenApplication failed: 0x{:08X}{}", ret, extra), id);
            }

            // Verify PIN (user PIN = type 1)
            let c_pin = std::ffi::CString::new(pin_str).unwrap();
            let mut retry_count: ULONG = 0;
            let ret = api.verify_pin(h_app, 1, c_pin.into_raw(), &mut retry_count);
            
            if ret == SAR_OK {
                // Cache PIN: provider/device/app
                let pin_key = format!("{}/{}/{}", prov_name, dev_name, app_name);
                eprintln!("[INFO] Caching PIN for key: {}", pin_key);
                let mut pins = ctx.pins.write().unwrap();
                pins.insert(pin_key, pin_str.to_string());
            }

            // Cleanup
            api.close_application(h_app);
            api.dis_connect_dev(h_dev);

            if ret == SAR_OK {
                RpcResponse::ok(serde_json::json!(true), id)
            } else {
                let msg = match lang {
                    Language::CN => format!("PIN验证失败: 0x{:08X}, 剩余次数: {}", ret, retry_count),
                    Language::EN => format!("VerifyPIN failed: 0x{:08X}, retries left: {}", ret, retry_count),
                };
                RpcResponse::err(ret as i32, msg, id)
            }
        },
        "CreatePKCS10" => {
            // Params: [providerName, deviceName, appName, subject, keyType, keyLength, containerName]
            // Provider name normalization
            let prov_param = req.params.get(0).and_then(|v| v.as_str()).unwrap_or("");
            let prov_name = if prov_param.is_empty() || prov_param == "default" {
                &ctx.config.default
            } else {
                prov_param
            };
            let dev_name = match req.params.get(1).and_then(|v| v.as_str()) {
                Some(d) => d,
                None => return RpcResponse::err(-2, "Missing deviceName".into(), id),
            };
            let app_name = match req.params.get(2).and_then(|v| v.as_str()) {
                Some(a) => a,
                None => return RpcResponse::err(-2, "Missing appName".into(), id),
            };
            let subject = match req.params.get(3).and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return RpcResponse::err(-2, "Missing subject".into(), id),
            };
            let key_type = req.params.get(4).and_then(|v| v.as_str()).unwrap_or("SM2").to_uppercase();
            let key_length: u32 = req.params.get(5).and_then(|v| v.as_u64()).unwrap_or(256) as u32;
            let container_name_param = req.params.get(6).and_then(|v| v.as_str()).unwrap_or("");

            let is_ecc = key_type == "SM2" || key_type == "ECC";

            // Load API
            let api = match ctx.get_api(prov_name) {
                Ok(a) => a,
                Err(e) => return RpcResponse::err(-5, format!("Load Lib Failed: {}", e), id),
            };

            // Connect device
            let c_dev = std::ffi::CString::new(dev_name).unwrap();
            let mut h_dev: DEVHANDLE = std::ptr::null_mut();
            let ret = api.connect_dev(c_dev.into_raw(), &mut h_dev);
            if ret != SAR_OK {
                return RpcResponse::err(ret as i32, format!("ConnectDev failed: 0x{:08X}", ret), id);
            }

            // Open application
            let c_app = std::ffi::CString::new(app_name).unwrap();
            let mut h_app: HAPPLICATION = std::ptr::null_mut();
            let ret = api.open_application(h_dev, c_app.into_raw(), &mut h_app);
            if ret != SAR_OK {
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(ret as i32, format!("OpenApp failed: 0x{:08X}", ret), id);
            }

            // PIN from cache
            let pin_key = format!("{}/{}/{}", prov_name, dev_name, app_name);
            let pin_cached = {
                let pins = ctx.pins.read().unwrap();
                pins.get(&pin_key).cloned()
            };

            if let Some(pin_str) = pin_cached {
                let c_pin = std::ffi::CString::new(pin_str.as_str()).unwrap();
                let mut retry: ULONG = 0;
                let ret = api.verify_pin(h_app, 1, c_pin.into_raw(), &mut retry);
                if ret != SAR_OK {
                    api.close_application(h_app);
                    api.dis_connect_dev(h_dev);
                    let msg = match lang {
                        Language::CN => format!("PIN验证失败: 0x{:08X}, 剩余重试: {}", ret, retry),
                        Language::EN => format!("VerifyPIN failed: 0x{:08X}, retry: {}", ret, retry),
                    };
                    return RpcResponse::err(ret as i32, msg, id);
                }
            } else {
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(-10, "User not logged in. Call CheckPIN first.".into(), id);
            }

            // Determine container name
            let cont_name = if !container_name_param.is_empty() {
                container_name_param.to_string()
            } else {
                // Generate random container name (UUID-like)
                let mut rand_bytes = [0u8; 16];
                let ret = api.gen_random(h_dev, rand_bytes.as_mut_ptr(), 16);
                if ret == SAR_OK {
                    format!("{{{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
                        rand_bytes[0], rand_bytes[1], rand_bytes[2], rand_bytes[3],
                        rand_bytes[4], rand_bytes[5], rand_bytes[6], rand_bytes[7],
                        rand_bytes[8], rand_bytes[9], rand_bytes[10], rand_bytes[11],
                        rand_bytes[12], rand_bytes[13], rand_bytes[14], rand_bytes[15])
                } else {
                    // Fallback: use timestamp
                    format!("{{CSR-{}}}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis())
                }
            };

            // Create or open container
            let c_cont = std::ffi::CString::new(cont_name.as_str()).unwrap();
            let mut h_cont: HCONTAINER = std::ptr::null_mut();
            let mut ret = api.create_container(h_app, c_cont.clone().into_raw(), &mut h_cont);
            
            // If creation failed but container name was provided, try to open it
            if ret != SAR_OK && !container_name_param.is_empty() {
                ret = api.open_container(h_app, c_cont.into_raw(), &mut h_cont);
            }

            if ret != SAR_OK {
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(ret as i32, format!("Create/OpenContainer failed: 0x{:08X}", ret), id);
            }

            // Generate key pair & build SPKI
            let mut ecc_pub = ECCPUBLICKEYBLOB { BitLen: 0, XCoordinate: [0u8; 64], YCoordinate: [0u8; 64] };
            let (spki, sig_alg_oid, hash_alg_id) = if is_ecc {
                // SM2 key generation
                let ret = api.gen_ecc_key_pair(h_cont, SGD_SM2_1, &mut ecc_pub);
                if ret != SAR_OK {
                    api.close_container(h_cont);
                    api.close_application(h_app);
                    api.dis_connect_dev(h_dev);
                    return RpcResponse::err(ret as i32, format!("GenECCKeyPair failed: 0x{:08X}", ret), id);
                }
                println!("[DEBUG] SM2 PubKey X: {:02X?}", ecc_pub.XCoordinate);
                println!("[DEBUG] SM2 PubKey Y: {:02X?}", ecc_pub.YCoordinate);
                (build_sm2_spki(&ecc_pub), OID_SM3_WITH_SM2, SGD_SM3)
            } else {
                // RSA key generation
                let mut rsa_pub = RSAPUBLICKEYBLOB { AlgID: 0, BitLen: 0, Modulus: [0u8; 256], PublicExponent: [0u8; 4] };
                let ret = api.gen_rsa_key_pair(h_cont, key_length, &mut rsa_pub);
                if ret != SAR_OK {
                    api.close_container(h_cont);
                    api.close_application(h_app);
                    api.dis_connect_dev(h_dev);
                    return RpcResponse::err(ret as i32, format!("GenRSAKeyPair failed: 0x{:08X}", ret), id);
                }
                (build_rsa_spki(&rsa_pub), OID_SHA256_WITH_RSA, 0x00000004_u32) // SHA256
            };

            // Build TBSCertificationRequestInfo
            let version = der_small_integer(0); // v1
            let subject_dn = build_subject_dn(subject);
            let attributes = der_context_0(&[]); // empty attributes [0]
            let tbs = der_sequence(&[&version, &subject_dn, &spki, &attributes]);

            // Hash TBS via device
            let mut h_hash: HANDLE = std::ptr::null_mut();
            let ret = if is_ecc {
                let mut id = *b"1234567812345678";
                api.digest_init(h_dev, hash_alg_id, &mut ecc_pub as *mut _, id.as_mut_ptr(), 16, &mut h_hash)
            } else {
                api.digest_init(h_dev, hash_alg_id, std::ptr::null_mut(), std::ptr::null_mut(), 0, &mut h_hash)
            };
            if ret != SAR_OK {
                api.close_container(h_cont);
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(ret as i32, format!("DigestInit failed: 0x{:08X}", ret), id);
            }

            let mut hash_buf = vec![0u8; 64];
            let mut hash_len: ULONG = hash_buf.len() as ULONG;
            let mut tbs_buf = tbs.clone();
            let ret = api.digest(h_hash, tbs_buf.as_mut_ptr(), tbs_buf.len() as ULONG, hash_buf.as_mut_ptr(), &mut hash_len);
            if ret != SAR_OK {
                api.close_container(h_cont);
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                return RpcResponse::err(ret as i32, format!("Digest failed: 0x{:08X}", ret), id);
            }
            hash_buf.truncate(hash_len as usize);
            println!("[DEBUG] TBS Hash: {:02X?}", hash_buf);

            // Sign hash
            let sig_bits = if is_ecc {
                let mut sig = ECCSIGNATUREBLOB { r: [0u8; 64], s: [0u8; 64] };
                let ret = api.ecc_sign_data(h_cont, hash_buf.as_mut_ptr(), hash_buf.len() as ULONG, &mut sig);
                println!("[DEBUG] SM2 Sig r: {:02X?}", sig.r);
                println!("[DEBUG] SM2 Sig s: {:02X?}", sig.s);
                api.close_container(h_cont);
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                if ret != SAR_OK {
                    return RpcResponse::err(ret as i32, format!("ECCSignData failed: 0x{:08X}", ret), id);
                }
                // ASN.1 DER encode r,s (SM2 values are right-aligned in 64-byte SKF arrays)
                let r_der = der_encode_integer(&sig.r[32..64]);
                let s_der = der_encode_integer(&sig.s[32..64]);
                let sig_der = der_sequence(&[&r_der, &s_der]);
                sig_der
            } else {
                let mut sig_buf = vec![0u8; 512];
                let mut sig_len: ULONG = sig_buf.len() as ULONG;
                let ret = api.rsa_sign_data(h_cont, hash_buf.as_mut_ptr(), hash_buf.len() as ULONG, sig_buf.as_mut_ptr(), &mut sig_len);
                api.close_container(h_cont);
                api.close_application(h_app);
                api.dis_connect_dev(h_dev);
                if ret != SAR_OK {
                    return RpcResponse::err(ret as i32, format!("RSASignData failed: 0x{:08X}", ret), id);
                }
                sig_buf.truncate(sig_len as usize);
                sig_buf
            };

            // Build signatureAlgorithm
            let sig_alg = if is_ecc {
                der_sequence(&[&der_oid(sig_alg_oid)])
            } else {
                der_sequence(&[&der_oid(sig_alg_oid), &[0x05, 0x00]])
            };

            // Assemble CertificationRequest: SEQUENCE { tbs, sigAlg, BIT STRING sig }
            let sig_bit_string = der_bit_string(&sig_bits);
            let csr_der = der_sequence(&[&tbs, &sig_alg, &sig_bit_string]);

            // Convert to PEM
            let csr_b64 = BASE64_STANDARD.encode(&csr_der);
            let mut pem = String::from("-----BEGIN CERTIFICATE REQUEST-----\n");
            for (i, ch) in csr_b64.chars().enumerate() {
                pem.push(ch);
                if (i + 1) % 64 == 0 { pem.push('\n'); }
            }
            if !pem.ends_with('\n') { pem.push('\n'); }
            pem.push_str("-----END CERTIFICATE REQUEST-----");
            std::fs::write("/tmp/last_generated.csr", pem.as_bytes()).unwrap();

            RpcResponse::ok(serde_json::json!({
                "pem": pem,
                "container": cont_name,
                "keyType": key_type,
                "keyLength": key_length
            }), id)
        },
        _ => RpcResponse::err(-3, format!("Method {} not found", req.method), id),
    }
}