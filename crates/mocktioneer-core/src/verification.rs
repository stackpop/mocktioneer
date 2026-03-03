use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use edgezero_core::body::Body;
use edgezero_core::context::RequestContext;
use edgezero_core::http::{Method, StatusCode, Uri};
use edgezero_core::proxy::ProxyRequest;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const JWKS_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const SIGNING_VERSION: &str = "1.1";

#[derive(Debug, Clone, Deserialize)]
struct TrustedServerResponse {
    jwks: JwksResponse,
}

#[derive(Debug, Clone, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Clone, Deserialize)]
struct JwkKey {
    kid: String,
    x: String, // Base64url-encoded Ed25519 public key
}

struct JwksCache {
    jwks: JwksResponse,
    fetched_at: Instant,
}

#[derive(Serialize)]
struct SigningPayload<'a> {
    version: &'a str,
    kid: &'a str,
    host: &'a str,
    scheme: &'a str,
    id: &'a str,
    ts: u64,
}

static JWKS_CACHE: LazyLock<Mutex<HashMap<String, JwksCache>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("No domain for JWKS verification")]
    NoJwksDomain,
}

async fn fetch_jwks(ctx: &RequestContext, domain: &str) -> Result<JwksResponse, VerificationError> {
    let jwks_url = format!("https://{}/.well-known/trusted-server.json", domain);

    log::debug!("Fetching JWKS from {}", jwks_url);

    let uri = jwks_url
        .parse::<Uri>()
        .map_err(|e| VerificationError::HttpError(format!("Invalid JWKS URL: {}", e)))?;

    log::info!("URI: {}", uri);
    let proxy_request = ProxyRequest::new(Method::GET, uri);
    let proxy_handle = ctx
        .proxy_handle()
        .ok_or_else(|| VerificationError::HttpError("Proxy not available".to_string()))?;

    let resp = proxy_handle
        .forward(proxy_request)
        .await
        .map_err(|e| VerificationError::HttpError(format!("JWKS fetch failed: {}", e)))?;

    if resp.status() != StatusCode::OK {
        return Err(VerificationError::HttpError(format!(
            "JWKS server returned status: {}",
            resp.status()
        )));
    }

    let body = resp.into_body();

    let body_bytes = match body {
        Body::Once(bytes) => bytes.to_vec(),
        Body::Stream(mut stream) => {
            let mut collected = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| {
                    VerificationError::HttpError(format!("Stream read failed: {}", e))
                })?;
                collected.extend_from_slice(&chunk);
            }

            collected
        }
    };
    let response: TrustedServerResponse = serde_json::from_slice(&body_bytes)
        .map_err(|e| VerificationError::HttpError(format!("JWKS parse failed: {}", e)))?;
    Ok(response.jwks)
}

async fn get_cached_jwks(
    ctx: &RequestContext,
    domain: &str,
) -> Result<JwksResponse, VerificationError> {
    let cache_key = domain.to_string();

    {
        let cache = JWKS_CACHE
            .lock()
            .map_err(|_| VerificationError::HttpError("Cache lock poisoned".to_string()))?;

        if let Some(cached) = cache.get(&cache_key) {
            let cache_age = cached.fetched_at.elapsed();
            if cache_age < JWKS_CACHE_TTL {
                log::debug!("JWKS cache hit for {} (age: {:?})", cache_key, cache_age);
                return Ok(cached.jwks.clone());
            }

            log::debug!(
                "JWKS cache expired for {} (age: {:?})",
                cache_key,
                cache_age
            );
        } else {
            log::debug!("JWKS cache empty for {} (first fetch)", cache_key);
        }
    }

    log::debug!("Fetching fresh JWKS for {}", cache_key);
    let jwks = fetch_jwks(ctx, domain).await?;

    let mut cache = JWKS_CACHE
        .lock()
        .map_err(|_| VerificationError::HttpError("Cache lock poisoned".to_string()))?;

    cache.insert(
        cache_key,
        JwksCache {
            jwks: jwks.clone(),
            fetched_at: Instant::now(),
        },
    );

    Ok(jwks)
}

fn find_public_key<'a>(jwks: &'a JwksResponse, kid: &str) -> Result<&'a str, VerificationError> {
    jwks.keys
        .iter()
        .find(|k| k.kid == kid)
        .map(|k| k.x.as_str())
        .ok_or_else(|| VerificationError::KeyNotFound(format!("Key {} not found in JWKS", kid)))
}

fn verify_ed25519_signature(
    public_key_b64: &str,
    signature_b64: &str,
    message: &str,
) -> Result<(), VerificationError> {
    let public_key_bytes = URL_SAFE_NO_PAD.decode(public_key_b64).map_err(|e| {
        VerificationError::InvalidSignature(format!("Invalid public key encoding: {}", e))
    })?;

    if public_key_bytes.len() != 32 {
        return Err(VerificationError::InvalidSignature(format!(
            "Invalid public key length: expected 32, got {}",
            public_key_bytes.len()
        )));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&public_key_bytes);

    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| VerificationError::InvalidSignature(format!("Invalid public key: {}", e)))?;

    let signature_bytes = URL_SAFE_NO_PAD.decode(signature_b64).map_err(|e| {
        VerificationError::InvalidSignature(format!("Invalid signature encoding: {}", e))
    })?;

    if signature_bytes.len() != 64 {
        return Err(VerificationError::InvalidSignature(format!(
            "Invalid signature length: expected 64, got {}",
            signature_bytes.len()
        )));
    }

    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&signature_bytes);

    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_| VerificationError::SignatureVerificationFailed)?;

    Ok(())
}

fn build_signing_payload(
    request_id: &str,
    key_id: &str,
    request_host: &str,
    request_scheme: &str,
    timestamp: u64,
    version: &str,
) -> Result<String, VerificationError> {
    if version != SIGNING_VERSION {
        return Err(VerificationError::InvalidSignature(format!(
            "Unsupported ext.trusted_server.version '{}'; expected '{}'",
            version, SIGNING_VERSION
        )));
    }

    let payload = SigningPayload {
        version,
        kid: key_id,
        host: request_host,
        scheme: request_scheme,
        id: request_id,
        ts: timestamp,
    };

    serde_json::to_string(&payload).map_err(|e| {
        VerificationError::InvalidSignature(format!("Failed to serialize signing payload: {}", e))
    })
}

fn required_ext_str<'a>(
    ext_obj: &'a serde_json::Value,
    field: &str,
    missing_error: VerificationError,
) -> Result<&'a str, VerificationError> {
    ext_obj
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or(missing_error)
}

fn required_ext_u64(
    ext_obj: &serde_json::Value,
    field: &str,
    missing_error: VerificationError,
) -> Result<u64, VerificationError> {
    ext_obj
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or(missing_error)
}

pub async fn verify_request_id_signature(
    ctx: &RequestContext,
    request_id: &str,
    ext: Option<&serde_json::Value>,
    domain: &str,
) -> Result<String, VerificationError> {
    let ext_obj = ext.and_then(|e| e.get("trusted_server")).ok_or_else(|| {
        VerificationError::InvalidSignature("Missing ext.trusted_server".to_string())
    })?;

    let signature = required_ext_str(
        ext_obj,
        "signature",
        VerificationError::InvalidSignature("Missing ext.trusted_server.signature".to_string()),
    )?;

    let key_id = required_ext_str(
        ext_obj,
        "kid",
        VerificationError::KeyNotFound("Missing ext.trusted_server.kid".to_string()),
    )?;

    let version = required_ext_str(
        ext_obj,
        "version",
        VerificationError::InvalidSignature("Missing ext.trusted_server.version".to_string()),
    )?;

    let request_host = required_ext_str(
        ext_obj,
        "request_host",
        VerificationError::InvalidSignature("Missing ext.trusted_server.request_host".to_string()),
    )?;

    let request_scheme = required_ext_str(
        ext_obj,
        "request_scheme",
        VerificationError::InvalidSignature(
            "Missing ext.trusted_server.request_scheme".to_string(),
        ),
    )?;

    let timestamp = required_ext_u64(
        ext_obj,
        "ts",
        VerificationError::InvalidSignature("Missing ext.trusted_server.ts".to_string()),
    )?;

    let payload = build_signing_payload(
        request_id,
        key_id,
        request_host,
        request_scheme,
        timestamp,
        version,
    )?;

    log::info!(
        "Signature verification requested: id={}, kid={}, domain={:?}, version={}, ts={}",
        request_id,
        key_id,
        domain,
        version,
        timestamp
    );

    let jwks = get_cached_jwks(ctx, domain).await?;
    let public_key = find_public_key(&jwks, key_id)?;
    verify_ed25519_signature(public_key, signature, &payload)?;

    Ok(key_id.to_string())
}

#[cfg(test)]
mod tests {
    use edgezero_core::http::request_builder;
    use edgezero_core::params::PathParams;
    use futures::executor::block_on;
    use std::collections::HashMap;

    use super::*;

    fn create_test_context() -> RequestContext {
        let request = request_builder()
            .method(Method::POST)
            .uri("/openrtb2/auction")
            .body(Body::empty())
            .unwrap();
        RequestContext::new(request, PathParams::new(HashMap::new()))
    }

    #[test]
    fn verify_missing_signature_field() {
        let request_id = "test-id";
        let ext = serde_json::json!({
                "trusted_server": {
                    "kid": "test-key"
                }
        });

        let ctx = create_test_context();

        let result = block_on(verify_request_id_signature(
            &ctx,
            request_id,
            Some(&ext),
            "example.com",
        ));
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::InvalidSignature(_)
        ));
    }

    #[test]
    fn verify_missing_kid_field() {
        let request_id = "test-id";
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig"
            }
        });

        let ctx = create_test_context();

        let result = block_on(verify_request_id_signature(
            &ctx,
            request_id,
            Some(&ext),
            "example.com",
        ));
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::KeyNotFound(_)
        ));
    }

    #[test]
    fn verify_missing_trusted_server_object() {
        let request_id = "test-id";
        let ext = serde_json::json!({
            "some_other_field": "value"
        });

        let ctx = create_test_context();

        let result = block_on(verify_request_id_signature(
            &ctx,
            request_id,
            Some(&ext),
            "example.com",
        ));
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::InvalidSignature(_)
        ));
    }

    #[test]
    fn verify_with_none_ext() {
        let request_id = "test-id";

        let ctx = create_test_context();

        let result = block_on(verify_request_id_signature(
            &ctx,
            request_id,
            None,
            "example.com",
        ));
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::InvalidSignature(_)
        ));
    }

    #[test]
    fn verify_missing_version_field() {
        let request_id = "test-id";
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "request_host": "example.com",
                "request_scheme": "https",
                "ts": 1706900000
            }
        });

        let ctx = create_test_context();

        let result = block_on(verify_request_id_signature(
            &ctx,
            request_id,
            Some(&ext),
            "example.com",
        ));
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::InvalidSignature(_)
        ));
    }

    #[test]
    fn build_signing_payload_uses_v11_shape() {
        let payload = build_signing_payload(
            "req-123",
            "kid-abc",
            "publisher.example",
            "https",
            1706900000,
            "1.1",
        )
        .expect("payload");

        assert_eq!(
            payload,
            "{\"version\":\"1.1\",\"kid\":\"kid-abc\",\"host\":\"publisher.example\",\"scheme\":\"https\",\"id\":\"req-123\",\"ts\":1706900000}"
        );
    }

    #[test]
    fn build_signing_payload_rejects_unknown_version() {
        let result = build_signing_payload(
            "req-123",
            "kid-abc",
            "publisher.example",
            "https",
            1706900000,
            "1.0",
        );

        assert!(matches!(
            result.unwrap_err(),
            VerificationError::InvalidSignature(_)
        ));
    }

    #[test]
    fn find_public_key_found() {
        let jwks = JwksResponse {
            keys: vec![JwkKey {
                kid: "key-001".to_string(),
                x: "test-key-base64url".to_string(),
            }],
        };

        let result = find_public_key(&jwks, "key-001");
        assert_eq!(result.unwrap(), "test-key-base64url");
    }

    #[test]
    fn find_public_key_not_found() {
        let jwks = JwksResponse { keys: vec![] };

        let result = find_public_key(&jwks, "missing-key");
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::KeyNotFound(_)
        ));
    }

    #[test]
    fn verify_ed25519_invalid_key_length() {
        let result = verify_ed25519_signature("dGVzdA", "sig", "message");
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::InvalidSignature(_)
        ));
    }

    #[test]
    fn verify_ed25519_invalid_signature_length() {
        let public_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let result = verify_ed25519_signature(public_key, "dGVzdA", "message");
        assert!(matches!(
            result.unwrap_err(),
            VerificationError::InvalidSignature(_)
        ));
    }
}
