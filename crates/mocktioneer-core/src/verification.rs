use anyedge_core::body::Body;
use anyedge_core::context::RequestContext;
use anyedge_core::http::{request_builder, Method, StatusCode, Uri};
use anyedge_core::params::PathParams;
use anyedge_core::proxy::ProxyRequest;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::StreamExt;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const JWKS_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

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

fn create_request_ctx() -> RequestContext {
    let request = request_builder()
        .method(Method::GET)
        .uri("/")
        .body(Body::empty())
        .expect("minimal request should be valid");
    RequestContext::new(request, PathParams::new(HashMap::new()))
}

async fn fetch_jwks(domain: &str) -> Result<JwksResponse, VerificationError> {
    let jwks_url = format!("http://{}/.well-known/ts.jwks.json", domain);

    log::debug!("Fetching JWKS from {}", jwks_url);

    let uri = jwks_url
        .parse::<Uri>()
        .map_err(|e| VerificationError::HttpError(format!("Invalid JWKS URL: {}", e)))?;

    log::info!("URI: {}", uri);
    let proxy_request = ProxyRequest::new(Method::GET, uri);
    let ctx = create_request_ctx();
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
    serde_json::from_slice(&body_bytes)
        .map_err(|e| VerificationError::HttpError(format!("JWKS parse failed: {}", e)))
}

async fn get_cached_jwks(domain: &str) -> Result<JwksResponse, VerificationError> {
    let cache_key = domain.to_string();

    {
        let cache = JWKS_CACHE
            .lock()
            .map_err(|_| VerificationError::HttpError("Cache lock poisoned".to_string()))?;

        if let Some(cached) = cache.get(&cache_key) {
            if cached.fetched_at.elapsed() < JWKS_CACHE_TTL {
                log::debug!(
                    "JWKS cache hit for {} (age: {:?})",
                    cache_key,
                    cached.fetched_at.elapsed()
                );
                return Ok(cached.jwks.clone());
            } else {
                log::debug!(
                    "JWKS cache expired for {} (age: {:?})",
                    cache_key,
                    cached.fetched_at.elapsed()
                );
            }
        } else {
            log::debug!("JWKS cache empty for {} (first fetch)", cache_key);
        }
    }

    log::debug!("Fetching fresh JWKS for {}", cache_key);
    let jwks = fetch_jwks(domain).await?;

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

pub async fn verify_request_id_signature(
    request_id: &str,
    ext: Option<&serde_json::Value>,
    domain: &str,
) -> Result<String, VerificationError> {
    let ext_obj = ext.and_then(|e| e.get("trusted_server")).ok_or_else(|| {
        VerificationError::InvalidSignature("Missing ext.trusted_server".to_string())
    })?;

    let signature = ext_obj
        .get("signature")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            VerificationError::InvalidSignature("Missing ext.trusted_server.signature".to_string())
        })?;

    let key_id = ext_obj.get("kid").and_then(|v| v.as_str()).ok_or_else(|| {
        VerificationError::KeyNotFound("Missing ext.trusted_server.kid".to_string())
    })?;

    log::info!(
        "Signature verification requested: id={}, kid={}, domain={:?}",
        request_id,
        key_id,
        domain
    );

    let jwks = get_cached_jwks(domain).await?;
    let public_key = find_public_key(&jwks, key_id)?;
    verify_ed25519_signature(public_key, signature, request_id)?;

    Ok(key_id.to_string())
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;

    #[test]
    fn verify_missing_signature_field() {
        let request_id = "test-id";
        let ext = serde_json::json!({
            "trusted_server": {
                "kid": "test-key"
            }
        });

        let result = block_on(verify_request_id_signature(
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
        let result = block_on(verify_request_id_signature(
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
        let result = block_on(verify_request_id_signature(
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
        let result = block_on(verify_request_id_signature(request_id, None, "example.com"));
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
