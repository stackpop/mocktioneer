use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use edgezero_core::context::RequestContext;
use edgezero_core::http::StatusCode;
use edgezero_core::outbound::{OutboundCachePolicy, OutboundRequest};
use edgezero_core::{EdgeError, MonotonicClock, MonotonicInstant};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use url::Host;

const JWKS_CACHE_TTL: Duration = Duration::from_mins(10);
const SIGNING_VERSION: &str = "1.1";

/// Maximum allowed clock skew for timestamp freshness check (5 minutes in milliseconds).
const TS_FRESHNESS_WINDOW_MS: u64 = 5 * 60 * 1000;

/// Maximum JWKS response body size (64 KiB).
const MAX_JWKS_BODY_BYTES: u64 = 64 * 1024;
const JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

// Bounds the owned host/key allocations, not process RSS or allocator overhead.
const MAX_JWKS_CACHE_BYTES: usize = 2 * 1024 * 1024;
const MAX_JWKS_CACHE_ENTRIES: usize = 32;

static JWKS_CACHE: LazyLock<Mutex<JwksCache>> = LazyLock::new(|| Mutex::new(JwksCache::default()));

#[derive(Debug, Clone, Deserialize)]
struct JwkKey {
    kid: String,
    x: String, // Base64url-encoded Ed25519 public key
}

#[derive(Default)]
struct JwksCache {
    charged_bytes: usize,
    entries: HashMap<String, CachedJwks>,
}

struct CachedJwks {
    charge: usize,
    fetched_at: MonotonicInstant,
    jwks: Arc<JwksResponse>,
}

#[derive(Debug, Clone, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

// Canonical hostname syntax only; this does not authorize destinations or validate DNS answers.
struct ValidatedJwksHost(String);

impl ValidatedJwksHost {
    fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }

    fn new(domain: &str) -> Result<Self, VerificationError> {
        validate_jwks_host(domain).map(Self)
    }
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

#[expect(
    clippy::arithmetic_side_effects,
    reason = "entry charges sum to charged_bytes and every accepted total is bounded by 2 MiB"
)]
impl JwksCache {
    fn get(&mut self, host: &str, now: MonotonicInstant) -> Option<Arc<JwksResponse>> {
        if let Some(entry) = self.entries.get(host) {
            if cache_entry_is_fresh(entry, now) {
                log::debug!("JWKS cache hit for {host}");
                return Some(Arc::clone(&entry.jwks));
            }
            log::debug!("JWKS cache expired for {host}");
            self.remove(host);
        } else {
            log::debug!("JWKS cache miss for {host}");
        }
        None
    }

    fn insert(&mut self, host: String, jwks: Arc<JwksResponse>, now: MonotonicInstant) {
        // A rejected replacement must also discard the previous version.
        self.remove(&host);
        let Some(charge) = jwks_charge(
            host.capacity(),
            jwks.keys.capacity(),
            jwks.keys
                .iter()
                .map(|key| (key.kid.capacity(), key.x.capacity())),
        ) else {
            return;
        };
        if charge > MAX_JWKS_CACHE_BYTES {
            return;
        }
        self.entries.retain(|_, entry| {
            let fresh = cache_entry_is_fresh(entry, now);
            if !fresh {
                self.charged_bytes -= entry.charge;
            }
            fresh
        });
        while self.entries.len() >= MAX_JWKS_CACHE_ENTRIES
            || charge > MAX_JWKS_CACHE_BYTES - self.charged_bytes
        {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(cached_host, entry)| (entry.fetched_at, *cached_host))
                .map(|(cached_host, _)| cached_host.clone());
            if let Some(oldest_host) = oldest {
                self.remove(&oldest_host);
            } else {
                break;
            }
        }
        self.charged_bytes += charge;
        self.entries.insert(
            host,
            CachedJwks {
                fetched_at: now,
                jwks,
                charge,
            },
        );
    }
    fn remove(&mut self, host: &str) {
        if let Some(entry) = self.entries.remove(host) {
            self.charged_bytes -= entry.charge;
        }
    }
}

fn jwks_charge(
    host_capacity: usize,
    keys_capacity: usize,
    key_capacities: impl IntoIterator<Item = (usize, usize)>,
) -> Option<usize> {
    let base = keys_capacity
        .checked_mul(size_of::<JwkKey>())?
        .checked_add(host_capacity)?;
    key_capacities
        .into_iter()
        .try_fold(base, |charge, (kid, x)| {
            charge.checked_add(kid)?.checked_add(x)
        })
}

fn cache_entry_is_fresh(entry: &CachedJwks, now: MonotonicInstant) -> bool {
    now.checked_duration_since(entry.fetched_at)
        .is_some_and(|age| age < JWKS_CACHE_TTL)
}

fn jwks_body_too_large_error() -> VerificationError {
    VerificationError::HttpError(format!(
        "JWKS response body exceeds {MAX_JWKS_BODY_BYTES} byte limit"
    ))
}

fn outbound_error_detail(error: &EdgeError) -> String {
    if error.status() == StatusCode::INTERNAL_SERVER_ERROR {
        "internal error".to_owned()
    } else {
        error.to_string()
    }
}

fn jwks_transport_error(error: &EdgeError, stage: &str) -> VerificationError {
    if matches!(error, EdgeError::ResponseTooLarge { .. }) {
        jwks_body_too_large_error()
    } else {
        VerificationError::HttpError(format!("{stage}: {}", outbound_error_detail(error)))
    }
}

async fn fetch_jwks(
    ctx: &RequestContext,
    validated_host: &ValidatedJwksHost,
) -> Result<JwksResponse, VerificationError> {
    let host = validated_host.as_str();
    log::debug!("Fetching JWKS for {host}");
    let jwks_url = format!("https://{host}/.well-known/trusted-server.json");
    let request = OutboundRequest::get(&jwks_url)
        .map_err(|error| VerificationError::HttpError(format!("Invalid JWKS URL: {error}")))?
        .timeout(JWKS_FETCH_TIMEOUT)
        .max_response_bytes(MAX_JWKS_BODY_BYTES)
        .cache_policy(OutboundCachePolicy::PlatformDefault);
    let client = ctx
        .http_client()
        .ok_or_else(|| VerificationError::HttpError("Proxy not available".to_owned()))?;
    let response = client
        .send(request)
        .await
        .map_err(|error| jwks_transport_error(&error, "JWKS fetch failed"))?;
    if response.status() != StatusCode::OK {
        return Err(VerificationError::HttpError(format!(
            "JWKS server returned status: {}",
            response.status()
        )));
    }
    let bytes = response
        .into_bytes_bounded(MAX_JWKS_BODY_BYTES)
        .await
        .map_err(|error| jwks_transport_error(&error, "Stream read failed"))?;
    let parsed: TrustedServerResponse = serde_json::from_slice(&bytes)
        .map_err(|error| VerificationError::HttpError(format!("JWKS parse failed: {error}")))?;
    log::debug!("Fetched JWKS for {host}");
    Ok(parsed.jwks)
}

async fn load_jwks(
    cache: &Mutex<JwksCache>,
    clock: &MonotonicClock,
    ctx: &RequestContext,
    domain: &str,
) -> Result<Arc<JwksResponse>, VerificationError> {
    let host = ValidatedJwksHost::new(domain)?;
    match cache.lock() {
        Ok(mut cached) => {
            if let Some(jwks) = cached.get(host.as_str(), clock.now()) {
                return Ok(jwks);
            }
        }
        Err(_) => log::debug!(
            "JWKS cache lock poisoned; bypassing cache for {}",
            host.as_str()
        ),
    }

    let jwks = Arc::new(fetch_jwks(ctx, &host).await?);
    if let Ok(mut cached) = cache.lock() {
        cached.insert(host.into_string(), Arc::clone(&jwks), clock.now());
    }
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
    verify_request_id_signature_with_cache(
        &JWKS_CACHE,
        &ctx.monotonic_clock(),
        ctx,
        request_id,
        ext,
        site_domain,
    )
    .await
}

async fn verify_request_id_signature_with_cache(
    cache: &Mutex<JwksCache>,
    clock: &MonotonicClock,
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

    let jwks = load_jwks(cache, clock, ctx, site_domain).await?;
    let public_key = find_public_key(&jwks, key_id)?;
    verify_ed25519_signature(public_key, signature, &payload)?;

    Ok(key_id.to_owned())
}

#[cfg(test)]
mod tests {
    use edgezero_core::Deadline;
    use edgezero_core::http::{HeaderMap, HeaderValue};
    use edgezero_core::outbound::{
        HttpClient, OutboundBatch, OutboundHttpClient, OutboundRequestParts, OutboundResponse,
        ResponseBodyPolicy, ResponseMode,
    };
    use futures::stream::iter;
    use std::io::Error as IoError;
    use std::panic::catch_unwind;
    use std::ptr::eq;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use edgezero_core::body::Body;
    use edgezero_core::http::Method;
    use edgezero_core::http::request_builder;
    use edgezero_core::params::PathParams;
    use futures::executor::block_on;
    use std::collections::HashMap;

    use super::*;

    #[derive(Clone)]
    enum StubReply {
        BoundedResponse {
            body_error: bool,
            chunks: Vec<Vec<u8>>,
            declared: Option<u64>,
            status: StatusCode,
        },
        Bytes(StatusCode, Vec<u8>),
        Internal,
        SendBodyError,
        SendLimit,
        StreamError,
        StreamLimit,
        Timeout,
        Transport,
    }

    #[derive(Debug)]
    struct CapturedRequest {
        cache: OutboundCachePolicy,
        method: Method,
        mode: ResponseMode,
        timeout: Option<Duration>,
        uri: String,
    }

    struct StubClient {
        calls: Arc<AtomicUsize>,
        captured: Arc<Mutex<Vec<CapturedRequest>>>,
        on_send: Option<Arc<dyn Fn() + Send + Sync>>,
        reply: StubReply,
    }

    #[async_trait::async_trait(?Send)]
    impl OutboundHttpClient for StubClient {
        async fn send(&self, request: OutboundRequest) -> Result<OutboundResponse, EdgeError> {
            use edgezero_core::outbound::{ContentTypeProvenance, OutboundResponse};
            self.calls.fetch_add(1, Ordering::SeqCst);
            if let Some(on_send) = &self.on_send {
                on_send();
            }
            let parts = request.into_parts();
            self.captured.lock().unwrap().push(CapturedRequest {
                uri: parts.uri.to_string(),
                method: parts.method.clone(),
                mode: parts.response_mode,
                timeout: parts.timeout,
                cache: parts.cache_policy,
            });
            let (status, body) = match &self.reply {
                StubReply::Bytes(status, bytes) => (*status, Body::from_bytes(bytes.clone())),
                StubReply::BoundedResponse {
                    status,
                    declared,
                    chunks,
                    body_error,
                } => {
                    use edgezero_core::outbound::{
                        apply_response_body_policy, collect_response_stream,
                    };
                    let policy = response_body_policy(&parts, *status);
                    let mut headers = HeaderMap::new();
                    if let Some(length) = declared {
                        headers.insert(
                            "content-length",
                            HeaderValue::from_str(&length.to_string()).unwrap(),
                        );
                    }
                    let mut stream_chunks: Vec<_> = chunks
                        .iter()
                        .cloned()
                        .map(|bytes| Ok(bytes.into()))
                        .collect();
                    if *body_error {
                        stream_chunks
                            .push(Err(EdgeError::bad_gateway("upstream response body failed")));
                    }
                    let stream = apply_response_body_policy(
                        Box::pin(iter(stream_chunks)),
                        &mut headers,
                        policy,
                    )?;
                    let bytes = collect_response_stream(stream, MAX_JWKS_BODY_BYTES).await?;
                    (*status, Body::from_bytes(bytes))
                }
                StubReply::Transport => {
                    return Err(EdgeError::bad_gateway("upstream transport failed"));
                }
                StubReply::SendLimit => {
                    return Err(EdgeError::response_too_large(
                        "outbound response exceeds limit",
                    ));
                }
                StubReply::SendBodyError => {
                    return Err(EdgeError::bad_gateway("upstream response body failed"));
                }
                StubReply::Internal => {
                    return Err(EdgeError::internal(IoError::other(
                        "PRIVATE_PROVIDER_SENTINEL",
                    )));
                }
                StubReply::Timeout => {
                    return Err(EdgeError::gateway_timeout("upstream request timed out"));
                }
                StubReply::StreamError => (
                    StatusCode::OK,
                    Body::from_stream(iter([Err(EdgeError::bad_gateway(
                        "upstream response body failed",
                    ))])),
                ),
                StubReply::StreamLimit => {
                    (StatusCode::OK, Body::from_bytes(vec![b' '; 0x0001_0001]))
                }
            };
            Ok(OutboundResponse::new(
                parts.method,
                status,
                HeaderMap::new(),
                body,
                edgezero_core::MonotonicClock::default(),
                ContentTypeProvenance::Unknown,
            ))
        }

        fn start_batch_until(
            &self,
            requests: Vec<OutboundRequest>,
            _cutoff: Deadline,
        ) -> OutboundBatch {
            OutboundBatch::cutoff(requests.len())
        }
    }

    fn response_body_policy(
        parts: &OutboundRequestParts,
        status: StatusCode,
    ) -> ResponseBodyPolicy {
        ResponseBodyPolicy {
            max_brotli_window_bits: parts.max_brotli_window_bits,
            max_chunk_bytes: parts.max_chunk_bytes,
            max_decoded_response_bytes: parts.max_decoded_response_bytes,
            max_decoder_bytes: parts.max_decoder_bytes,
            max_encoded_response_bytes: parts.max_encoded_response_bytes,
            response_decoding: parts.response_decoding,
            response_mode: parts.response_mode,
            status,
        }
    }

    fn client_context(
        reply: StubReply,
    ) -> (
        RequestContext,
        Arc<Mutex<Vec<CapturedRequest>>>,
        Arc<AtomicUsize>,
    ) {
        client_context_with_hook(reply, None)
    }

    fn client_context_with_hook(
        reply: StubReply,
        on_send: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> (
        RequestContext,
        Arc<Mutex<Vec<CapturedRequest>>>,
        Arc<AtomicUsize>,
    ) {
        let captured = Arc::new(Mutex::new(Vec::new()));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut request = request_builder().uri("/").body(Body::empty()).unwrap();
        request
            .extensions_mut()
            .insert(HttpClient::with_client(StubClient {
                reply,
                calls: Arc::clone(&calls),
                captured: Arc::clone(&captured),
                on_send,
            }));
        (
            RequestContext::new(request, PathParams::new(HashMap::new())),
            captured,
            calls,
        )
    }

    #[test]
    fn jwks_request_uses_buffered_limit_timeout_and_platform_cache() {
        let (ctx, captured, _) = client_context(StubReply::Bytes(
            StatusCode::OK,
            br#"{"jwks":{"keys":[]}}"#.to_vec(),
        ));
        assert!(
            block_on(fetch_jwks(
                &ctx,
                &ValidatedJwksHost::new("example.com").unwrap(),
            ))
            .unwrap()
            .keys
            .is_empty()
        );
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].uri,
            "https://example.com/.well-known/trusted-server.json"
        );
        assert_eq!(
            requests[0].mode,
            ResponseMode::Buffered {
                max_bytes: 0x0001_0000
            }
        );
        assert_eq!(requests[0].method, Method::GET);
        assert_eq!(requests[0].timeout, Some(Duration::from_secs(5)));
        assert_eq!(requests[0].cache, OutboundCachePolicy::PlatformDefault);
    }

    #[test]
    fn outbound_internal_errors_keep_diagnostics_redacted() {
        use edgezero_core::error::StoreExtractionReason;
        assert_eq!(
            outbound_error_detail(&EdgeError::internal(IoError::other("private diagnostic"))),
            "internal error"
        );
        for reason in [
            StoreExtractionReason::BackendFailure,
            StoreExtractionReason::IntegrityMismatch,
            StoreExtractionReason::InvalidSecretValue,
            StoreExtractionReason::MalformedEnvelope,
            StoreExtractionReason::MissingRegistry,
            StoreExtractionReason::StructuralLimit,
            StoreExtractionReason::UnsupportedVersion,
            StoreExtractionReason::UnknownStore,
            StoreExtractionReason::ValueTooLarge,
        ] {
            let error = EdgeError::store_extraction(reason, "private diagnostic", None);
            assert_eq!(
                outbound_error_detail(&error),
                "internal error",
                "{reason:?}"
            );
        }
        for error in [
            EdgeError::bad_gateway("public transport message"),
            EdgeError::service_unavailable("public availability message"),
            EdgeError::bad_request("public request message"),
        ] {
            assert_eq!(outbound_error_detail(&error), error.to_string());
        }
    }

    #[test]
    fn jwks_failures_preserve_full_public_reason_strings() {
        for (reply, expected) in [
            (
                StubReply::Transport,
                "HTTP error: JWKS fetch failed: upstream transport failed",
            ),
            (
                StubReply::BoundedResponse {
                    status: StatusCode::BAD_GATEWAY,
                    declared: Some(0x0001_0001),
                    chunks: vec![b"non-200 payload".to_vec()],
                    body_error: false,
                },
                "HTTP error: JWKS response body exceeds 65536 byte limit",
            ),
            (
                StubReply::BoundedResponse {
                    status: StatusCode::BAD_GATEWAY,
                    declared: None,
                    chunks: vec![vec![b'x'; 0x8000], vec![b'x'; 0x8001]],
                    body_error: false,
                },
                "HTTP error: JWKS response body exceeds 65536 byte limit",
            ),
            (
                StubReply::BoundedResponse {
                    status: StatusCode::BAD_GATEWAY,
                    declared: None,
                    chunks: vec![b"non-200 payload".to_vec()],
                    body_error: true,
                },
                "HTTP error: JWKS fetch failed: upstream response body failed",
            ),
            (
                StubReply::SendLimit,
                "HTTP error: JWKS response body exceeds 65536 byte limit",
            ),
            (
                StubReply::StreamLimit,
                "HTTP error: JWKS response body exceeds 65536 byte limit",
            ),
            (
                StubReply::SendBodyError,
                "HTTP error: JWKS fetch failed: upstream response body failed",
            ),
            (
                StubReply::Internal,
                "HTTP error: JWKS fetch failed: internal error",
            ),
            (
                StubReply::Timeout,
                "HTTP error: JWKS fetch failed: upstream request timed out",
            ),
            (
                StubReply::StreamError,
                "HTTP error: Stream read failed: upstream response body failed",
            ),
            (
                StubReply::Bytes(StatusCode::BAD_GATEWAY, b"invalid json".to_vec()),
                "HTTP error: JWKS server returned status: 502 Bad Gateway",
            ),
            (
                StubReply::Bytes(StatusCode::OK, b"invalid json".to_vec()),
                "HTTP error: JWKS parse failed: expected value at line 1 column 1",
            ),
        ] {
            let (ctx, _, _) = client_context(reply);
            assert_eq!(
                block_on(fetch_jwks(
                    &ctx,
                    &ValidatedJwksHost::new("example.com").unwrap(),
                ))
                .unwrap_err()
                .to_string(),
                expected
            );
        }
        assert_eq!(
            block_on(fetch_jwks(
                &create_test_context(),
                &ValidatedJwksHost::new("example.com").unwrap(),
            ))
            .unwrap_err()
            .to_string(),
            "HTTP error: Proxy not available"
        );
    }

    fn decode_fixture(coding: &str, encoded: &[u8]) -> Result<Vec<u8>, EdgeError> {
        use edgezero_core::outbound::{apply_response_body_policy, collect_response_stream};
        let parts = OutboundRequest::get("https://example.com/")?
            .max_response_bytes(MAX_JWKS_BODY_BYTES)
            .into_parts();
        let policy = response_body_policy(&parts, StatusCode::OK);
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-encoding",
            HeaderValue::from_str(coding)
                .map_err(|error| EdgeError::bad_request(error.to_string()))?,
        );
        let source = Box::pin(iter([Ok(encoded.to_vec().into())]));
        let stream = apply_response_body_policy(source, &mut headers, policy)?;
        block_on(collect_response_stream(stream, MAX_JWKS_BODY_BYTES)).map(|bytes| bytes.to_vec())
    }

    #[test]
    fn recognized_jwks_codings_decode_before_the_final_buffered_cap() {
        let gzip_bytes = decode_fixture(
            "gzip",
            &[
                31, 139, 8, 0, 0, 0, 0, 0, 0, 19, 171, 86, 202, 42, 207, 46, 86, 178, 170, 86, 202,
                78, 173, 44, 86, 178, 138, 142, 173, 173, 5, 0, 177, 230, 86, 195, 20, 0, 0, 0,
            ],
        )
        .unwrap();
        let gzip_parsed: TrustedServerResponse = serde_json::from_slice(&gzip_bytes).unwrap();
        assert!(gzip_parsed.jwks.keys.is_empty());
        let brotli_bytes = decode_fixture(
            "br",
            &[
                139, 9, 128, 123, 34, 106, 119, 107, 115, 34, 58, 123, 34, 107, 101, 121, 115, 34,
                58, 91, 93, 125, 125, 3,
            ],
        )
        .unwrap();
        let brotli_parsed: TrustedServerResponse = serde_json::from_slice(&brotli_bytes).unwrap();
        assert!(brotli_parsed.jwks.keys.is_empty());
        let deflate_bytes = decode_fixture(
            "deflate",
            &[
                120, 156, 171, 86, 202, 42, 207, 46, 86, 178, 170, 86, 202, 78, 173, 44, 86, 178,
                138, 142, 173, 173, 5, 0, 73, 150, 7, 32,
            ],
        )
        .unwrap();
        let deflate_parsed: TrustedServerResponse = serde_json::from_slice(&deflate_bytes).unwrap();
        assert!(deflate_parsed.jwks.keys.is_empty());
        assert!(matches!(
            decode_fixture(
                "gzip",
                &[
                    31, 139, 8, 0, 0, 0, 0, 0, 0, 19, 237, 193, 129, 0, 0, 0, 0, 195, 32, 149, 249,
                    83, 254, 32, 85, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 13, 1, 180, 186, 30, 1, 0,
                    1, 0
                ]
            ),
            Err(EdgeError::ResponseTooLarge { .. })
        ));
        assert!(matches!(
            decode_fixture("br", &[91, 0, 0, 129, 95, 2, 36, 30, 11, 4, 116, 239, 1, 0]),
            Err(EdgeError::ResponseTooLarge { .. })
        ));
        assert!(matches!(
            decode_fixture(
                "deflate",
                &[
                    120, 156, 237, 193, 129, 0, 0, 0, 0, 195, 32, 149, 249, 83, 254, 32, 85, 1, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 192, 13, 17, 16, 2, 1
                ]
            ),
            Err(EdgeError::ResponseTooLarge { .. })
        ));
    }

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
        assert!(
            err.to_string()
                .contains("Missing ext.trusted_server.request_host")
        );
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
        assert!(
            err.to_string()
                .contains("Missing ext.trusted_server.request_scheme")
        );
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

    fn empty_jwks() -> JwksResponse {
        JwksResponse { keys: Vec::new() }
    }

    fn controlled_clock() -> (MonotonicClock, Arc<Mutex<MonotonicInstant>>) {
        let now = Arc::new(Mutex::new(MonotonicInstant::now()));
        let observed = Arc::clone(&now);
        (MonotonicClock::new(move || *observed.lock().unwrap()), now)
    }

    fn assert_accounting(cache: &JwksCache) {
        assert_eq!(
            cache.charged_bytes,
            cache
                .entries
                .values()
                .map(|entry| entry.charge)
                .sum::<usize>()
        );
        assert!(cache.charged_bytes <= MAX_JWKS_CACHE_BYTES);
        assert!(cache.entries.len() <= MAX_JWKS_CACHE_ENTRIES);
    }

    fn jwks_with_charge(host: &str, charge: usize) -> JwksResponse {
        let key = JwkKey {
            kid: String::new(),
            x: String::with_capacity(
                charge
                    .checked_sub(host.len())
                    .unwrap()
                    .checked_sub(size_of::<JwkKey>())
                    .unwrap(),
            ),
        };
        JwksResponse { keys: vec![key] }
    }

    #[test]
    fn cache_hits_share_key_storage_without_changing_accounting() {
        let now = MonotonicInstant::now();
        let mut cache = JwksCache::default();
        cache.insert(
            "example.com".to_owned(),
            Arc::new(JwksResponse {
                keys: vec![JwkKey {
                    kid: "key".to_owned(),
                    x: "public-key".to_owned(),
                }],
            }),
            now,
        );
        let charged = cache.charged_bytes;
        let first = cache.get("example.com", now).unwrap();
        let second = cache.get("example.com", now).unwrap();
        assert!(eq(first.keys.as_ptr(), second.keys.as_ptr()));
        assert_eq!(cache.charged_bytes, charged);
        assert_accounting(&cache);
        cache.remove("example.com");
        assert_eq!(first.keys[0].kid, "key");
        assert_eq!(second.keys[0].x, "public-key");
        assert_accounting(&cache);
    }

    #[test]
    fn cache_ttl_boundary_and_backwards_clock_expire() {
        let now = MonotonicInstant::now();
        let mut cache = JwksCache::default();
        cache.insert("example.com".to_owned(), Arc::new(empty_jwks()), now);
        assert!(
            cache
                .get(
                    "example.com",
                    (now + JWKS_CACHE_TTL)
                        .checked_sub(Duration::from_nanos(1))
                        .unwrap()
                )
                .is_some()
        );
        assert!(cache.get("example.com", now + JWKS_CACHE_TTL).is_none());
        assert_accounting(&cache);
        cache.insert(
            "example.com".to_owned(),
            Arc::new(empty_jwks()),
            now + Duration::from_secs(1),
        );
        assert!(cache.get("example.com", now).is_none());
        assert_accounting(&cache);
        assert_eq!(cache.charged_bytes, 0);
    }

    #[test]
    fn cache_evicts_oldest_then_hostname_on_equal_age() {
        let now = MonotonicInstant::now();
        let mut cache = JwksCache::default();
        for i in 0..MAX_JWKS_CACHE_ENTRIES {
            cache.insert(
                format!("host-{i:02}.example"),
                Arc::new(empty_jwks()),
                now + Duration::from_secs(u64::try_from(i).unwrap()),
            );
        }
        cache.insert(
            "new.example".to_owned(),
            Arc::new(empty_jwks()),
            now + Duration::from_secs(32),
        );
        assert!(!cache.entries.contains_key("host-00.example"));
        assert!(cache.entries.contains_key("host-01.example"));
        assert_accounting(&cache);
        let mut cache_2 = JwksCache::default();
        for i in (0..MAX_JWKS_CACHE_ENTRIES).rev() {
            cache_2.insert(format!("host-{i:02}.example"), Arc::new(empty_jwks()), now);
        }
        cache_2.insert("new.example".to_owned(), Arc::new(empty_jwks()), now);
        assert!(!cache_2.entries.contains_key("host-00.example"));
        assert_accounting(&cache_2);
    }

    #[test]
    fn cache_charges_actual_owned_capacity_and_exact_byte_boundary() {
        let now = MonotonicInstant::now();
        let mut host = String::with_capacity(100);
        host.push_str("spare.example");
        let mut jwks = empty_jwks();
        jwks.keys.reserve_exact(8);
        jwks.keys.push(JwkKey {
            kid: String::with_capacity(70),
            x: String::with_capacity(90),
        });
        let expected = host.capacity() + jwks.keys.capacity() * size_of::<JwkKey>() + 160;
        let mut cache = JwksCache::default();
        cache.insert(host, Arc::new(jwks), now);
        assert_eq!(cache.charged_bytes, expected);
        assert_accounting(&cache);
        let mut cache_2 = JwksCache::default();
        cache_2.insert(
            "budget.example".to_owned(),
            Arc::new(jwks_with_charge("budget.example", MAX_JWKS_CACHE_BYTES)),
            now,
        );
        assert_eq!(cache_2.charged_bytes, MAX_JWKS_CACHE_BYTES);
        assert_accounting(&cache_2);
        let mut cache_3 = JwksCache::default();
        cache_3.insert(
            "budget.example".to_owned(),
            Arc::new(jwks_with_charge("budget.example", MAX_JWKS_CACHE_BYTES + 1)),
            now,
        );
        assert!(cache_3.entries.is_empty());
        assert_accounting(&cache_3);
    }

    #[test]
    fn cache_charge_overflow_is_checked_without_allocating() {
        assert_eq!(jwks_charge(0, usize::MAX, []), None);
        assert_eq!(jwks_charge(usize::MAX, 1, []), None);
        assert_eq!(jwks_charge(0, 0, [(usize::MAX, 1)]), None);
        assert_eq!(jwks_charge(1, 0, [(usize::MAX, 0)]), None);
    }

    #[test]
    fn cache_replacement_expiry_and_byte_eviction_preserve_accounting() {
        let now = MonotonicInstant::now();
        let mut cache = JwksCache::default();
        cache.insert("old.example".to_owned(), Arc::new(empty_jwks()), now);
        cache.insert(
            "live.example".to_owned(),
            Arc::new(empty_jwks()),
            now + Duration::from_secs(1),
        );
        cache.insert(
            "new.example".to_owned(),
            Arc::new(empty_jwks()),
            now + JWKS_CACHE_TTL,
        );
        assert!(!cache.entries.contains_key("old.example"));
        assert!(cache.entries.contains_key("live.example"));
        assert_accounting(&cache);
        cache.insert(
            "live.example".to_owned(),
            Arc::new(jwks_with_charge("live.example", MAX_JWKS_CACHE_BYTES)),
            now + JWKS_CACHE_TTL,
        );
        assert_eq!(cache.entries.len(), 1);
        assert_eq!(cache.charged_bytes, MAX_JWKS_CACHE_BYTES);
        assert_accounting(&cache);
        cache.insert(
            "live.example".to_owned(),
            Arc::new(empty_jwks()),
            now + JWKS_CACHE_TTL,
        );
        assert_eq!(cache.charged_bytes, "live.example".len());
        assert_accounting(&cache);
    }

    #[test]
    fn oversized_replacement_removes_old_without_evicting_other_hosts() {
        let now = MonotonicInstant::now();
        let mut cache = JwksCache::default();
        cache.insert("old.example".to_owned(), Arc::new(empty_jwks()), now);
        cache.insert("other.example".to_owned(), Arc::new(empty_jwks()), now);
        cache.insert(
            "old.example".to_owned(),
            Arc::new(jwks_with_charge("old.example", MAX_JWKS_CACHE_BYTES + 1)),
            now,
        );
        assert!(!cache.entries.contains_key("old.example"));
        assert!(cache.entries.contains_key("other.example"));
        assert_accounting(&cache);
    }

    #[test]
    fn near_body_limit_many_keys_charge_exceeds_serialized_size() {
        let keys = vec![serde_json::json!({ "kid": "", "x": "" }); 3500];
        let bytes = serde_json::to_vec(&serde_json::json!({"jwks": {"keys": keys}})).unwrap();
        assert!(
            bytes.len() > 60 * 1024 && bytes.len() <= usize::try_from(MAX_JWKS_BODY_BYTES).unwrap()
        );
        let parsed: TrustedServerResponse = serde_json::from_slice(&bytes).unwrap();
        let mut cache = JwksCache::default();
        cache.insert(
            "many.example".to_owned(),
            Arc::new(parsed.jwks),
            MonotonicInstant::now(),
        );
        assert!(cache.charged_bytes > bytes.len());
        assert_accounting(&cache);
    }

    #[test]
    fn loader_canonicalizes_aliases_before_lookup_and_fetch() {
        let (ctx, captured, calls) = client_context(StubReply::Bytes(
            StatusCode::OK,
            br#"{"jwks":{"keys":[]}}"#.to_vec(),
        ));
        let cache = Mutex::new(JwksCache::default());
        let (clock, _) = controlled_clock();
        let large_alias = format!(
            "{}Example.COM.{}",
            " ".repeat(512 * 1024),
            " ".repeat(512 * 1024)
        );
        for alias in ["Example.COM", "example.com.", large_alias.as_str()] {
            block_on(load_jwks(&cache, &clock, &ctx, alias)).unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            captured.lock().unwrap()[0].uri,
            "https://example.com/.well-known/trusted-server.json"
        );
        let cached = cache.lock().unwrap();
        assert_eq!(cached.entries.len(), 1);
        assert!(cached.entries.keys().all(|host| host.len() <= 253));
        assert!(cached.entries.contains_key("example.com"));
        assert_accounting(&cached);
    }

    #[test]
    fn loader_refetches_at_exact_ttl_and_when_clock_moves_backwards() {
        let (ctx, _, calls) = client_context(StubReply::Bytes(
            StatusCode::OK,
            br#"{"jwks":{"keys":[]}}"#.to_vec(),
        ));
        let cache = Mutex::new(JwksCache::default());
        let (clock, now) = controlled_clock();
        let start = clock.now();
        block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        *now.lock().unwrap() = (start + JWKS_CACHE_TTL)
            .checked_sub(Duration::from_nanos(1))
            .unwrap();
        block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        *now.lock().unwrap() = start + JWKS_CACHE_TTL;
        block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        *now.lock().unwrap() = start;
        block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn loader_miss_cache_and_hit_share_storage_and_actual_capacity_charge() {
        let bytes = br#"{"jwks":{"keys":[{"kid":"1","x":"x"},{"kid":"2","x":"x"},{"kid":"3","x":"x"},{"kid":"4","x":"x"},{"kid":"5","x":"x"}]}}"#.to_vec();
        let (ctx, _, calls) = client_context(StubReply::Bytes(StatusCode::OK, bytes));
        let cache = Mutex::new(JwksCache::default());
        let (clock, _) = controlled_clock();
        let miss = block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        assert!(miss.keys.capacity() > miss.keys.len());
        let charged = {
            let cached = cache.lock().unwrap();
            let (host, entry) = cached.entries.get_key_value("example.com").unwrap();
            assert!(Arc::ptr_eq(&miss, &entry.jwks));
            let expected = jwks_charge(
                host.capacity(),
                miss.keys.capacity(),
                miss.keys
                    .iter()
                    .map(|key| (key.kid.capacity(), key.x.capacity())),
            )
            .unwrap();
            assert_eq!(cached.charged_bytes, expected);
            assert_accounting(&cached);
            cached.charged_bytes
        };
        let hit = block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        assert!(Arc::ptr_eq(&miss, &hit));
        assert_eq!(cache.lock().unwrap().charged_bytes, charged);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn loader_inserts_after_fetch_and_never_holds_cache_lock_during_send() {
        let cache = Arc::new(Mutex::new(JwksCache::default()));
        let observed_cache = Arc::clone(&cache);
        let (clock, now) = controlled_clock();
        let after_fetch = clock.now() + JWKS_CACHE_TTL;
        let (ctx, _, calls) = client_context_with_hook(
            StubReply::Bytes(StatusCode::OK, br#"{"jwks":{"keys":[]}}"#.to_vec()),
            Some(Arc::new(move || {
                drop(observed_cache.try_lock().unwrap());
                *now.lock().unwrap() = after_fetch;
            })),
        );
        block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        assert_eq!(
            cache.lock().unwrap().entries["example.com"].fetched_at,
            after_fetch
        );
        block_on(load_jwks(&cache, &clock, &ctx, "example.com")).unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    fn poison_cache(cache: &Mutex<JwksCache>) {
        let result = catch_unwind(|| {
            let _guard = cache.lock().unwrap();
            panic!("intentional local-cache poison");
        });
        assert!(result.is_err());
        assert!(cache.is_poisoned());
    }

    #[test]
    fn oversized_cache_entry_rejection_preserves_signature_material() {
        let domain = "oversized.example";
        let (ext, reply) = signed_fixture(domain);
        let mut parsed: TrustedServerResponse = serde_json::from_slice(&reply).unwrap();
        parsed.jwks.keys.push(JwkKey {
            kid: "unused".to_owned(),
            x: "x".repeat(MAX_JWKS_CACHE_BYTES),
        });
        // This insertion fixture exceeds the network body's 64 KiB limit.
        let jwks = Arc::new(parsed.jwks);
        let mut cache = JwksCache::default();
        cache.insert(
            domain.to_owned(),
            Arc::clone(&jwks),
            MonotonicInstant::now(),
        );
        assert!(cache.entries.is_empty());
        assert_accounting(&cache);
        let trusted = &ext["trusted_server"];
        let payload = build_signing_payload(
            "req-success",
            "kid-success",
            domain,
            "https",
            trusted["ts"].as_u64().unwrap(),
            SIGNING_VERSION,
        )
        .unwrap();
        verify_ed25519_signature(
            find_public_key(&jwks, "kid-success").unwrap(),
            trusted["signature"].as_str().unwrap(),
            &payload,
        )
        .unwrap();
    }

    #[test]
    fn signature_verification_bypasses_cache_poisoned_before_lookup() {
        let domain = "poison-before.example";
        let (ext, reply) = signed_fixture(domain);
        let (ctx, _, calls) = client_context(StubReply::Bytes(StatusCode::OK, reply));
        let cache = Mutex::new(JwksCache::default());
        poison_cache(&cache);
        let (clock, _) = controlled_clock();
        for _ in 0_u8..2 {
            assert_eq!(
                block_on(verify_request_id_signature_with_cache(
                    &cache,
                    &clock,
                    &ctx,
                    "req-success",
                    Some(&ext),
                    domain
                ))
                .unwrap(),
                "kid-success"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn signature_verification_bypasses_cache_poisoned_during_fetch() {
        let domain = "poison-in-fetch.example";
        let (ext, reply) = signed_fixture(domain);
        let cache = Arc::new(Mutex::new(JwksCache::default()));
        let observed_cache = Arc::clone(&cache);
        let (ctx, _, _) = client_context_with_hook(
            StubReply::Bytes(StatusCode::OK, reply),
            Some(Arc::new(move || poison_cache(&observed_cache))),
        );
        let (clock, _) = controlled_clock();
        assert_eq!(
            block_on(verify_request_id_signature_with_cache(
                &cache,
                &clock,
                &ctx,
                "req-success",
                Some(&ext),
                domain
            ))
            .unwrap(),
            "kid-success"
        );
        assert!(cache.is_poisoned());
    }

    fn signed_fixture(domain: &str) -> (serde_json::Value, Vec<u8>) {
        use ed25519_dalek::{Signer as _, SigningKey};
        let signing_key = SigningKey::from_bytes(&[42_u8; 32]);
        let timestamp = current_time_ms().unwrap();
        let payload = build_signing_payload(
            "req-success",
            "kid-success",
            domain,
            "https",
            timestamp,
            SIGNING_VERSION,
        )
        .unwrap();
        let ext = serde_json::json!({"trusted_server": {
            "signature": URL_SAFE_NO_PAD.encode(signing_key.sign(payload.as_bytes()).to_bytes()),
            "kid": "kid-success", "version": SIGNING_VERSION, "request_host": domain,
            "request_scheme": "https", "ts": timestamp
        }});
        let reply = serde_json::to_vec(&serde_json::json!({"jwks": {"keys": [{
            "kid": "kid-success", "x": URL_SAFE_NO_PAD.encode(signing_key.verifying_key().as_bytes())
        }]}})).unwrap();
        (ext, reply)
    }

    #[test]
    fn verify_request_id_signature_success_path() {
        let domain = "verify-success.example";
        let (ext, reply) = signed_fixture(domain);
        let (ctx, _, calls) = client_context(StubReply::Bytes(StatusCode::OK, reply));
        let cache = Mutex::new(JwksCache::default());
        let (clock, _) = controlled_clock();
        for _ in 0_u8..2 {
            assert_eq!(
                block_on(verify_request_id_signature_with_cache(
                    &cache,
                    &clock,
                    &ctx,
                    "req-success",
                    Some(&ext),
                    domain
                ))
                .unwrap(),
                "kid-success"
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
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
