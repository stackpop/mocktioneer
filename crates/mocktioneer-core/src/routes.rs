use std::collections::HashMap;
use std::marker::PhantomData;

use async_trait::async_trait;
use edgezero_core::action;
use edgezero_core::context::RequestContext;
use edgezero_core::extractor::{
    ForwardedHost, FromRequest, Headers, ValidatedJson, ValidatedQuery,
};
use edgezero_core::http::{
    header, response_builder, HeaderMap, HeaderValue, Method, Response, StatusCode,
};
use edgezero_core::middleware::{Middleware, Next};
use edgezero_core::{body::Body, error::EdgeError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use validator::{Validate, ValidationError};

use crate::aps::ApsBidRequest;
use crate::render::extract_ec_hash;
use crate::auction::{
    build_aps_response, build_openrtb_response, is_standard_size, standard_sizes,
};
use crate::openrtb::OpenRTBRequest;
use crate::render::{creative_html, info_html, render_svg, render_template_str, SignatureStatus};

#[derive(Deserialize, Validate)]
struct StaticImgQuery {
    #[validate(range(min = 0.0))]
    bid: Option<f64>,
}

#[derive(Deserialize, Validate)]
struct StaticCreativeQuery {
    #[serde(default)]
    pixel_html: Option<bool>,
    #[serde(default)]
    pixel_js: Option<bool>,
}

#[derive(Deserialize, Validate)]
struct PixelQueryParams {
    #[validate(length(min = 1, max = 128))]
    pid: String,
}

#[derive(Deserialize, Validate)]
struct ClickQueryParams {
    #[serde(default)]
    #[validate(length(max = 128))]
    crid: Option<String>,
    #[serde(default)]
    #[validate(range(min = 1))]
    w: Option<i64>,
    #[serde(default)]
    #[validate(range(min = 1))]
    h: Option<i64>,
    #[serde(flatten)]
    extra: HashMap<String, String>,
}

#[derive(Deserialize, Validate)]
struct StaticAssetPath {
    #[validate(custom(function = "validate_static_asset_size"))]
    size: String,
}

enum AssetFormat {
    Svg,
    Html,
}

impl AssetFormat {
    fn suffix(&self) -> &'static str {
        match self {
            AssetFormat::Svg => ".svg",
            AssetFormat::Html => ".html",
        }
    }
}

trait AssetFormatMarker {
    const FORMAT: AssetFormat;

    fn handle_invalid(path: &str, width: i64, height: i64) -> EdgeError;
}

struct SvgSize;
struct HtmlSize;

impl AssetFormatMarker for SvgSize {
    const FORMAT: AssetFormat = AssetFormat::Svg;

    fn handle_invalid(_path: &str, width: i64, height: i64) -> EdgeError {
        log::warn!("non-standard image size {}x{}", width, height);
        EdgeError::validation(format!("non-standard image size {}x{}", width, height))
    }
}

impl AssetFormatMarker for HtmlSize {
    const FORMAT: AssetFormat = AssetFormat::Html;

    fn handle_invalid(path: &str, width: i64, height: i64) -> EdgeError {
        log::warn!("non-standard creative size {}x{}", width, height);
        EdgeError::not_found(path.to_string())
    }
}

#[derive(Clone, Copy)]
struct SizeDimensions {
    width: i64,
    height: i64,
}

impl SizeDimensions {}

struct ValidatedSize<F>(SizeDimensions, PhantomData<F>);

async fn extract_size<F>(ctx: &RequestContext) -> Result<ValidatedSize<F>, EdgeError>
where
    F: AssetFormatMarker,
{
    let params: StaticAssetPath = ctx.path()?;
    params
        .validate()
        .map_err(|err| EdgeError::validation(err.to_string()))?;

    if let Some((width, height)) = parse_size_param(&params.size, F::FORMAT.suffix()) {
        if !is_standard_size(width, height) {
            return Err(F::handle_invalid(ctx.request().uri().path(), width, height));
        }

        return Ok(ValidatedSize(SizeDimensions { width, height }, PhantomData));
    }

    Err(EdgeError::not_found(ctx.request().uri().path()))
}

#[async_trait(?Send)]
impl<F> FromRequest for ValidatedSize<F>
where
    F: AssetFormatMarker + Send + Sync + 'static,
{
    async fn from_request(ctx: &RequestContext) -> Result<Self, EdgeError> {
        extract_size::<F>(ctx).await
    }
}

fn parse_size_param(size: &str, suffix: &str) -> Option<(i64, i64)> {
    let cleaned = size.split(['?', '&']).next().unwrap_or(size);

    if !cleaned.ends_with(suffix) {
        return None;
    }
    let core = &cleaned[..cleaned.len().saturating_sub(suffix.len())];
    let mut it = core.split('x');
    let w = it.next()?.parse::<i64>().ok()?;
    let h = it.next()?.parse::<i64>().ok()?;
    Some((w, h))
}

fn validate_static_asset_size(value: &str) -> Result<(), ValidationError> {
    if parse_size_param(value, ".svg").is_some() || parse_size_param(value, ".html").is_some() {
        return Ok(());
    }

    let mut err = ValidationError::new("invalid_size");
    err.message = Some("expected format <width>x<height>.(svg|html)".into());
    Err(err)
}

fn build_response(status: StatusCode, body: Body) -> Response {
    let mut builder = response_builder().status(status);
    if let Body::Once(bytes) = &body {
        if !bytes.is_empty() {
            builder = builder.header(header::CONTENT_LENGTH, bytes.len().to_string());
        }
    }
    builder
        .body(body)
        .expect("static response builder should not fail")
}

fn apply_cors(headers: &mut HeaderMap) {
    headers.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    headers.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("*, content-type"),
    );
}

pub struct Cors;

#[async_trait(?Send)]
impl Middleware for Cors {
    async fn handle(&self, ctx: RequestContext, next: Next<'_>) -> Result<Response, EdgeError> {
        let method = ctx.request().method().clone();
        let mut response = if method == Method::OPTIONS {
            Ok(options_response())
        } else {
            next.run(ctx).await
        }?;
        apply_cors(response.headers_mut());
        Ok(response)
    }
}

#[action]
pub async fn handle_options() -> Response {
    options_response()
}

fn options_response() -> Response {
    let mut response = build_response(StatusCode::NO_CONTENT, Body::empty());
    response.headers_mut().insert(
        header::ALLOW,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response
}

#[action]
pub async fn handle_root(ForwardedHost(host): ForwardedHost) -> Response {
    let html = info_html(&host);
    let mut response = build_response(StatusCode::OK, Body::text(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

#[action]
pub async fn handle_openrtb_auction(
    RequestContext(ctx): RequestContext,
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<OpenRTBRequest>,
) -> Result<Response, EdgeError> {
    // Capture signature verification status for metadata
    let signature_status = if let Some(domain) = req.site.as_ref().and_then(|s| s.domain.as_deref())
    {
        match crate::verification::verify_request_id_signature(
            &ctx,
            &req.id,
            req.ext.as_ref(),
            domain,
            domain,
        )
        .await
        {
            Ok(kid) => {
                log::info!("✅ Request signature verified with key: {}", kid);
                SignatureStatus::Verified { kid }
            }
            Err(e) => {
                log::error!("❌ Signature verification failed: {}", e);
                SignatureStatus::Failed {
                    reason: e.to_string(),
                }
            }
        }
    } else {
        log::info!("⚠️ Signature verification skipped (no domain)");
        SignatureStatus::NotPresent {
            reason: "No site.domain present in request".to_string(),
        }
    };

    log::info!("auction id={}, imps={}", req.id, req.imp.len());

    // Build response with embedded metadata (signature status + request + response preview)
    let resp = build_openrtb_response(&req, &host, signature_status);
    let body = Body::json(&resp).map_err(|e| {
        log::error!("Failed to serialize OpenRTB response: {}", e);
        EdgeError::internal(e)
    })?;
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

#[action]
pub async fn handle_static_img(
    ValidatedSize(size, _): ValidatedSize<SvgSize>,
    ValidatedQuery(query): ValidatedQuery<StaticImgQuery>,
) -> Response {
    let SizeDimensions {
        width: w,
        height: h,
    } = size;
    let svg = render_svg(w, h, query.bid);
    let mut response = build_response(StatusCode::OK, Body::from(svg));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml"),
    );
    response
}

#[action]
pub async fn handle_static_creatives(
    ValidatedSize(size, _): ValidatedSize<HtmlSize>,
    ValidatedQuery(query): ValidatedQuery<StaticCreativeQuery>,
    ForwardedHost(host): ForwardedHost,
) -> Response {
    let SizeDimensions {
        width: w,
        height: h,
    } = size;
    let pixel_html = query.pixel_html.unwrap_or(true);
    let pixel_js = query.pixel_js.unwrap_or(false);
    let html = creative_html(w, h, pixel_html, pixel_js, &host);
    let mut response = build_response(StatusCode::OK, Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

fn parse_cookie<'a>(cookie_header: &'a str, name: &str) -> Option<&'a str> {
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == name {
                return Some(v.trim());
            }
        }
    }
    None
}

const PIXEL_GIF: &[u8] = include_bytes!("../static/pixel.gif");

const MTKID_COOKIE_NAME: &str = "mtkid";
const MTKID_MAX_AGE: u64 = 60 * 60 * 24 * 365;

/// Read an existing `mtkid` cookie or generate a new one deterministically.
///
/// When no `mtkid` cookie is present, generates a deterministic ID using
/// `SHA-256("mtkid:" || host)` truncated to 32 hex chars. This satisfies the
/// project's determinism requirement (same host always produces the same ID)
/// while still producing unique IDs per deployment.
///
/// Returns `(mtkid_value, Option<set_cookie_header_value>)`.
fn get_or_create_mtkid(headers: &HeaderMap, host: &str) -> (String, Option<String>) {
    let existing = headers
        .get(header::COOKIE)
        .and_then(|c| c.to_str().ok())
        .and_then(|c| parse_cookie(c, MTKID_COOKIE_NAME));

    match existing {
        Some(id) => (id.to_string(), None),
        None => {
            // Deterministic: SHA-256("mtkid:" || host), truncated to 32 hex chars.
            // Same host always produces the same mtkid. Different hosts differ.
            let mut hasher = Sha256::new();
            hasher.update(b"mtkid:");
            hasher.update(host.as_bytes());
            let hash = hasher.finalize();
            let id = hex_encode(&hash)[..32].to_string();
            let cookie_val = format!(
                "{}={}; Path=/; Max-Age={}; SameSite=None; Secure; HttpOnly",
                MTKID_COOKIE_NAME, id, MTKID_MAX_AGE
            );
            (id, Some(cookie_val))
        }
    }
}

#[action]
pub async fn handle_pixel(
    Headers(headers): Headers,
    ForwardedHost(host): ForwardedHost,
    ValidatedQuery(params): ValidatedQuery<PixelQueryParams>,
) -> Response {
    let PixelQueryParams { pid: _ } = params;

    let (_, set_cookie) = get_or_create_mtkid(&headers, &host);

    let mut response = build_response(StatusCode::OK, Body::from(PIXEL_GIF));
    {
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/gif"));
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        );
        headers.insert("Pragma", HeaderValue::from_static("no-cache"));
        headers.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&PIXEL_GIF.len().to_string()).expect("length"),
        );
    }

    if let Some(cookie) = set_cookie {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append("Set-Cookie", value);
        }
    }

    response
}

#[derive(Deserialize, Validate)]
struct ApsWinParams {
    #[validate(length(min = 1))]
    slot: String,
    #[validate(range(min = 0.0))]
    price: f64,
}

#[action]
pub async fn handle_aps_bid(
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<ApsBidRequest>,
) -> Result<Response, EdgeError> {
    log::info!(
        "APS auction pubId={}, slots={}",
        req.pub_id,
        req.slots.len()
    );

    let resp = build_aps_response(&req, &host);
    let body = Body::json(&resp).map_err(|e| {
        log::error!("Failed to serialize APS response: {}", e);
        EdgeError::internal(e)
    })?;
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

#[action]
pub async fn handle_aps_win(ValidatedQuery(params): ValidatedQuery<ApsWinParams>) -> Response {
    log::info!(
        "APS win notification slot={}, price={:.2}",
        params.slot,
        params.price
    );
    build_response(StatusCode::NO_CONTENT, Body::empty())
}

#[action]
pub async fn handle_adserver_mediate(
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<crate::mediation::MediationRequest>,
) -> Result<Response, EdgeError> {
    log::info!(
        "Mediation request for auction '{}' with {} impressions and {} bidder responses",
        req.id,
        req.imp.len(),
        req.ext.bidder_responses.len()
    );

    let resp = crate::mediation::mediate_auction(req, &host);

    log::info!(
        "Mediation complete for auction '{}': {} seatbid(s)",
        resp.id,
        resp.seatbid.len()
    );

    let body = Body::json(&resp).map_err(|e| {
        log::error!("Failed to serialize mediation response: {}", e);
        EdgeError::internal(e)
    })?;
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

#[action]
pub async fn handle_click(ValidatedQuery(params): ValidatedQuery<ClickQueryParams>) -> Response {
    let ClickQueryParams { crid, w, h, extra } = params;
    let crid = crid.unwrap_or_default();
    let w = w.map(|v| v.to_string()).unwrap_or_default();
    let h = h.map(|v| v.to_string()).unwrap_or_default();
    let mut extra_pairs: Vec<_> = extra.into_iter().collect();
    extra_pairs.sort_by(|a, b| a.0.cmp(&b.0));
    let extra_json: Vec<_> = extra_pairs
        .into_iter()
        .map(|(k, v)| serde_json::json!({ "KEY": k, "VALUE": v }))
        .collect();
    log::info!("click crid={}, size={}x{}", crid, w, h);
    const CLICK_TMPL: &str = include_str!("../static/templates/click.html.hbs");
    let html = render_template_str(
        CLICK_TMPL,
        &serde_json::json!({
            "CRID": crid,
            "W": w,
            "H": h,
            "EXTRA": extra_json,
        }),
    );
    let mut response = build_response(StatusCode::OK, Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

/// Returns all standard ad sizes as JSON array.
/// Useful for test fixtures and keeping external configs in sync with STANDARD_SIZES.
///
/// Response format:
/// ```json
/// {
///   "sizes": [
///     {"width": 300, "height": 250},
///     {"width": 728, "height": 90},
///     ...
///   ]
/// }
/// ```
#[action]
pub async fn handle_sizes() -> Response {
    let sizes: Vec<serde_json::Value> = standard_sizes()
        .map(|(w, h)| {
            serde_json::json!({
                "width": w,
                "height": h
            })
        })
        .collect();

    let body = serde_json::json!({ "sizes": sizes });
    let mut response = build_response(StatusCode::OK, Body::from(body.to_string()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

// ---------------------------------------------------------------------------
// Edge Cookie (EC) sync endpoints
// ---------------------------------------------------------------------------

/// The partner ID that mocktioneer uses when registering with trusted-server.
const PARTNER_ID: &str = "mocktioneer";

/// Env var for the bearer token expected on inbound pull sync requests.
const PULL_TOKEN_ENV: &str = "MOCKTIONEER_PULL_TOKEN";

/// Env var for allowed trusted-server domains (comma-separated).
/// When set, `/sync/start` only redirects to domains in this list.
/// When unset, any `ts_domain` is accepted (development mode).
///
/// **WASM note:** `std::env::var` returns `Err` on Cloudflare Workers
/// (no env var support via `std::env`). On that platform, the allowlist
/// is effectively disabled. For production Cloudflare deployments, use
/// a platform-native config mechanism or accept the open-redirect risk
/// in controlled environments.
const TS_ALLOWED_DOMAINS_ENV: &str = "MOCKTIONEER_TS_DOMAINS";

/// Returns true if `s` looks like a valid hostname (no path, auth, port, or fragment).
fn is_valid_hostname(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.contains(['/', '@', ':', '?', '#', ' ', '\t', '\n', '\r'])
}

/// Validates that a string is a valid EC identifier in `{64-hex}.{6-alnum}` format.
fn validate_ec_id(value: &str) -> Result<(), ValidationError> {
    if extract_ec_hash(value).is_none() {
        let mut err = ValidationError::new("invalid_ec_id");
        err.message = Some("ec_id must be in {64-hex}.{6-alnum} format".into());
        return Err(err);
    }
    Ok(())
}

#[derive(Deserialize, Validate)]
struct SyncStartParams {
    /// The trusted-server hostname (e.g., "ts.publisher.com").
    #[validate(length(min = 1, max = 256))]
    ts_domain: String,
}

#[derive(Deserialize, Validate)]
struct SyncDoneParams {
    /// Whether the sync succeeded ("1") or failed ("0").
    ts_synced: String,
    /// Failure reason — present only when ts_synced=0.
    #[serde(default)]
    ts_reason: Option<String>,
}

#[derive(Deserialize, Validate)]
struct ResolveParams {
    /// Full EC identifier in `{64-hex}.{6-alnum}` format.
    #[validate(custom(function = "validate_ec_id"))]
    ec_id: String,
    /// Client IP address.
    #[validate(length(min = 1, max = 45))]
    ip: String,
}

#[derive(Serialize)]
struct ResolveResponse {
    uid: Option<String>,
}

/// `GET /sync/start?ts_domain=publisher.example.com`
///
/// Initiates the pixel sync redirect chain:
/// 1. Reads/sets the `mtkid` cookie (mocktioneer's buyer UID).
/// 2. Redirects to trusted-server's `GET /sync` with `partner=mocktioneer`,
///    `uid={mtkid}`, and `return={self}/sync/done`.
///
/// **Open-redirect protection:** When `MOCKTIONEER_TS_DOMAINS` is set
/// (comma-separated allowlist), the `ts_domain` query param is validated
/// against it. Requests with unlisted domains receive `403 Forbidden`.
/// When unset, any domain is accepted (development/demo mode).
///
/// Additionally, `ts_domain` is always validated as a clean hostname —
/// values containing `/`, `@`, `:`, `?`, `#`, or whitespace are rejected
/// with `400 Bad Request` to prevent path injection even without an allowlist.
#[action]
pub async fn handle_sync_start(
    Headers(headers): Headers,
    ForwardedHost(host): ForwardedHost,
    ValidatedQuery(params): ValidatedQuery<SyncStartParams>,
) -> Response {
    // Reject ts_domain values that contain path/auth/port/fragment characters
    if !is_valid_hostname(&params.ts_domain) {
        log::warn!(
            "EC sync start rejected: ts_domain={} is not a valid hostname",
            sanitize_for_log(&params.ts_domain, 64)
        );
        return build_response(StatusCode::BAD_REQUEST, Body::empty());
    }

    // Validate ts_domain against allowlist when configured
    if let Ok(allowed) = std::env::var(TS_ALLOWED_DOMAINS_ENV) {
        let is_allowed = allowed
            .split(',')
            .any(|d| d.trim().eq_ignore_ascii_case(&params.ts_domain));
        if !is_allowed {
            log::warn!(
                "EC sync start rejected: ts_domain={} not in {}",
                params.ts_domain,
                TS_ALLOWED_DOMAINS_ENV
            );
            return build_response(StatusCode::FORBIDDEN, Body::empty());
        }
    }

    let (mtkid, set_cookie) = get_or_create_mtkid(&headers, &host);

    // Build the return URL (where TS redirects back after sync)
    let scheme = if is_local_host(&host) {
        "http"
    } else {
        "https"
    };
    let return_url = format!("{}://{}/sync/done", scheme, host);

    // Build the redirect to trusted-server's /sync endpoint
    let redirect_url = format!(
        "https://{}/sync?partner={}&uid={}&return={}",
        params.ts_domain,
        PARTNER_ID,
        urlencoding(&mtkid),
        urlencoding(&return_url),
    );

    log::info!(
        "EC sync start: mtkid={}, redirect to {}",
        mtkid,
        redirect_url
    );

    let loc = match HeaderValue::from_str(&redirect_url) {
        Ok(v) => v,
        Err(_) => {
            log::error!("EC sync start: invalid redirect URL: {}", redirect_url);
            return build_response(StatusCode::INTERNAL_SERVER_ERROR, Body::empty());
        }
    };

    let mut response = build_response(StatusCode::FOUND, Body::empty());
    {
        let h = response.headers_mut();
        h.insert(header::LOCATION, loc);
        h.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        );
    }

    if let Some(cookie) = set_cookie {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append("Set-Cookie", value);
        }
    }

    response
}

/// `GET /sync/done?ts_synced=1` or `GET /sync/done?ts_synced=0&ts_reason=no_consent`
///
/// Callback from trusted-server after pixel sync completes. Returns a 1x1 pixel
/// so the browser redirect chain terminates cleanly.
#[action]
pub async fn handle_sync_done(ValidatedQuery(params): ValidatedQuery<SyncDoneParams>) -> Response {
    let success = params.ts_synced == "1";
    let reason = params.ts_reason.as_deref().unwrap_or("none");
    if success {
        log::info!("EC sync done: success");
    } else {
        log::warn!(
            "EC sync done: failed, reason={}",
            sanitize_for_log(reason, 128)
        );
    }

    // Return 1x1 transparent pixel
    let mut response = build_response(StatusCode::OK, Body::from(PIXEL_GIF));
    {
        let h = response.headers_mut();
        h.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/gif"));
        h.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        );
        h.insert(
            header::CONTENT_LENGTH,
            HeaderValue::from_str(&PIXEL_GIF.len().to_string()).expect("length"),
        );
    }
    response
}

/// `GET /resolve?ec_id={64-hex}.{6-alnum}&ip={ip_address}`
///
/// Pull sync resolution endpoint. Trusted-server calls this S2S to resolve
/// an EC identifier + IP to a mocktioneer buyer UID.
///
/// The `ec_id` is the full Edge Cookie value in `{64-hex}.{6-alnum}` format.
/// The 64-hex prefix (hash) is extracted internally and used with the IP to
/// derive a deterministic UID: `SHA-256(ec_hash | ip)` → `mtk-{hash[0:12]}`.
/// Always the same for the same `(ec_id, ip)` pair.
///
/// Authentication: `Authorization: Bearer {token}` validated against
/// `MOCKTIONEER_PULL_TOKEN` env var (constant-time comparison). If the env
/// var is unset, auth is skipped.
///
/// **WASM note:** `std::env::var` returns `Err` on Cloudflare Workers,
/// which means auth is silently disabled on that platform. See
/// `TS_ALLOWED_DOMAINS_ENV` for the same limitation.
#[action]
pub async fn handle_resolve(
    Headers(headers): Headers,
    ValidatedQuery(params): ValidatedQuery<ResolveParams>,
) -> Result<Response, EdgeError> {
    // Check bearer token if configured
    if let Ok(expected_token) = std::env::var(PULL_TOKEN_ENV) {
        let auth_header = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let provided_token = auth_header.strip_prefix("Bearer ").unwrap_or("");
        if !constant_time_token_eq(provided_token, &expected_token) {
            log::warn!(
                "Pull sync auth failed for ec_id={}",
                sanitize_for_log(&params.ec_id, 72)
            );
            return Ok(build_response(StatusCode::UNAUTHORIZED, Body::empty()));
        }
    }

    // Extract the 64-hex hash prefix from the full ec_id, then hash with IP.
    // Validation already confirmed the format, so unwrap is safe here.
    let ec_hash = extract_ec_hash(&params.ec_id).expect("validated ec_id format");

    // Generate deterministic UID from ec_hash + IP: mtk-{sha256(ec_hash|ip)[0:12]}
    let mut hasher = Sha256::new();
    hasher.update(ec_hash.as_bytes());
    hasher.update(b"|");
    hasher.update(params.ip.as_bytes());
    let hash = hasher.finalize();
    let hex = hex_encode(&hash);
    let uid = format!("mtk-{}", &hex[..12]);

    log::info!(
        "Pull sync resolve: ec_id={}..., ip={}, uid={}",
        &params.ec_id[..8],
        params.ip,
        uid
    );

    let body = Body::json(&ResolveResponse { uid: Some(uid) }).map_err(|e| {
        log::error!("Failed to serialize resolve response: {}", e);
        EdgeError::internal(e)
    })?;
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

/// Minimal percent-encoding for URL query parameter values.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push('%');
                out.push(char::from(HEX_CHARS[(b >> 4) as usize]));
                out.push(char::from(HEX_CHARS[(b & 0x0f) as usize]));
            }
        }
    }
    out
}

const HEX_CHARS: [u8; 16] = *b"0123456789ABCDEF";

/// Encode bytes as lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(char::from(b"0123456789abcdef"[(b >> 4) as usize]));
        s.push(char::from(b"0123456789abcdef"[(b & 0x0f) as usize]));
    }
    s
}

/// Constant-time token comparison using `subtle::ConstantTimeEq`.
/// Compares SHA-256 digests to avoid leaking length information.
fn constant_time_token_eq(provided: &str, expected: &str) -> bool {
    let hash_a = Sha256::digest(provided.as_bytes());
    let hash_b = Sha256::digest(expected.as_bytes());
    hash_a.ct_eq(&hash_b).into()
}

/// Returns true if the host looks like a local development address.
fn is_local_host(host: &str) -> bool {
    // Handle bracketed IPv6 with port: [::1]:8787 → ::1
    let hostname = if host.starts_with('[') {
        host.split(']').next().map(|s| &s[1..]).unwrap_or(host)
    } else {
        host.split(':').next().unwrap_or(host)
    };
    hostname == "localhost"
        || hostname == "127.0.0.1"
        || hostname == "::1"
        || hostname.ends_with(".localhost")
}

/// Sanitize a user-supplied string for safe logging.
/// Strips control characters and truncates to `max_len`.
fn sanitize_for_log(s: &str, max_len: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(max_len)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use edgezero_core::body::Body;
    use edgezero_core::context::RequestContext;
    use edgezero_core::error::EdgeError;
    use edgezero_core::http::{request_builder, Method, Response, StatusCode};
    use edgezero_core::params::PathParams;
    use edgezero_core::response::IntoResponse;
    use futures::executor::block_on;
    use std::collections::HashMap;

    fn response_from(result: Result<Response, EdgeError>) -> Response {
        match result {
            Ok(response) => response,
            Err(err) => err.into_response(),
        }
    }

    fn ctx(method: Method, uri: &str, body: Body, params: &[(&str, &str)]) -> RequestContext {
        let mut builder = request_builder();
        builder = builder.method(method).uri(uri);
        let request = builder.body(body).expect("request");
        let map = params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<HashMap<_, _>>();
        RequestContext::new(request, PathParams::new(map))
    }

    #[test]
    fn parse_size_param_parses_suffix() {
        assert_eq!(parse_size_param("300x250.svg", ".svg"), Some((300, 250)));
        assert_eq!(parse_size_param("300x250.html", ".svg"), None);
        assert_eq!(parse_size_param("bad", ".svg"), None);
    }

    #[test]
    fn parse_cookie_extracts_value() {
        let c = "a=1; mtkid=xyz; x=y";
        assert_eq!(parse_cookie(c, "mtkid"), Some("xyz"));
        assert_eq!(parse_cookie(c, "missing"), None);
    }

    #[test]
    fn handle_pixel_sets_cookie_when_absent() {
        let ctx = ctx(Method::GET, "/pixel?pid=test", Body::empty(), &[]);
        let response = response_from(block_on(handle_pixel(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/gif");
        let cookies = response.headers().get_all("set-cookie");
        assert!(cookies
            .iter()
            .any(|c| c.to_str().unwrap_or_default().starts_with("mtkid=")));
    }

    #[test]
    fn handle_pixel_requires_pid() {
        let ctx = ctx(Method::GET, "/pixel", Body::empty(), &[]);
        let response = response_from(block_on(handle_pixel(ctx)));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn handle_pixel_does_not_reset_cookie_when_present() {
        let mut builder = request_builder();
        builder = builder
            .method(Method::GET)
            .uri("/pixel?pid=test")
            .header("Cookie", "mtkid=abc");
        let request = builder.body(Body::empty()).expect("request");
        let ctx = RequestContext::new(request, PathParams::default());
        let response = response_from(block_on(handle_pixel(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().get("set-cookie").is_none());
    }

    #[test]
    fn handle_openrtb_auction_invalid_json_400() {
        let ctx = ctx(
            Method::POST,
            "/openrtb2/auction",
            Body::from("not-json"),
            &[],
        );
        let response = response_from(block_on(handle_openrtb_auction(ctx)));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "application/json");
    }

    #[test]
    fn handle_openrtb_auction_missing_imps_422() {
        let body = serde_json::json!({
            "id": "req-1",
            "imp": []
        });
        let ctx = ctx(
            Method::POST,
            "/openrtb2/auction",
            Body::json(&body).expect("json body"),
            &[],
        );
        let response = response_from(block_on(handle_openrtb_auction(ctx)));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn handle_openrtb_auction_missing_media_422() {
        let body = serde_json::json!({
            "id": "req-2",
            "imp": [
                { "id": "imp-1" }
            ]
        });
        let ctx = ctx(
            Method::POST,
            "/openrtb2/auction",
            Body::json(&body).expect("json body"),
            &[],
        );
        let response = response_from(block_on(handle_openrtb_auction(ctx)));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn handle_static_img_svg_ok_and_nonstandard_422() {
        let ctx_ok = ctx(
            Method::GET,
            "/static/img/300x250.svg?bid=2.50",
            Body::empty(),
            &[("size", "300x250.svg")],
        );
        let response = response_from(block_on(handle_static_img(ctx_ok)));
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/svg+xml");

        let ctx_nonstandard = ctx(
            Method::GET,
            "/static/img/333x222.svg",
            Body::empty(),
            &[("size", "333x222.svg")],
        );
        let response = response_from(block_on(handle_static_img(ctx_nonstandard)));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn handle_static_creatives_html_ok() {
        let mut builder = request_builder();
        builder = builder
            .method(Method::GET)
            .uri("/static/creatives/300x250.html")
            .header(header::HOST, "mocktioneer.edgecompute.app");
        let request = builder.body(Body::empty()).expect("request");
        let ctx = RequestContext::new(
            request,
            PathParams::new(HashMap::from([(
                String::from("size"),
                String::from("300x250.html"),
            )])),
        );
        let response = response_from(block_on(handle_static_creatives(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.starts_with("text/html"));
        let body = response.into_body().into_bytes();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(body_str.contains("data-static-pid=\""));
        assert!(body_str.contains("//mocktioneer.edgecompute.app/pixel?pid="));
        assert!(!body_str.contains("var jsPid = \""));
    }

    #[test]
    fn handle_static_creatives_html_ok_with_js_pixel() {
        let ctx = ctx(
            Method::GET,
            "/static/creatives/300x250.html?pixel_js=true",
            Body::empty(),
            &[("size", "300x250.html")],
        );
        let response = response_from(block_on(handle_static_creatives(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
        assert!(body.contains("data-static-pid=\""));
        assert!(body.contains("var jsPid = \""));
        let static_pid = body
            .split("data-static-pid=\"")
            .nth(1)
            .and_then(|s| s.split('\"').next())
            .expect("static pid");
        let js_pid = body
            .split("var jsPid = \"")
            .nth(1)
            .and_then(|s| s.split('\"').next())
            .expect("js pid");
        assert_ne!(static_pid, js_pid);
    }

    #[test]
    fn handle_static_creatives_html_ok_without_pixel() {
        let ctx = ctx(
            Method::GET,
            "/static/creatives/300x250.html?pixel_html=false",
            Body::empty(),
            &[("size", "300x250.html")],
        );
        let response = response_from(block_on(handle_static_creatives(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().into_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(!body.contains("/pixel"));
        assert!(!body.contains("var jsPid = \""));
    }

    #[test]
    fn handle_static_creatives_html_ok_with_malformed_query_delimiter() {
        let ctx = ctx(
            Method::GET,
            "/static/creatives/300x250.html",
            Body::empty(),
            &[("size", "300x250.html&crid=mocktioneer-1&bid=")],
        );
        let response = response_from(block_on(handle_static_creatives(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn handle_static_img_rejects_negative_bid() {
        let ctx = ctx(
            Method::GET,
            "/static/img/300x250.svg?bid=-1.0",
            Body::empty(),
            &[("size", "300x250.svg")],
        );
        let response = response_from(block_on(handle_static_img(ctx)));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn handle_static_creatives_rejects_invalid_html_pixel_toggle() {
        let ctx = ctx(
            Method::GET,
            "/static/creatives/300x250.html?pixel_html=maybe",
            Body::empty(),
            &[("size", "300x250.html")],
        );
        let response = response_from(block_on(handle_static_creatives(ctx)));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn handle_static_creatives_rejects_invalid_js_pixel_toggle() {
        let ctx = ctx(
            Method::GET,
            "/static/creatives/300x250.html?pixel_js=maybe",
            Body::empty(),
            &[("size", "300x250.html")],
        );
        let response = response_from(block_on(handle_static_creatives(ctx)));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn handle_click_echoes_params() {
        let ctx = ctx(
            Method::GET,
            "/click?crid=abc&w=300&h=250",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_click(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
        assert!(body.contains("abc"));
        assert!(body.contains("300"));
        assert!(body.contains("250"));
        assert!(!body.contains("Additional Parameters"));
    }

    #[test]
    fn handle_root_returns_html() {
        let ctx = ctx(Method::GET, "/", Body::empty(), &[]);
        let response = response_from(block_on(handle_root(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.starts_with("text/html"));
    }

    #[test]
    fn handle_click_lists_additional_params() {
        let ctx = ctx(
            Method::GET,
            "/click?crid=abc&foo=bar&baz=qux",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_click(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
        assert!(body.contains("Additional Parameters"));
        assert!(body.contains("foo"));
        assert!(body.contains("bar"));
        assert!(body.contains("baz"));
        assert!(body.contains("qux"));
    }

    #[test]
    fn handle_aps_bid_valid_request() {
        let body = serde_json::json!({
            "pubId": "5555",
            "slots": [
                {
                    "slotID": "header-banner",
                    "slotName": "header-banner",
                    "sizes": [[728, 90], [970, 250]]
                }
            ],
            "pageUrl": "https://example.com/article",
            "ua": "Mozilla/5.0",
            "timeout": 800
        });
        let ctx = ctx(
            Method::POST,
            "/e/dtb/bid",
            Body::json(&body).expect("json body"),
            &[],
        );
        let response = response_from(block_on(handle_aps_bid(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "application/json");

        // Parse response and check structure (real Amazon APS format)
        let body_bytes = response.into_body().into_bytes();
        let resp_json: serde_json::Value = serde_json::from_slice(&body_bytes).expect("valid json");

        // Check contextual wrapper
        assert!(resp_json.get("contextual").is_some());
        let contextual = resp_json.get("contextual").unwrap();

        // Check slots array
        let slots = contextual.get("slots").unwrap().as_array().unwrap();
        assert_eq!(slots.len(), 1);

        // Check slot details (should select 970x250 with highest CPM from [728x90, 970x250])
        let slot = &slots[0];
        assert_eq!(
            slot.get("slotID").unwrap().as_str().unwrap(),
            "header-banner"
        );
        assert_eq!(slot.get("size").unwrap().as_str().unwrap(), "970x250");
        assert!(slot.get("amznbid").is_some());
        assert!(slot.get("amzniid").is_some());
    }

    #[test]
    fn handle_aps_bid_empty_slots() {
        let body = serde_json::json!({
            "pubId": "5555",
            "slots": []
        });
        let ctx = ctx(
            Method::POST,
            "/e/dtb/bid",
            Body::json(&body).expect("json body"),
            &[],
        );
        let response = response_from(block_on(handle_aps_bid(ctx)));
        // Empty slots should fail validation
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn handle_aps_bid_invalid_json() {
        let ctx = ctx(Method::POST, "/e/dtb/bid", Body::from("not-json"), &[]);
        let response = response_from(block_on(handle_aps_bid(ctx)));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn handle_aps_win_valid() {
        let ctx = ctx(
            Method::GET,
            "/aps/win?slot=header-banner&price=2.50",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_aps_win(ctx)));
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn handle_aps_win_missing_slot() {
        let ctx = ctx(Method::GET, "/aps/win?price=2.50", Body::empty(), &[]);
        let response = response_from(block_on(handle_aps_win(ctx)));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn handle_aps_win_missing_price() {
        let ctx = ctx(
            Method::GET,
            "/aps/win?slot=header-banner",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_aps_win(ctx)));
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn handle_aps_win_negative_price() {
        let ctx = ctx(
            Method::GET,
            "/aps/win?slot=header-banner&price=-1.0",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_aps_win(ctx)));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn handle_sizes_returns_json() {
        let ctx = ctx(Method::GET, "/_/sizes", Body::empty(), &[]);
        let response = response_from(block_on(handle_sizes(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "application/json");
        let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let sizes = json["sizes"].as_array().unwrap();
        assert_eq!(sizes.len(), standard_sizes().count());
        // Check one size has all expected fields
        let first = &sizes[0];
        assert!(first["width"].is_i64());
        assert!(first["height"].is_i64());
        // CPM is no longer included — bid price is fixed at FIXED_BID_CPM
        assert!(first.get("cpm").is_none());
    }

    // -----------------------------------------------------------------------
    // Edge Cookie (EC) sync endpoint tests
    // -----------------------------------------------------------------------

    #[test]
    fn handle_sync_start_redirects_with_new_mtkid() {
        let ctx = ctx(
            Method::GET,
            "/sync/start?ts_domain=ts.publisher.com",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_sync_start(ctx)));
        assert_eq!(
            response.status(),
            StatusCode::FOUND,
            "should redirect to TS /sync"
        );
        let location = response
            .headers()
            .get(header::LOCATION)
            .expect("should have Location header")
            .to_str()
            .unwrap();
        assert!(
            location.starts_with("https://ts.publisher.com/sync?"),
            "should redirect to TS domain"
        );
        assert!(
            location.contains("partner=mocktioneer"),
            "should include partner=mocktioneer"
        );
        assert!(
            location.contains("uid="),
            "should include uid= with generated mtkid"
        );
        assert!(
            location.contains("return="),
            "should include return= callback URL"
        );
        // Should set mtkid cookie
        let cookies = response.headers().get_all("set-cookie");
        assert!(
            cookies
                .iter()
                .any(|c| c.to_str().unwrap_or_default().starts_with("mtkid=")),
            "should set mtkid cookie"
        );
    }

    #[test]
    fn handle_sync_start_reuses_existing_mtkid() {
        let mut builder = request_builder();
        builder = builder
            .method(Method::GET)
            .uri("/sync/start?ts_domain=ts.publisher.com")
            .header("Cookie", "mtkid=existing-id-123");
        let request = builder.body(Body::empty()).expect("request");
        let ctx = RequestContext::new(request, PathParams::default());
        let response = response_from(block_on(handle_sync_start(ctx)));
        assert_eq!(response.status(), StatusCode::FOUND);
        let location = response
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            location.contains("uid=existing-id-123"),
            "should use existing mtkid in redirect"
        );
        // Should NOT set a new cookie
        assert!(
            response.headers().get("set-cookie").is_none(),
            "should not reset existing cookie"
        );
    }

    #[test]
    fn handle_sync_start_missing_ts_domain() {
        let ctx = ctx(Method::GET, "/sync/start", Body::empty(), &[]);
        let response = response_from(block_on(handle_sync_start(ctx)));
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "should reject missing ts_domain"
        );
    }

    #[test]
    fn handle_sync_done_success() {
        let ctx = ctx(Method::GET, "/sync/done?ts_synced=1", Body::empty(), &[]);
        let response = response_from(block_on(handle_sync_done(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let ct = response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/gif", "should return a pixel");
    }

    #[test]
    fn handle_sync_done_failure() {
        let ctx = ctx(
            Method::GET,
            "/sync/done?ts_synced=0&ts_reason=no_consent",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_sync_done(ctx)));
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "should still return pixel even on sync failure"
        );
    }

    #[test]
    fn handle_resolve_returns_deterministic_uid() {
        // Ensure no auth token is set (tests may run concurrently)
        std::env::remove_var(PULL_TOKEN_ENV);

        let ec_id = format!("{}.AbC123", "a".repeat(64));
        let uri = format!("/resolve?ec_id={}&ip=203.0.113.1", ec_id);
        let rctx = ctx(Method::GET, &uri, Body::empty(), &[]);
        let response = response_from(block_on(handle_resolve(rctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(response.into_body().into_bytes().to_vec()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let uid = json["uid"].as_str().expect("should have uid").to_string();
        assert!(uid.starts_with("mtk-"), "uid should start with mtk-");
        assert_eq!(uid.len(), 16, "uid should be mtk- + 12 hex chars");

        // Same IP should produce the same UID (deterministic)
        let rctx2 = ctx(Method::GET, &uri, Body::empty(), &[]);
        let response2 = response_from(block_on(handle_resolve(rctx2)));
        let body2 = String::from_utf8(response2.into_body().into_bytes().to_vec()).unwrap();
        let json2: serde_json::Value = serde_json::from_str(&body2).unwrap();
        assert_eq!(
            json2["uid"].as_str().unwrap(),
            &uid,
            "should be deterministic"
        );
    }

    #[test]
    fn handle_resolve_different_ips_produce_different_uids() {
        // Ensure no auth token is set
        std::env::remove_var(PULL_TOKEN_ENV);

        let ec_id = format!("{}.XyZ789", "b".repeat(64));

        let uri1 = format!("/resolve?ec_id={}&ip=203.0.113.1", ec_id);
        let ctx1 = ctx(Method::GET, &uri1, Body::empty(), &[]);
        let resp1 = response_from(block_on(handle_resolve(ctx1)));
        let body1 = String::from_utf8(resp1.into_body().into_bytes().to_vec()).unwrap();
        let uid1 = serde_json::from_str::<serde_json::Value>(&body1).unwrap()["uid"]
            .as_str()
            .unwrap()
            .to_string();

        let uri2 = format!("/resolve?ec_id={}&ip=198.51.100.1", ec_id);
        let ctx2 = ctx(Method::GET, &uri2, Body::empty(), &[]);
        let resp2 = response_from(block_on(handle_resolve(ctx2)));
        let body2 = String::from_utf8(resp2.into_body().into_bytes().to_vec()).unwrap();
        let uid2 = serde_json::from_str::<serde_json::Value>(&body2).unwrap()["uid"]
            .as_str()
            .unwrap()
            .to_string();

        assert_ne!(uid1, uid2, "different IPs should produce different UIDs");
    }

    #[test]
    fn handle_resolve_rejects_invalid_ec_id() {
        // Ensure no auth token is set
        std::env::remove_var(PULL_TOKEN_ENV);

        let ctx = ctx(
            Method::GET,
            "/resolve?ec_id=tooshort&ip=1.2.3.4",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_resolve(ctx)));
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "should reject invalid ec_id format"
        );
    }

    /// Auth test is run with `--ignored` because it uses env vars that conflict
    /// with parallel test execution. Run: `cargo test -p mocktioneer-core -- --ignored`
    #[test]
    #[ignore = "uses env vars that race with parallel tests"]
    fn handle_resolve_rejects_when_auth_fails() {
        std::env::set_var(PULL_TOKEN_ENV, "correct-token");

        let ec_id = format!("{}.TsT456", "c".repeat(64));
        let uri = format!("/resolve?ec_id={}&ip=1.2.3.4", ec_id);

        // Request with wrong token
        let mut builder = request_builder();
        builder = builder
            .method(Method::GET)
            .uri(&uri)
            .header("Authorization", "Bearer wrong-token");
        let request = builder.body(Body::empty()).expect("request");
        let rctx = RequestContext::new(request, PathParams::default());
        let response = response_from(block_on(handle_resolve(rctx)));
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Request with correct token should succeed
        let mut builder2 = request_builder();
        builder2 = builder2
            .method(Method::GET)
            .uri(&uri)
            .header("Authorization", "Bearer correct-token");
        let request2 = builder2.body(Body::empty()).expect("request");
        let rctx2 = RequestContext::new(request2, PathParams::default());
        let response2 = response_from(block_on(handle_resolve(rctx2)));
        assert_eq!(response2.status(), StatusCode::OK);

        // Clean up env var
        std::env::remove_var(PULL_TOKEN_ENV);
    }

    #[test]
    fn urlencoding_encodes_special_chars() {
        assert_eq!(urlencoding("hello world"), "hello%20world");
        assert_eq!(urlencoding("a=b&c=d"), "a%3Db%26c%3Dd");
        assert_eq!(urlencoding("plain"), "plain");
        assert_eq!(
            urlencoding("https://example.com/path"),
            "https%3A%2F%2Fexample.com%2Fpath"
        );
    }

    #[test]
    fn hex_encode_produces_lowercase_hex() {
        assert_eq!(hex_encode(&[0x00, 0xff, 0xab]), "00ffab");
        assert_eq!(hex_encode(&[0xde, 0xad, 0xbe, 0xef]), "deadbeef");
    }

    #[test]
    fn is_valid_hostname_accepts_valid_domains() {
        assert!(is_valid_hostname("ts.publisher.com"));
        assert!(is_valid_hostname("localhost"));
        assert!(is_valid_hostname("my-server.example.org"));
    }

    #[test]
    fn is_valid_hostname_rejects_path_injection() {
        assert!(!is_valid_hostname("evil.com/path"));
        assert!(!is_valid_hostname("user@evil.com"));
        assert!(!is_valid_hostname("evil.com:8080"));
        assert!(!is_valid_hostname("evil.com?query"));
        assert!(!is_valid_hostname("evil.com#fragment"));
        assert!(!is_valid_hostname("evil.com foo"));
        assert!(!is_valid_hostname(""));
    }

    #[test]
    fn is_local_host_detects_local_addresses() {
        assert!(is_local_host("localhost"));
        assert!(is_local_host("localhost:8787"));
        assert!(is_local_host("127.0.0.1"));
        assert!(is_local_host("127.0.0.1:7676"));
        assert!(is_local_host("[::1]"));
        assert!(is_local_host("[::1]:8787"));
        assert!(is_local_host("foo.localhost"));
        assert!(!is_local_host("example.com"));
        assert!(!is_local_host("notlocalhost.com"));
    }

    #[test]
    fn sanitize_for_log_strips_control_chars() {
        assert_eq!(sanitize_for_log("normal text", 128), "normal text");
        assert_eq!(sanitize_for_log("has\nnewline", 128), "hasnewline");
        assert_eq!(sanitize_for_log("has\ttab", 128), "hastab");
        assert_eq!(sanitize_for_log("a\x00b\x1fc", 128), "abc");
    }

    #[test]
    fn sanitize_for_log_truncates() {
        assert_eq!(sanitize_for_log("abcdefgh", 4), "abcd");
    }

    #[test]
    fn constant_time_token_eq_works() {
        assert!(constant_time_token_eq("secret", "secret"));
        assert!(!constant_time_token_eq("secret", "wrong"));
        assert!(!constant_time_token_eq("short", "different-length"));
        assert!(!constant_time_token_eq("", "notempty"));
        assert!(constant_time_token_eq("", ""));
    }

    #[test]
    fn handle_sync_start_rejects_path_injection() {
        let ctx = ctx(
            Method::GET,
            "/sync/start?ts_domain=evil.com%2Fpath",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_sync_start(ctx)));
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "should reject ts_domain with path component"
        );
    }

    #[test]
    fn handle_sync_start_rejects_auth_injection() {
        let ctx = ctx(
            Method::GET,
            "/sync/start?ts_domain=user%40evil.com",
            Body::empty(),
            &[],
        );
        let response = response_from(block_on(handle_sync_start(ctx)));
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "should reject ts_domain with @ (basic auth syntax)"
        );
    }

    #[test]
    fn handle_resolve_rejects_non_hex_ec_id() {
        std::env::remove_var(PULL_TOKEN_ENV);

        // 64 chars but not hex, plus valid suffix
        let ec_id = format!("{}.AbC123", "z".repeat(64));
        let uri = format!("/resolve?ec_id={}&ip=1.2.3.4", ec_id);
        let ctx = ctx(Method::GET, &uri, Body::empty(), &[]);
        let response = response_from(block_on(handle_resolve(ctx)));
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "should reject non-hex ec_id"
        );
    }

    #[test]
    fn handle_pixel_produces_deterministic_mtkid() {
        let ctx1 = ctx(Method::GET, "/pixel?pid=test", Body::empty(), &[]);
        let response1 = response_from(block_on(handle_pixel(ctx1)));
        let cookie1 = response1
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let ctx2 = ctx(Method::GET, "/pixel?pid=test", Body::empty(), &[]);
        let response2 = response_from(block_on(handle_pixel(ctx2)));
        let cookie2 = response2
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        assert_eq!(cookie1, cookie2, "same host should produce same mtkid");
    }
}
