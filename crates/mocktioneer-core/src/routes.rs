use std::collections::HashMap;
use std::env;
use std::marker::PhantomData;
use std::net::IpAddr;

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
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq as _;
use validator::{Validate, ValidationError};

use crate::aps::ApsBidRequest;
use crate::auction::{
    build_aps_response, build_openrtb_response, is_standard_size, standard_sizes, FIXED_BID_CPM,
};
use crate::mediation::{mediate_auction, MediationRequest};
use crate::openrtb::OpenRTBRequest;
use crate::render::{
    creative_html, extract_ec_hash, info_html, render_svg, render_template_str, SignatureStatus,
};
use crate::verification::verify_request_id_signature;

const CLICK_TMPL: &str = include_str!("../static/templates/click.html.hbs");
const MTKID_COOKIE_NAME: &str = "mtkid";
const MTKID_HASH_HEX_LEN: usize = 32_usize;
const MTKID_MAX_AGE: u64 = 60_u64 * 60_u64 * 24_u64 * 365_u64;
/// The partner ID that mocktioneer uses when registering with trusted-server.
const PARTNER_ID: &str = "mocktioneer";
const PERCENT_HEX_CHARS_UPPER: [u8; 16] = *b"0123456789ABCDEF";
const PIXEL_GIF: &[u8] = include_bytes!("../static/pixel.gif");
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

#[derive(Deserialize, Validate)]
struct ApsWinParams {
    #[validate(range(min = 0.0_f64))]
    price: f64,
    #[validate(length(min = 1_u64))]
    slot: String,
}

#[derive(Deserialize, Validate)]
struct StaticImgQuery {
    #[validate(range(min = 0.0_f64))]
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
    #[validate(length(min = 1_u64, max = 128_u64))]
    pid: String,
}

#[derive(Deserialize, Validate)]
struct ClickQueryParams {
    #[serde(default)]
    #[validate(length(max = 128_u64))]
    crid: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, String>,
    #[serde(default, rename = "h")]
    #[validate(range(min = 1_i64))]
    height: Option<i64>,
    #[serde(default, rename = "w")]
    #[validate(range(min = 1_i64))]
    width: Option<i64>,
}

#[derive(Deserialize, Validate)]
struct StaticAssetPath {
    #[validate(custom(function = "validate_static_asset_size"))]
    size: String,
}

enum AssetFormat {
    Html,
    Svg,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PullAuthOutcome {
    Authorized,
    Misconfigured,
    Unauthorized,
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
        log::warn!("non-standard image size {width}x{height}");
        EdgeError::validation(format!("non-standard image size {width}x{height}"))
    }
}

impl AssetFormatMarker for HtmlSize {
    const FORMAT: AssetFormat = AssetFormat::Html;

    fn handle_invalid(path: &str, width: i64, height: i64) -> EdgeError {
        log::warn!("non-standard creative size {width}x{height}");
        EdgeError::not_found(path.to_owned())
    }
}

#[derive(Clone, Copy)]
struct SizeDimensions {
    height: i64,
    width: i64,
}

#[derive(Deserialize, Validate)]
struct ResolveParams {
    /// Full EC identifier in `{64-hex}.{6-alnum}` format.
    #[validate(custom(function = "validate_ec_id"))]
    ec_id: String,
    /// Client IP address.
    #[validate(
        length(min = 1_u64, max = 45_u64),
        custom(function = "validate_ip_address")
    )]
    ip: String,
}

#[derive(Serialize)]
struct ResolveResponse {
    uid: String,
}

#[derive(Deserialize, Validate)]
struct SyncDoneParams {
    /// Failure reason — present only when `ts_synced=0`.
    #[serde(default)]
    #[validate(length(max = 256_u64))]
    ts_reason: Option<String>,
    /// Whether the sync succeeded ("1") or failed ("0").
    #[validate(custom(function = "validate_ts_synced"))]
    ts_synced: String,
}

#[derive(Deserialize, Validate)]
struct SyncStartParams {
    /// The trusted-server hostname (e.g., `ts.publisher.com`).
    #[validate(length(min = 1_u64, max = 253_u64))]
    ts_domain: String,
}

struct ValidatedSize<F>(SizeDimensions, PhantomData<F>);

pub struct Cors;

#[async_trait(?Send)]
impl<F> FromRequest for ValidatedSize<F>
where
    F: AssetFormatMarker + Send + Sync + 'static,
{
    async fn from_request(ctx: &RequestContext) -> Result<Self, EdgeError> {
        extract_size::<F>(ctx)
    }
}

#[async_trait(?Send)]
impl Middleware for Cors {
    #[inline]
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

fn extract_size<F>(ctx: &RequestContext) -> Result<ValidatedSize<F>, EdgeError>
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

        return Ok(ValidatedSize(SizeDimensions { height, width }, PhantomData));
    }

    Err(EdgeError::not_found(ctx.request().uri().path()))
}

fn parse_size_param(size: &str, suffix: &str) -> Option<(i64, i64)> {
    let cleaned = size.split(['?', '&']).next().unwrap_or(size);

    let core = cleaned.strip_suffix(suffix)?;
    let mut iter = core.split('x');
    let width = iter.next()?.parse::<i64>().ok()?;
    let height = iter.next()?.parse::<i64>().ok()?;
    Some((width, height))
}

fn validate_static_asset_size(value: &str) -> Result<(), ValidationError> {
    if parse_size_param(value, ".svg").is_some() || parse_size_param(value, ".html").is_some() {
        return Ok(());
    }

    let mut err = ValidationError::new("invalid_size");
    err.message = Some("expected format <width>x<height>.(svg|html)".into());
    Err(err)
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

fn build_response(status: StatusCode, body: Body) -> Response {
    let mut builder = response_builder().status(status);
    if let Body::Once(bytes) = &body {
        if !bytes.is_empty() {
            builder = builder.header(header::CONTENT_LENGTH, bytes.len().to_string());
        }
    }
    builder.body(body).unwrap_or_else(|_| {
        response_builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap_or_default()
    })
}

#[action]
pub async fn handle_options() -> Result<Response, EdgeError> {
    Ok(options_response())
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
pub async fn handle_root(ForwardedHost(host): ForwardedHost) -> Result<Response, EdgeError> {
    let html = info_html(&host);
    let mut response = build_response(StatusCode::OK, Body::text(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    Ok(response)
}

/// Interpret a config-store lookup for `bid_cpm`. Pure (no `ctx`) so it is
/// unit-testable. `None` (store absent / unseeded / key missing) → default;
/// a present-but-unparseable / non-finite / ≤0 value → error.
fn cpm_from_lookup(found: Option<String>) -> Result<f64, EdgeError> {
    match found {
        None => Ok(FIXED_BID_CPM),
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0_f64)
            .ok_or_else(|| {
                EdgeError::internal(anyhow::anyhow!(
                    "config store `mocktioneer_config` has malformed bid_cpm: {raw:?}"
                ))
            }),
    }
}

/// Resolve the effective bid CPM from the bound default config store, falling
/// back to `FIXED_BID_CPM` when no store is bound. A backend read error
/// propagates via `From<ConfigStoreError>`.
async fn resolve_bid_cpm(ctx: &RequestContext) -> Result<f64, EdgeError> {
    let Some(store) = ctx.config_store_default() else {
        return Ok(FIXED_BID_CPM);
    };
    cpm_from_lookup(store.get("bid_cpm").await.map_err(EdgeError::from)?)
}

#[action]
pub async fn handle_openrtb_auction(
    RequestContext(ctx): RequestContext,
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<OpenRTBRequest>,
) -> Result<Response, EdgeError> {
    // Capture signature verification status for metadata
    let signature_status =
        if let Some(domain) = req.site.as_ref().and_then(|site| site.domain.as_deref()) {
            match verify_request_id_signature(&ctx, &req.id, req.ext.as_ref(), domain).await {
                Ok(kid) => {
                    log::info!("\u{2705} Request signature verified with key: {kid}");
                    SignatureStatus::Verified { kid }
                }
                Err(err) => {
                    log::error!("\u{274c} Signature verification failed: {err}");
                    SignatureStatus::Failed {
                        reason: err.to_string(),
                    }
                }
            }
        } else {
            log::info!("\u{26a0}\u{fe0f} Signature verification skipped (no domain)");
            SignatureStatus::NotPresent {
                reason: "No site.domain present in request".to_owned(),
            }
        };

    log::info!("auction id={}, imps={}", req.id, req.imp.len());

    let cpm = resolve_bid_cpm(&ctx).await?;
    // Build response with embedded metadata (signature status + request + response preview)
    let resp = build_openrtb_response(&req, &host, signature_status, cpm);
    let body = Body::json(&resp).map_err(|err| {
        log::error!("Failed to serialize OpenRTB response: {err}");
        EdgeError::internal(err)
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
) -> Result<Response, EdgeError> {
    let SizeDimensions { width, height } = size;
    let svg = render_svg(width, height, query.bid);
    let mut response = build_response(StatusCode::OK, Body::from(svg));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml"),
    );
    Ok(response)
}

#[action]
pub async fn handle_static_creatives(
    ValidatedSize(size, _): ValidatedSize<HtmlSize>,
    ValidatedQuery(query): ValidatedQuery<StaticCreativeQuery>,
    ForwardedHost(host): ForwardedHost,
) -> Result<Response, EdgeError> {
    let SizeDimensions { width, height } = size;
    let pixel_html = query.pixel_html.unwrap_or(true);
    let pixel_js = query.pixel_js.unwrap_or(false);
    let html = creative_html(width, height, pixel_html, pixel_js, &host);
    let mut response = build_response(StatusCode::OK, Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    Ok(response)
}

fn parse_cookie<'cookie>(cookie_header: &'cookie str, name: &str) -> Option<&'cookie str> {
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some((key, value)) = trimmed.split_once('=') {
            if key.trim() == name {
                return Some(value.trim());
            }
        }
    }
    None
}

/// Read an existing `mtkid` cookie or generate a new one deterministically.
///
/// When no `mtkid` cookie is present, generates a deterministic host-scoped ID
/// using `SHA-256("mtkid:" || host)` truncated to 32 hex chars. This is
/// intentionally mock/test-oriented: first-time visitors on the same host get
/// the same generated value instead of a per-visitor identifier.
///
/// Returns `(mtkid_value, Option<set_cookie_header_value>)`.
fn get_or_create_mtkid(headers: &HeaderMap, host: &str) -> (String, Option<String>) {
    let existing = headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| parse_cookie(value, MTKID_COOKIE_NAME));

    if let Some(id) = existing {
        return (id.to_owned(), None);
    }

    // Deterministic and host-scoped: SHA-256("mtkid:" || host),
    // truncated to 32 hex chars. Same host always produces the same
    // generated mtkid; existing cookies are still reused as-is.
    let mut hasher = Sha256::new();
    hasher.update(b"mtkid:");
    hasher.update(host.as_bytes());
    let hash = hasher.finalize();
    let hex = hex_encode(&hash);
    let id: String = hex.chars().take(MTKID_HASH_HEX_LEN).collect();
    let cookie_val = format!(
        "{MTKID_COOKIE_NAME}={id}; Path=/; Max-Age={MTKID_MAX_AGE}; SameSite=None; Secure; HttpOnly",
    );
    (id, Some(cookie_val))
}

#[action]
pub async fn handle_pixel(
    Headers(headers): Headers,
    ForwardedHost(host): ForwardedHost,
    ValidatedQuery(params): ValidatedQuery<PixelQueryParams>,
) -> Result<Response, EdgeError> {
    // `pid` is validated during extraction (length 1..=128) but intentionally
    // unused: the pixel endpoint only echoes a tracking cookie, not the pid.
    let PixelQueryParams { .. } = params;

    let (_, set_cookie) = get_or_create_mtkid(&headers, &host);

    let mut response = build_response(StatusCode::OK, Body::from(PIXEL_GIF));
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/gif"));
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );
    response_headers.insert("Pragma", HeaderValue::from_static("no-cache"));
    if let Ok(length_value) = HeaderValue::from_str(&PIXEL_GIF.len().to_string()) {
        response_headers.insert(header::CONTENT_LENGTH, length_value);
    }

    if let Some(cookie) = set_cookie {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append("Set-Cookie", value);
        }
    }

    Ok(response)
}

#[action]
pub async fn handle_aps_bid(
    RequestContext(ctx): RequestContext,
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<ApsBidRequest>,
) -> Result<Response, EdgeError> {
    log::info!(
        "APS auction pubId={}, slots={}",
        req.pub_id,
        req.slots.len()
    );

    let cpm = resolve_bid_cpm(&ctx).await?;
    let resp = build_aps_response(&req, &host, cpm);
    let body = Body::json(&resp).map_err(|err| {
        log::error!("Failed to serialize APS response: {err}");
        EdgeError::internal(err)
    })?;
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

#[action]
pub async fn handle_aps_win(
    ValidatedQuery(params): ValidatedQuery<ApsWinParams>,
) -> Result<Response, EdgeError> {
    log::info!(
        "APS win notification slot={}, price={:.2}",
        params.slot,
        params.price
    );
    Ok(build_response(StatusCode::NO_CONTENT, Body::empty()))
}

#[action]
pub async fn handle_adserver_mediate(
    ForwardedHost(host): ForwardedHost,
    ValidatedJson(req): ValidatedJson<MediationRequest>,
) -> Result<Response, EdgeError> {
    log::info!(
        "Mediation request for auction '{}' with {} impressions and {} bidder responses",
        req.id,
        req.imp.len(),
        req.ext.bidder_responses.len()
    );

    let resp = mediate_auction(req, &host);

    log::info!(
        "Mediation complete for auction '{}': {} seatbid(s)",
        resp.id,
        resp.seatbid.len()
    );

    let body = Body::json(&resp).map_err(|err| {
        log::error!("Failed to serialize mediation response: {err}");
        EdgeError::internal(err)
    })?;
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

#[action]
pub async fn handle_click(
    ValidatedQuery(params): ValidatedQuery<ClickQueryParams>,
) -> Result<Response, EdgeError> {
    let ClickQueryParams {
        crid,
        extra,
        height,
        width,
    } = params;
    let crid_str = crid.unwrap_or_default();
    let width_str = width.map(|val| val.to_string()).unwrap_or_default();
    let height_str = height.map(|val| val.to_string()).unwrap_or_default();
    let mut extra_pairs: Vec<_> = extra.into_iter().collect();
    extra_pairs.sort_by(|left, right| left.0.cmp(&right.0));
    let extra_json: Vec<_> = extra_pairs
        .into_iter()
        .map(|(key, value)| serde_json::json!({ "KEY": key, "VALUE": value }))
        .collect();
    log::info!("click crid={crid_str}, size={width_str}x{height_str}");
    let html = render_template_str(
        CLICK_TMPL,
        &serde_json::json!({
            "CRID": crid_str,
            "W": width_str,
            "H": height_str,
            "EXTRA": extra_json,
        }),
    );
    let mut response = build_response(StatusCode::OK, Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    Ok(response)
}

/// Returns all standard ad sizes as JSON array.
/// Useful for test fixtures and keeping external configs in sync with `STANDARD_SIZES`.
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
pub async fn handle_sizes() -> Result<Response, EdgeError> {
    let sizes: Vec<serde_json::Value> = standard_sizes()
        .map(|(width, height)| {
            serde_json::json!({
                "width": width,
                "height": height
            })
        })
        .collect();

    let body = serde_json::json!({ "sizes": sizes });
    let mut response = build_response(StatusCode::OK, Body::from(body.to_string()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

// ---------------------------------------------------------------------------
// Edge Cookie (EC) sync endpoints
// ---------------------------------------------------------------------------

#[inline]
fn validation_error(code: &'static str, message: &'static str) -> ValidationError {
    let mut err = ValidationError::new(code);
    err.message = Some(message.into());
    err
}

/// Returns true if `value` is a clean redirect hostname.
///
/// This intentionally allows local/demo hostnames such as `localhost`, but rejects
/// IP literals and any path, auth, port, query, fragment, or whitespace syntax.
fn is_valid_hostname(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 253_usize
        || value.contains(['/', '@', ':', '?', '#', ' ', '\t', '\n', '\r'])
    {
        return false;
    }

    if value.parse::<IpAddr>().is_ok() {
        return false;
    }

    value.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63_usize
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

/// Validates that a string is a valid EC identifier in `{64-hex}.{6-alnum}` format.
fn validate_ec_id(value: &str) -> Result<(), ValidationError> {
    if extract_ec_hash(value).is_none() {
        return Err(validation_error(
            "invalid_ec_id",
            "ec_id must be in {64-hex}.{6-alnum} format",
        ));
    }
    Ok(())
}

/// Validates trusted-server sync status values.
fn validate_ts_synced(value: &str) -> Result<(), ValidationError> {
    if matches!(value, "0" | "1") {
        Ok(())
    } else {
        Err(validation_error(
            "invalid_ts_synced",
            "ts_synced must be either 0 or 1",
        ))
    }
}

/// Validates client IP address values accepted by `/resolve`.
fn validate_ip_address(value: &str) -> Result<(), ValidationError> {
    value
        .parse::<IpAddr>()
        .map(|_addr| ())
        .map_err(|_err| validation_error("invalid_ip", "ip must be a valid IPv4 or IPv6 address"))
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
) -> Result<Response, EdgeError> {
    // Reject ts_domain values that contain path/auth/port/fragment characters
    if !is_valid_hostname(&params.ts_domain) {
        log::warn!(
            "EC sync start rejected: ts_domain={} is not a valid hostname",
            sanitize_for_log(&params.ts_domain, 64)
        );
        return Ok(build_response(StatusCode::BAD_REQUEST, Body::empty()));
    }

    // Validate ts_domain against allowlist when configured
    let allowed_domains = env::var(TS_ALLOWED_DOMAINS_ENV).ok();
    if !is_ts_domain_allowed(&params.ts_domain, allowed_domains.as_deref()) {
        log::warn!(
            "EC sync start rejected: ts_domain={} not in {TS_ALLOWED_DOMAINS_ENV}",
            sanitize_for_log(&params.ts_domain, 64),
        );
        return Ok(build_response(StatusCode::FORBIDDEN, Body::empty()));
    }

    let (mtkid, set_cookie) = get_or_create_mtkid(&headers, &host);

    // Build the return URL (where TS redirects back after sync)
    let scheme = if is_local_host(&host) {
        "http"
    } else {
        "https"
    };
    let return_url = format!("{scheme}://{host}/sync/done");

    // Build the redirect to trusted-server's /sync endpoint
    let encoded_uid = urlencoding(&mtkid);
    let encoded_return = urlencoding(&return_url);
    let ts_domain = &params.ts_domain;
    let redirect_url = format!(
        "https://{ts_domain}/sync?partner={PARTNER_ID}&uid={encoded_uid}&return={encoded_return}"
    );

    log::info!(
        "EC sync start: mtkid={}, ts_domain={}",
        sanitize_for_log(&mtkid, 64),
        sanitize_for_log(&params.ts_domain, 64)
    );

    let Ok(loc) = HeaderValue::from_str(&redirect_url) else {
        log::error!(
            "EC sync start: invalid redirect URL for ts_domain={}",
            sanitize_for_log(&params.ts_domain, 64)
        );
        return Ok(build_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            Body::empty(),
        ));
    };

    let mut response = build_response(StatusCode::FOUND, Body::empty());
    let response_headers = response.headers_mut();
    response_headers.insert(header::LOCATION, loc);
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );

    if let Some(cookie) = set_cookie {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append("Set-Cookie", value);
        }
    }

    Ok(response)
}

/// `GET /sync/done?ts_synced=1` or `GET /sync/done?ts_synced=0&ts_reason=no_consent`
///
/// Callback from trusted-server after pixel sync completes. Returns a 1x1 pixel
/// so the browser redirect chain terminates cleanly.
#[action]
pub async fn handle_sync_done(
    ValidatedQuery(params): ValidatedQuery<SyncDoneParams>,
) -> Result<Response, EdgeError> {
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
    let response_headers = response.headers_mut();
    response_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/gif"));
    response_headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );
    if let Ok(length_value) = HeaderValue::from_str(&PIXEL_GIF.len().to_string()) {
        response_headers.insert(header::CONTENT_LENGTH, length_value);
    }
    Ok(response)
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
/// var is unset, auth is skipped. If the env var is set but empty, requests
/// fail closed with `401 Unauthorized`.
///
/// **WASM note:** `std::env::var` returns `Err` on Cloudflare Workers,
/// which means auth is silently disabled on that platform. See
/// `TS_ALLOWED_DOMAINS_ENV` for the same limitation.
#[action]
pub async fn handle_resolve(
    Headers(headers): Headers,
    ValidatedQuery(params): ValidatedQuery<ResolveParams>,
) -> Result<Response, EdgeError> {
    let expected_token = env::var(PULL_TOKEN_ENV).ok();
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());

    match authorize_pull_token(auth_header, expected_token.as_deref()) {
        PullAuthOutcome::Authorized => {}
        PullAuthOutcome::Unauthorized => {
            log::warn!(
                "Pull sync auth failed for ec_id_prefix={}...",
                ec_id_log_prefix(&params.ec_id)
            );
            return Ok(build_response(StatusCode::UNAUTHORIZED, Body::empty()));
        }
        PullAuthOutcome::Misconfigured => {
            log::error!("{PULL_TOKEN_ENV} is set but empty; rejecting pull sync request");
            return Ok(build_response(StatusCode::UNAUTHORIZED, Body::empty()));
        }
    }

    let uid = resolve_uid(&params.ec_id, &params.ip)?;

    log::info!(
        "Pull sync resolve: ec_id_prefix={}..., ip={}",
        ec_id_log_prefix(&params.ec_id),
        sanitize_for_log(&params.ip, 45)
    );

    let body = Body::json(&ResolveResponse { uid }).map_err(|err| {
        log::error!("Failed to serialize resolve response: {err}");
        EdgeError::internal(err)
    })?;
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}

fn authorize_pull_token(
    auth_header: Option<&str>,
    expected_token: Option<&str>,
) -> PullAuthOutcome {
    let Some(expected) = expected_token else {
        return PullAuthOutcome::Authorized;
    };

    if expected.trim().is_empty() {
        return PullAuthOutcome::Misconfigured;
    }

    let provided_token = auth_header
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");

    if constant_time_token_eq(provided_token, expected) {
        PullAuthOutcome::Authorized
    } else {
        PullAuthOutcome::Unauthorized
    }
}

fn resolve_uid(ec_id: &str, ip: &str) -> Result<String, EdgeError> {
    let ec_hash = extract_ec_hash(ec_id)
        .ok_or_else(|| EdgeError::validation("invalid ec_id format".to_owned()))?;
    Ok(resolve_uid_from_ec_hash(ec_hash, ip))
}

fn resolve_uid_from_ec_hash(ec_hash: &str, ip: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ec_hash.as_bytes());
    hasher.update(b"|");
    hasher.update(ip.as_bytes());
    let hash = hasher.finalize();
    let hex = hex_encode(&hash);
    let prefix: String = hex.chars().take(12_usize).collect();
    format!("mtk-{prefix}")
}

fn ec_id_log_prefix(ec_id: &str) -> String {
    extract_ec_hash(ec_id).map_or_else(
        || sanitize_for_log(ec_id, 8_usize),
        |ec_hash| ec_hash.chars().take(8_usize).collect(),
    )
}

fn is_ts_domain_allowed(ts_domain: &str, allowed_domains: Option<&str>) -> bool {
    allowed_domains.is_none_or(|allowed| {
        allowed
            .split(',')
            .any(|domain| domain.trim().eq_ignore_ascii_case(ts_domain))
    })
}

/// Minimal percent-encoding for URL query parameter values.
fn urlencoding(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => {
                out.push('%');
                out.push(char::from(
                    *PERCENT_HEX_CHARS_UPPER
                        .get(usize::from(byte >> 4_u8))
                        .unwrap_or(&b'0'),
                ));
                out.push(char::from(
                    *PERCENT_HEX_CHARS_UPPER
                        .get(usize::from(byte & 0x0f_u8))
                        .unwrap_or(&b'0'),
                ));
            }
        }
    }
    out
}

/// Encode bytes as lowercase hex string.
fn hex_encode(bytes: &[u8]) -> String {
    const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(bytes.len().saturating_mul(2_usize));
    for &byte in bytes {
        out.push(char::from(
            *HEX_LOWER.get(usize::from(byte >> 4_u8)).unwrap_or(&b'0'),
        ));
        out.push(char::from(
            *HEX_LOWER.get(usize::from(byte & 0x0f_u8)).unwrap_or(&b'0'),
        ));
    }
    out
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
        host.split(']')
            .next()
            .map_or(host, |segment| segment.get(1_usize..).unwrap_or(host))
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
fn sanitize_for_log(input: &str, max_len: usize) -> String {
    input
        .chars()
        .filter(|character| !character.is_control())
        .take(max_len)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use edgezero_core::body::Body;
    use edgezero_core::config_store::{ConfigStore, ConfigStoreError, ConfigStoreHandle};
    use edgezero_core::context::RequestContext;
    use edgezero_core::error::EdgeError;
    use edgezero_core::http::{request_builder, Method, Response, StatusCode};
    use edgezero_core::params::PathParams;
    use edgezero_core::response::IntoResponse as _;
    use edgezero_core::store_registry::{ConfigRegistry, StoreRegistry};
    use futures::executor::block_on;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Arc;

    /// In-memory `ConfigStore` for tests (mirrors app-demo's `MapConfigStore`).
    struct MapConfigStore(HashMap<String, String>);

    // `ConfigStore` is declared `#[async_trait(?Send)]` in edgezero-core, so
    // the impl MUST use the same `(?Send)` mode or method signatures won't match.
    #[async_trait(?Send)]
    impl ConfigStore for MapConfigStore {
        async fn get(&self, key: &str) -> Result<Option<String>, ConfigStoreError> {
            Ok(self.0.get(key).cloned())
        }
    }

    fn response_from(result: Result<Response, EdgeError>) -> Response {
        match result {
            Ok(response) => response,
            Err(err) => err.into_response().expect("error response"),
        }
    }

    fn body_bytes(response: Response) -> Vec<u8> {
        response
            .into_body()
            .into_bytes()
            .expect("buffered body")
            .to_vec()
    }

    fn ctx(method: Method, uri: &str, body: Body, params: &[(&str, &str)]) -> RequestContext {
        let mut builder = request_builder();
        builder = builder.method(method).uri(uri);
        let request = builder.body(body).expect("request");
        let map = params
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<HashMap<_, _>>();
        RequestContext::new(request, PathParams::new(map))
    }

    // ---- bid_cpm resolution ----

    #[test]
    fn cpm_from_lookup_falls_back_when_absent() {
        assert_eq!(
            cpm_from_lookup(None).unwrap().to_bits(),
            FIXED_BID_CPM.to_bits()
        );
    }

    #[test]
    fn cpm_from_lookup_parses_valid_value() {
        assert!((cpm_from_lookup(Some("0.35".to_owned())).unwrap() - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn cpm_from_lookup_rejects_bad_values() {
        for bad in ["-1", "0", "abc", "inf", "NaN", ""] {
            assert!(
                cpm_from_lookup(Some(bad.to_owned())).is_err(),
                "expected {bad:?} to error"
            );
        }
    }

    fn ctx_with_config(pairs: &[(&str, &str)]) -> RequestContext {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        let handle = ConfigStoreHandle::new(Arc::new(MapConfigStore(map)));
        let by_id: BTreeMap<String, ConfigStoreHandle> =
            [("mocktioneer_config".to_owned(), handle)]
                .into_iter()
                .collect();
        let registry: ConfigRegistry = StoreRegistry::new(by_id, "mocktioneer_config".to_owned());

        let mut request = request_builder()
            .method(Method::GET)
            .uri("/")
            .body(Body::empty())
            .expect("request");
        request.extensions_mut().insert(registry);
        RequestContext::new(request, PathParams::new(HashMap::new()))
    }

    #[test]
    fn resolve_bid_cpm_reads_seeded_store() {
        let ctx = ctx_with_config(&[("bid_cpm", "0.35")]);
        let cpm = block_on(resolve_bid_cpm(&ctx)).unwrap();
        assert!((cpm - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn resolve_bid_cpm_falls_back_without_registry() {
        let request = request_builder()
            .method(Method::GET)
            .uri("/")
            .body(Body::empty())
            .expect("request");
        let ctx = RequestContext::new(request, PathParams::new(HashMap::new()));
        let cpm = block_on(resolve_bid_cpm(&ctx)).unwrap();
        assert_eq!(cpm.to_bits(), FIXED_BID_CPM.to_bits());
    }

    #[test]
    fn resolve_bid_cpm_errors_on_malformed_value() {
        let ctx = ctx_with_config(&[("bid_cpm", "-1")]);
        block_on(resolve_bid_cpm(&ctx)).unwrap_err();
    }

    #[test]
    fn parse_size_param_parses_suffix() {
        assert_eq!(parse_size_param("300x250.svg", ".svg"), Some((300, 250)));
        assert_eq!(parse_size_param("300x250.html", ".svg"), None);
        assert_eq!(parse_size_param("bad", ".svg"), None);
    }

    #[test]
    fn parse_cookie_extracts_value() {
        let header = "a=1; mtkid=xyz; x=y";
        assert_eq!(parse_cookie(header, "mtkid"), Some("xyz"));
        assert_eq!(parse_cookie(header, "missing"), None);
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
            .any(|value| value.to_str().unwrap_or_default().starts_with("mtkid=")));
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
        let response_ok = response_from(block_on(handle_static_img(ctx_ok)));
        assert_eq!(response_ok.status(), StatusCode::OK);
        let ct = response_ok
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
        let response_nonstandard = response_from(block_on(handle_static_img(ctx_nonstandard)));
        assert_eq!(
            response_nonstandard.status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
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
        let body_str = String::from_utf8(body_bytes(response)).unwrap();
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
        let body = String::from_utf8(body_bytes(response)).unwrap();
        assert!(body.contains("data-static-pid=\""));
        assert!(body.contains("var jsPid = \""));
        let static_pid = body
            .split("data-static-pid=\"")
            .nth(1)
            .and_then(|tail| tail.split('\"').next())
            .expect("static pid");
        let js_pid = body
            .split("var jsPid = \"")
            .nth(1)
            .and_then(|tail| tail.split('\"').next())
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
        let body = String::from_utf8(body_bytes(response)).unwrap();
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
        let body = String::from_utf8(body_bytes(response)).unwrap();
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
        let body = String::from_utf8(body_bytes(response)).unwrap();
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
                    "sizes": [[728_i32, 90_i32], [970_i32, 250_i32]]
                }
            ],
            "pageUrl": "https://example.com/article",
            "ua": "Mozilla/5.0",
            "timeout": 800_i32
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
        let bytes = body_bytes(response);
        let resp_json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");

        // Check contextual wrapper
        assert!(resp_json.get("contextual").is_some());
        let contextual = resp_json.get("contextual").unwrap();

        // Check slots array
        let slots = contextual.get("slots").unwrap().as_array().unwrap();
        assert_eq!(slots.len(), 1);

        // Check slot details (should select 970x250 with largest area from [728x90, 970x250])
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
        let body = String::from_utf8(body_bytes(response)).unwrap();
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
                .any(|cookie| cookie.to_str().unwrap_or_default().starts_with("mtkid=")),
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
    fn handle_sync_done_rejects_invalid_status() {
        let ctx = ctx(Method::GET, "/sync/done?ts_synced=x", Body::empty(), &[]);
        let response = response_from(block_on(handle_sync_done(ctx)));
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "should reject ts_synced values other than 0 or 1"
        );
    }

    #[test]
    fn handle_sync_done_rejects_overlong_reason() {
        let uri = format!("/sync/done?ts_synced=0&ts_reason={}", "x".repeat(257));
        let ctx = ctx(Method::GET, &uri, Body::empty(), &[]);
        let response = response_from(block_on(handle_sync_done(ctx)));
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "should reject overlong ts_reason values"
        );
    }

    #[test]
    fn resolve_uid_returns_deterministic_uid() {
        let ec_id = format!("{}.AbC123", "a".repeat(64));
        let uid = resolve_uid(&ec_id, "203.0.113.1").expect("uid");
        assert!(uid.starts_with("mtk-"), "uid should start with mtk-");
        assert_eq!(uid.len(), 16, "uid should be mtk- + 12 hex chars");

        let uid2 = resolve_uid(&ec_id, "203.0.113.1").expect("uid");
        assert_eq!(uid2, uid, "should be deterministic");
    }

    #[test]
    fn resolve_uid_different_ips_produce_different_uids() {
        let ec_id = format!("{}.XyZ789", "b".repeat(64));
        let uid1 = resolve_uid(&ec_id, "203.0.113.1").expect("uid");
        let uid2 = resolve_uid(&ec_id, "198.51.100.1").expect("uid");

        assert_ne!(uid1, uid2, "different IPs should produce different UIDs");
    }

    #[test]
    fn handle_resolve_rejects_invalid_ec_id() {
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

    #[test]
    fn handle_resolve_rejects_invalid_ip() {
        let ec_id = format!("{}.AbC123", "a".repeat(64));
        let uri = format!("/resolve?ec_id={ec_id}&ip=not-an-ip");
        let ctx = ctx(Method::GET, &uri, Body::empty(), &[]);
        let response = response_from(block_on(handle_resolve(ctx)));
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "should reject invalid IP address"
        );
    }

    #[test]
    fn validate_ip_address_accepts_ipv4_and_ipv6() {
        validate_ip_address("203.0.113.1").unwrap();
        validate_ip_address("2001:db8::1").unwrap();
        assert!(validate_ip_address("not-an-ip").is_err());
    }

    #[test]
    fn authorize_pull_token_allows_unset_auth() {
        assert_eq!(
            authorize_pull_token(None, None),
            PullAuthOutcome::Authorized
        );
    }

    #[test]
    fn authorize_pull_token_accepts_correct_bearer_token() {
        assert_eq!(
            authorize_pull_token(Some("Bearer correct-token"), Some("correct-token")),
            PullAuthOutcome::Authorized
        );
    }

    #[test]
    fn authorize_pull_token_rejects_missing_or_wrong_bearer_token() {
        assert_eq!(
            authorize_pull_token(None, Some("correct-token")),
            PullAuthOutcome::Unauthorized
        );
        assert_eq!(
            authorize_pull_token(Some("Bearer wrong-token"), Some("correct-token")),
            PullAuthOutcome::Unauthorized
        );
        assert_eq!(
            authorize_pull_token(Some("Basic correct-token"), Some("correct-token")),
            PullAuthOutcome::Unauthorized
        );
    }

    #[test]
    fn authorize_pull_token_rejects_empty_configured_token() {
        assert_eq!(
            authorize_pull_token(Some("Bearer "), Some("")),
            PullAuthOutcome::Misconfigured
        );
        assert_eq!(
            authorize_pull_token(Some("Bearer anything"), Some("   ")),
            PullAuthOutcome::Misconfigured
        );
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
    fn is_valid_hostname_rejects_ip_literals() {
        assert!(!is_valid_hostname("127.0.0.1"));
        assert!(!is_valid_hostname("203.0.113.10"));
        assert!(!is_valid_hostname("::1"));
        assert!(!is_valid_hostname("2001:db8::1"));
    }

    #[test]
    fn sync_start_params_enforces_dns_length_limit() {
        let valid_domain = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(61)
        );
        assert_eq!(valid_domain.len(), 253);
        let valid_params = SyncStartParams {
            ts_domain: valid_domain,
        };
        valid_params.validate().unwrap();

        let invalid_domain = format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(62)
        );
        assert_eq!(invalid_domain.len(), 254);
        let invalid_params = SyncStartParams {
            ts_domain: invalid_domain,
        };
        assert!(invalid_params.validate().is_err());
    }

    #[test]
    fn is_ts_domain_allowed_handles_unset_and_matching_allowlists() {
        assert!(is_ts_domain_allowed("ts.publisher.com", None));
        assert!(is_ts_domain_allowed(
            "ts.publisher.com",
            Some("other.example, TS.PUBLISHER.COM ")
        ));
        assert!(!is_ts_domain_allowed(
            "evil.example.com",
            Some("ts.publisher.com,other.example")
        ));
        assert!(!is_ts_domain_allowed("ts.publisher.com", Some("")));
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
    fn is_valid_hostname_rejects_invalid_labels() {
        assert!(!is_valid_hostname("bad..example.com"));
        assert!(!is_valid_hostname("-bad.example.com"));
        assert!(!is_valid_hostname("bad-.example.com"));
        assert!(!is_valid_hostname("bad_label.example.com"));
        assert!(!is_valid_hostname(&format!(
            "{}.example.com",
            "a".repeat(64)
        )));
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
        // 64 chars but not hex, plus valid suffix
        let ec_id = format!("{}.AbC123", "z".repeat(64));
        let uri = format!("/resolve?ec_id={ec_id}&ip=1.2.3.4");
        let ctx = ctx(Method::GET, &uri, Body::empty(), &[]);
        let response = response_from(block_on(handle_resolve(ctx)));
        assert!(
            response.status() == StatusCode::BAD_REQUEST
                || response.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "should reject non-hex ec_id"
        );
    }

    #[test]
    fn handle_pixel_produces_host_scoped_deterministic_mtkid() {
        let ctx1 = ctx(Method::GET, "/pixel?pid=test", Body::empty(), &[]);
        let response1 = response_from(block_on(handle_pixel(ctx1)));
        let cookie1 = response1
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();

        let ctx2 = ctx(Method::GET, "/pixel?pid=test", Body::empty(), &[]);
        let response2 = response_from(block_on(handle_pixel(ctx2)));
        let cookie2 = response2
            .headers()
            .get("set-cookie")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();

        assert_eq!(
            cookie1, cookie2,
            "same host should produce the same mock/test mtkid"
        );
    }
}
