use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use edgezero_core::body::Body;
use edgezero_core::context::RequestContext;
use edgezero_core::http::{Method, StatusCode, Uri};
use edgezero_core::proxy::ProxyRequest;
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use url::Host;

const JWKS_CACHE_TTL: Duration = Duration::from_mins(10);
const SIGNING_VERSION: &str = "1.1";

/// Maximum allowed clock skew for timestamp freshness check (5 minutes in milliseconds).
const TS_FRESHNESS_WINDOW_MS: u64 = 5 * 60 * 1000;

/// Maximum JWKS response body size (64 KiB).
const MAX_JWKS_BODY_BYTES: usize = 64 * 1024;

static JWKS_CACHE: LazyLock<Mutex<HashMap<String, JwksCache>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Deserialize)]
struct JwkKey {
    kid: String,
    x: String, // Base64url-encoded Ed25519 public key
}

struct JwksCache {
    fetched_at: Instant,
    jwks: JwksResponse,
}

#[derive(Debug, Clone, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Clone, Deserialize)]
struct TrustedServerResponse {
    jwks: JwksResponse,
}

// IMPORTANT: Field order defines the canonical signing payload.
// `serde_json::to_string` serializes struct fields in declaration order.
// Reordering fields will silently break signature verification.
#[derive(Serialize)]
#[expect(
    clippy::arbitrary_source_item_ordering,
    reason = "field order is the canonical Trusted Server v1.1 signing payload"
)]
struct SigningPayload<'payload> {
    version: &'payload str,
    kid: &'payload str,
    host: &'payload str,
    scheme: &'payload str,
    id: &'payload str,
    ts: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
}

fn jwks_body_too_large_error() -> VerificationError {
    VerificationError::HttpError(format!(
        "JWKS response body exceeds {MAX_JWKS_BODY_BYTES} byte limit"
    ))
}

async fn collect_jwks_body(body: Body) -> Result<Vec<u8>, VerificationError> {
    match body {
        Body::Once(bytes) => {
            if bytes.len() > MAX_JWKS_BODY_BYTES {
                return Err(jwks_body_too_large_error());
            }
            Ok(bytes.to_vec())
        }
        Body::Stream(mut stream) => {
            let mut collected = Vec::new();
            while let Some(next_chunk) = stream.next().await {
                let chunk = next_chunk.map_err(|err| {
                    VerificationError::HttpError(format!("Stream read failed: {err}"))
                })?;
                if collected.len().saturating_add(chunk.len()) > MAX_JWKS_BODY_BYTES {
                    return Err(jwks_body_too_large_error());
                }
                collected.extend_from_slice(&chunk);
            }

            Ok(collected)
        }
    }
}

async fn fetch_jwks(ctx: &RequestContext, domain: &str) -> Result<JwksResponse, VerificationError> {
    let host = validate_jwks_host(domain)?;
    let jwks_url = format!("https://{host}/.well-known/trusted-server.json");

    log::debug!("Fetching JWKS from {jwks_url}");

    let uri = jwks_url
        .parse::<Uri>()
        .map_err(|err| VerificationError::HttpError(format!("Invalid JWKS URL: {err}")))?;

    let proxy_request = ProxyRequest::new(Method::GET, uri);
    let proxy_handle = ctx
        .proxy_handle()
        .ok_or_else(|| VerificationError::HttpError("Proxy not available".to_owned()))?;

    let resp = proxy_handle
        .forward(proxy_request)
        .await
        .map_err(|err| VerificationError::HttpError(format!("JWKS fetch failed: {err}")))?;

    if resp.status() != StatusCode::OK {
        return Err(VerificationError::HttpError(format!(
            "JWKS server returned status: {}",
            resp.status()
        )));
    }

    let body_bytes = collect_jwks_body(resp.into_body()).await?;
    let response: TrustedServerResponse = serde_json::from_slice(&body_bytes)
        .map_err(|err| VerificationError::HttpError(format!("JWKS parse failed: {err}")))?;
    Ok(response.jwks)
}

async fn get_cached_jwks(
    ctx: &RequestContext,
    domain: &str,
) -> Result<JwksResponse, VerificationError> {
    let cache_key = domain.to_owned();

    {
        let cache = JWKS_CACHE
            .lock()
            .map_err(|err| VerificationError::HttpError(format!("Cache lock poisoned: {err}")))?;

        if let Some(cached) = cache.get(&cache_key) {
            let cache_age = cached.fetched_at.elapsed();
            if cache_age < JWKS_CACHE_TTL {
                log::debug!("JWKS cache hit for {cache_key} (age: {cache_age:?})");
                return Ok(cached.jwks.clone());
            }
            log::debug!("JWKS cache expired for {cache_key} (age: {cache_age:?})");
        } else {
            log::debug!("JWKS cache empty for {cache_key} (first fetch)");
        }
    }

    log::debug!("Fetching fresh JWKS for {cache_key}");
    let jwks = fetch_jwks(ctx, domain).await?;

    let mut cache = JWKS_CACHE
        .lock()
        .map_err(|err| VerificationError::HttpError(format!("Cache lock poisoned: {err}")))?;

    cache.insert(
        cache_key,
        JwksCache {
            jwks: jwks.clone(),
            fetched_at: Instant::now(),
        },
    );

    Ok(jwks)
}

fn find_public_key<'jwks>(
    jwks: &'jwks JwksResponse,
    kid: &str,
) -> Result<&'jwks str, VerificationError> {
    jwks.keys
        .iter()
        .find(|key| key.kid == kid)
        .map(|key| key.x.as_str())
        .ok_or_else(|| VerificationError::KeyNotFound(format!("Key {kid} not found in JWKS")))
}

fn verify_ed25519_signature(
    public_key_b64: &str,
    signature_b64: &str,
    message: &str,
) -> Result<(), VerificationError> {
    let public_key_bytes = URL_SAFE_NO_PAD.decode(public_key_b64).map_err(|err| {
        VerificationError::InvalidSignature(format!("Invalid public key encoding: {err}"))
    })?;

    if public_key_bytes.len() != 32 {
        return Err(VerificationError::InvalidSignature(format!(
            "Invalid public key length: expected 32, got {}",
            public_key_bytes.len()
        )));
    }

    let mut key_array = [0_u8; 32];
    key_array.copy_from_slice(&public_key_bytes);

    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|err| VerificationError::InvalidSignature(format!("Invalid public key: {err}")))?;

    let signature_bytes = URL_SAFE_NO_PAD.decode(signature_b64).map_err(|err| {
        VerificationError::InvalidSignature(format!("Invalid signature encoding: {err}"))
    })?;

    if signature_bytes.len() != 64 {
        return Err(VerificationError::InvalidSignature(format!(
            "Invalid signature length: expected 64, got {}",
            signature_bytes.len()
        )));
    }

    let mut sig_array = [0_u8; 64];
    sig_array.copy_from_slice(&signature_bytes);

    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|_err| VerificationError::SignatureVerificationFailed)?;

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
            "Unsupported ext.trusted_server.version '{version}'; expected '{SIGNING_VERSION}'"
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

    serde_json::to_string(&payload).map_err(|err| {
        VerificationError::InvalidSignature(format!("Failed to serialize signing payload: {err}"))
    })
}

/// Strip default ports (:443 for https, :80 for http) and lowercase the host
/// so that `example.com:443` matches `example.com` from the signer.
fn canonicalize_host(host: &str) -> String {
    let trimmed = host.trim();
    trimmed
        .strip_suffix(":443")
        .or_else(|| trimmed.strip_suffix(":80"))
        .unwrap_or(trimmed)
        .to_lowercase()
}

fn check_timestamp_freshness(timestamp_ms: u64) -> Result<(), VerificationError> {
    let now_ms = current_time_ms()?;
    let diff = now_ms.abs_diff(timestamp_ms);

    if diff > TS_FRESHNESS_WINDOW_MS {
        let direction = if timestamp_ms > now_ms {
            "future-dated"
        } else {
            "stale"
        };

        return Err(VerificationError::InvalidSignature(format!(
            "ext.trusted_server.ts is {direction}: {diff}ms drift exceeds {TS_FRESHNESS_WINDOW_MS}ms window"
        )));
    }

    Ok(())
}

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
fn current_time_ms() -> Result<u64, VerificationError> {
    let now_ms = js_sys::Date::now();
    if now_ms.is_finite() && now_ms >= 0.0 {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions,
            reason = "JS epoch milliseconds fit in u64; finiteness and sign are checked above"
        )]
        Ok(now_ms as u64)
    } else {
        Err(VerificationError::InvalidSignature(
            "System clock error".to_owned(),
        ))
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
fn current_time_ms() -> Result<u64, VerificationError> {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::as_conversions,
                reason = "epoch milliseconds fit in u64 for any realistic system clock"
            )]
            {
                elapsed.as_millis() as u64
            }
        })
        .map_err(|_err| VerificationError::InvalidSignature("System clock error".to_owned()))
}

fn invalid_site_domain(domain: &str) -> VerificationError {
    VerificationError::InvalidSignature(format!("Invalid site.domain host: {domain:?}"))
}

fn is_valid_public_dns_hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 || host == "localhost" || !host.contains('.') {
        return false;
    }

    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn required_ext_str<'ext>(
    ext_obj: &'ext serde_json::Value,
    field: &str,
    missing_error: impl FnOnce() -> VerificationError,
) -> Result<&'ext str, VerificationError> {
    ext_obj
        .get(field)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(missing_error)
}

fn required_ext_u64(
    ext_obj: &serde_json::Value,
    field: &str,
    missing_error: impl FnOnce() -> VerificationError,
) -> Result<u64, VerificationError> {
    ext_obj
        .get(field)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(missing_error)
}

fn validate_jwks_host(domain: &str) -> Result<String, VerificationError> {
    let host = domain.trim();
    if host.is_empty() {
        return Err(VerificationError::InvalidSignature(
            "Invalid site.domain: empty host".to_owned(),
        ));
    }

    if host
        .bytes()
        .any(|byte| matches!(byte, b'/' | b'\\' | b'@' | b'?' | b'#'))
    {
        return Err(invalid_site_domain(domain));
    }

    match Host::parse(host).map_err(|_err| invalid_site_domain(domain))? {
        Host::Domain(parsed) => {
            let canonical = parsed.trim_end_matches('.').to_ascii_lowercase();
            if is_valid_public_dns_hostname(&canonical) {
                Ok(canonical)
            } else {
                Err(invalid_site_domain(domain))
            }
        }
        Host::Ipv4(_) | Host::Ipv6(_) => Err(invalid_site_domain(domain)),
    }
}

/// Verifies the Ed25519 signature attached to an `OpenRTB` request ID.
///
/// # Errors
///
/// Returns [`VerificationError`] when the `ext.trusted_server` object is
/// missing or malformed, when the JWKS document cannot be fetched, when the
/// referenced key is absent, or when the signature does not match.
#[inline]
pub async fn verify_request_id_signature(
    ctx: &RequestContext,
    request_id: &str,
    ext: Option<&serde_json::Value>,
    site_domain: &str,
) -> Result<String, VerificationError> {
    let ext_obj = ext
        .and_then(|value| value.get("trusted_server"))
        .ok_or_else(|| {
            VerificationError::InvalidSignature("Missing ext.trusted_server".to_owned())
        })?;

    let signature = required_ext_str(ext_obj, "signature", || {
        VerificationError::InvalidSignature("Missing ext.trusted_server.signature".to_owned())
    })?;

    let key_id = required_ext_str(ext_obj, "kid", || {
        VerificationError::InvalidSignature("Missing ext.trusted_server.kid".to_owned())
    })?;

    let version = required_ext_str(ext_obj, "version", || {
        VerificationError::InvalidSignature("Missing ext.trusted_server.version".to_owned())
    })?;

    let request_host = required_ext_str(ext_obj, "request_host", || {
        VerificationError::InvalidSignature("Missing ext.trusted_server.request_host".to_owned())
    })?;

    let request_scheme = required_ext_str(ext_obj, "request_scheme", || {
        VerificationError::InvalidSignature("Missing ext.trusted_server.request_scheme".to_owned())
    })?;

    let timestamp = required_ext_u64(ext_obj, "ts", || {
        VerificationError::InvalidSignature("Missing ext.trusted_server.ts".to_owned())
    })?;

    // Cross-check: the signer's claimed host must match the publisher's
    // site.domain from the OpenRTB request. The bidder's own host (ForwardedHost)
    // is intentionally NOT compared — in header bidding the publisher's domain
    // and the bidder's domain are always different.
    let canon_ext_host = canonicalize_host(request_host);
    let canon_site_domain = canonicalize_host(site_domain);
    if canon_ext_host != canon_site_domain {
        return Err(VerificationError::InvalidSignature(format!(
            "ext.trusted_server.request_host '{request_host}' does not match site.domain '{site_domain}'"
        )));
    }

    // Note: request_scheme is part of the signed payload and verified
    // cryptographically. No separate cross-check is needed since site.domain
    // does not carry scheme information.

    // Enforce timestamp freshness to prevent replay attacks.
    check_timestamp_freshness(timestamp)?;

    let payload = build_signing_payload(
        request_id,
        key_id,
        request_host,
        request_scheme,
        timestamp,
        version,
    )?;

    log::info!(
        "Signature verification requested: id={request_id}, kid={key_id}, domain={site_domain:?}, version={version}, ts={timestamp}"
    );

    let jwks = get_cached_jwks(ctx, site_domain).await?;
    let public_key = find_public_key(&jwks, key_id)?;
    verify_ed25519_signature(public_key, signature, &payload)?;

    Ok(key_id.to_owned())
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
            VerificationError::InvalidSignature(_)
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
        let now_ms = current_time_ms().unwrap();
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "request_host": "example.com",
                "request_scheme": "https",
                "ts": now_ms
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
    fn verify_missing_request_host_field() {
        let now_ms = current_time_ms().unwrap();
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "version": "1.1",
                "request_scheme": "https",
                "ts": now_ms
            }
        });

        let ctx = create_test_context();
        let result = block_on(verify_request_id_signature(
            &ctx,
            "test-id",
            Some(&ext),
            "example.com",
        ));
        let err = result.unwrap_err();
        assert!(matches!(err, VerificationError::InvalidSignature(_)));
        assert!(err
            .to_string()
            .contains("Missing ext.trusted_server.request_host"));
    }

    #[test]
    fn verify_missing_request_scheme_field() {
        let now_ms = current_time_ms().unwrap();
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "version": "1.1",
                "request_host": "example.com",
                "ts": now_ms
            }
        });

        let ctx = create_test_context();
        let result = block_on(verify_request_id_signature(
            &ctx,
            "test-id",
            Some(&ext),
            "example.com",
        ));
        let err = result.unwrap_err();
        assert!(matches!(err, VerificationError::InvalidSignature(_)));
        assert!(err
            .to_string()
            .contains("Missing ext.trusted_server.request_scheme"));
    }

    #[test]
    fn verify_missing_ts_field() {
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "version": "1.1",
                "request_host": "example.com",
                "request_scheme": "https"
            }
        });

        let ctx = create_test_context();
        let result = block_on(verify_request_id_signature(
            &ctx,
            "test-id",
            Some(&ext),
            "example.com",
        ));
        let err = result.unwrap_err();
        assert!(matches!(err, VerificationError::InvalidSignature(_)));
        assert!(err.to_string().contains("Missing ext.trusted_server.ts"));
    }

    #[test]
    fn build_signing_payload_uses_v11_shape() {
        let payload = build_signing_payload(
            "req-123",
            "kid-abc",
            "publisher.example",
            "https",
            1_706_900_000_000,
            "1.1",
        )
        .expect("payload");

        assert_eq!(
            payload,
            "{\"version\":\"1.1\",\"kid\":\"kid-abc\",\"host\":\"publisher.example\",\"scheme\":\"https\",\"id\":\"req-123\",\"ts\":1706900000000}"
        );
    }

    #[test]
    fn build_signing_payload_rejects_unknown_version() {
        let result = build_signing_payload(
            "req-123",
            "kid-abc",
            "publisher.example",
            "https",
            1_706_900_000_000,
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
                kid: "key-001".to_owned(),
                x: "test-key-base64url".to_owned(),
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

    #[test]
    fn verify_host_mismatch_rejected() {
        let now_ms = current_time_ms().unwrap();
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "version": "1.1",
                "request_host": "attacker.example",
                "request_scheme": "https",
                "ts": now_ms
            }
        });

        let ctx = create_test_context();
        let result = block_on(verify_request_id_signature(
            &ctx,
            "test-id",
            Some(&ext),
            "example.com",
        ));
        let err = result.unwrap_err().to_string();
        assert!(err.contains("does not match site.domain"));
    }

    #[test]
    fn verify_stale_timestamp_rejected() {
        // Timestamp 10 minutes in the past (exceeds 5-minute window)
        let stale_ts = current_time_ms().unwrap() - 10 * 60 * 1000;
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "version": "1.1",
                "request_host": "example.com",
                "request_scheme": "https",
                "ts": stale_ts
            }
        });

        let ctx = create_test_context();
        let result = block_on(verify_request_id_signature(
            &ctx,
            "test-id",
            Some(&ext),
            "example.com",
        ));
        let err = result.unwrap_err();
        assert!(matches!(err, VerificationError::InvalidSignature(_)));
        assert!(err.to_string().contains("stale"));
    }

    #[test]
    fn verify_future_timestamp_rejected() {
        // Timestamp 10 minutes in the future (exceeds 5-minute window)
        let future_ts = current_time_ms().unwrap() + 10 * 60 * 1000;
        let ext = serde_json::json!({
            "trusted_server": {
                "signature": "test-sig",
                "kid": "test-key",
                "version": "1.1",
                "request_host": "example.com",
                "request_scheme": "https",
                "ts": future_ts
            }
        });

        let ctx = create_test_context();
        let result = block_on(verify_request_id_signature(
            &ctx,
            "test-id",
            Some(&ext),
            "example.com",
        ));
        let err = result.unwrap_err();
        assert!(matches!(err, VerificationError::InvalidSignature(_)));
        assert!(err.to_string().contains("future-dated"));
    }

    #[test]
    fn check_timestamp_freshness_within_window() {
        let now_ms = current_time_ms().unwrap();
        // Current time should pass.
        check_timestamp_freshness(now_ms).unwrap();
        // 1 minute ago should pass.
        check_timestamp_freshness(now_ms - 60_000).unwrap();
        // 1 minute in the future should pass.
        check_timestamp_freshness(now_ms + 60_000).unwrap();
    }

    #[test]
    fn verify_ed25519_roundtrip_with_known_keypair() {
        use ed25519_dalek::{Signer as _, SigningKey};

        // Deterministic seed for reproducible test.
        let seed: [u8; 32] = [42_u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        // Encode keys as base64url (no padding).
        let public_key_b64 = URL_SAFE_NO_PAD.encode(verifying_key.as_bytes());

        // Build a canonical signing payload.
        let payload = build_signing_payload(
            "req-roundtrip",
            "kid-test",
            "publisher.example",
            "https",
            1_706_900_000_000,
            "1.1",
        )
        .expect("payload");

        // Sign the payload.
        let signature = signing_key.sign(payload.as_bytes());
        let signature_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        // Verify should succeed.
        verify_ed25519_signature(&public_key_b64, &signature_b64, &payload).unwrap();

        // Verify with tampered payload should fail
        let tampered = payload.replace("req-roundtrip", "req-tampered");
        assert!(matches!(
            verify_ed25519_signature(&public_key_b64, &signature_b64, &tampered).unwrap_err(),
            VerificationError::SignatureVerificationFailed
        ));
    }

    #[test]
    fn verify_request_id_signature_success_path() {
        use ed25519_dalek::{Signer as _, SigningKey};

        let seed = [42_u8; 32];
        let signing_key = SigningKey::from_bytes(&seed);
        let public_key_b64 = URL_SAFE_NO_PAD.encode(signing_key.verifying_key().as_bytes());

        let domain = "verify-success.example";
        let request_id = "req-success";
        let key_id = "kid-success";
        let timestamp = current_time_ms().unwrap();
        let payload = build_signing_payload(
            request_id,
            key_id,
            domain,
            "https",
            timestamp,
            SIGNING_VERSION,
        )
        .expect("payload");
        let signature_b64 = URL_SAFE_NO_PAD.encode(signing_key.sign(payload.as_bytes()).to_bytes());

        {
            let mut cache = JWKS_CACHE.lock().unwrap();
            cache.remove(domain);
            let _previous = cache.insert(
                domain.to_owned(),
                JwksCache {
                    jwks: JwksResponse {
                        keys: vec![JwkKey {
                            kid: key_id.to_owned(),
                            x: public_key_b64,
                        }],
                    },
                    fetched_at: Instant::now(),
                },
            );
        }

        let ext = serde_json::json!({
            "trusted_server": {
                "signature": signature_b64,
                "kid": key_id,
                "version": SIGNING_VERSION,
                "request_host": domain,
                "request_scheme": "https",
                "ts": timestamp
            }
        });

        let ctx = create_test_context();
        let result = block_on(verify_request_id_signature(
            &ctx,
            request_id,
            Some(&ext),
            domain,
        ));

        JWKS_CACHE.lock().unwrap().remove(domain);

        assert_eq!(result.unwrap(), key_id);
    }

    #[test]
    fn canonicalize_host_cases() {
        assert_eq!(canonicalize_host("EXAMPLE.COM"), "example.com");
        assert_eq!(canonicalize_host("example.com:443"), "example.com");
        assert_eq!(canonicalize_host("example.com:80"), "example.com");
        assert_eq!(canonicalize_host("example.com:8080"), "example.com:8080");
        assert_eq!(canonicalize_host("  example.com  "), "example.com");
    }

    #[test]
    fn collect_jwks_body_rejects_oversized_once_body() {
        let body = Body::from_bytes(vec![0_u8; MAX_JWKS_BODY_BYTES + 1]);
        let err = block_on(collect_jwks_body(body)).unwrap_err();
        assert!(matches!(err, VerificationError::HttpError(_)));
        assert!(err.to_string().contains("JWKS response body exceeds"));
    }

    #[test]
    fn validate_jwks_host_accepts_hostname() {
        assert_eq!(validate_jwks_host("example.com").unwrap(), "example.com");
        assert_eq!(
            validate_jwks_host("  Example.COM  ").unwrap(),
            "example.com"
        );
    }

    #[test]
    fn validate_jwks_host_rejects_userinfo() {
        let err = validate_jwks_host("foo@evil.com").unwrap_err().to_string();
        assert!(err.contains("Invalid site.domain host"));
    }

    #[test]
    fn validate_jwks_host_rejects_path_query_and_fragment() {
        let path_err = validate_jwks_host("example.com/path")
            .unwrap_err()
            .to_string();
        assert!(path_err.contains("Invalid site.domain host"));

        let query_err = validate_jwks_host("example.com?x=1")
            .unwrap_err()
            .to_string();
        assert!(query_err.contains("Invalid site.domain host"));

        let fragment_err = validate_jwks_host("example.com#frag")
            .unwrap_err()
            .to_string();
        assert!(fragment_err.contains("Invalid site.domain host"));
    }

    #[test]
    fn validate_jwks_host_rejects_empty_input() {
        let err = validate_jwks_host("   ").unwrap_err().to_string();
        assert!(err.contains("empty host"));
    }

    #[test]
    fn validate_jwks_host_rejects_localhost_and_single_label_hosts() {
        let localhost_err = validate_jwks_host("localhost").unwrap_err().to_string();
        assert!(localhost_err.contains("Invalid site.domain host"));

        let single_label_err = validate_jwks_host("intranet").unwrap_err().to_string();
        assert!(single_label_err.contains("Invalid site.domain host"));
    }

    #[test]
    fn validate_jwks_host_rejects_ip_literals() {
        let ipv4_err = validate_jwks_host("127.0.0.1").unwrap_err().to_string();
        assert!(ipv4_err.contains("Invalid site.domain host"));

        let link_local_err = validate_jwks_host("169.254.169.254")
            .unwrap_err()
            .to_string();
        assert!(link_local_err.contains("Invalid site.domain host"));

        let public_ip_err = validate_jwks_host("8.8.8.8").unwrap_err().to_string();
        assert!(public_ip_err.contains("Invalid site.domain host"));

        let ipv6_err = validate_jwks_host("[::1]").unwrap_err().to_string();
        assert!(ipv6_err.contains("Invalid site.domain host"));
    }
}
