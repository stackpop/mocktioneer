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
use serde::Deserialize;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use crate::aps::ApsBidRequest;
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

#[action]
pub async fn handle_pixel(
    Headers(headers): Headers,
    ValidatedQuery(params): ValidatedQuery<PixelQueryParams>,
) -> Response {
    let cookie_name = "mtkid";
    let mut set_cookie = None;

    let PixelQueryParams { pid: _ } = params;

    let existing = headers
        .get(header::COOKIE)
        .and_then(|c| c.to_str().ok())
        .and_then(|c| parse_cookie(c, cookie_name));

    if existing.is_none() {
        let id = Uuid::now_v7().as_simple().to_string();
        let max_age = 60 * 60 * 24 * 365;
        let cookie_val = format!(
            "{}={}; Path=/; Max-Age={}; SameSite=None; Secure; HttpOnly",
            cookie_name, id, max_age
        );
        set_cookie = Some(cookie_val);
    }

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
/// Useful for test fixtures and keeping external configs in sync with SIZE_MAP.
///
/// Response format:
/// ```json
/// {
///   "sizes": [
///     {"width": 300, "height": 250, "cpm": 2.5},
///     {"width": 728, "height": 90, "cpm": 3.0},
///     ...
///   ]
/// }
/// ```
#[action]
pub async fn handle_sizes() -> Response {
    use crate::auction::get_cpm;

    let sizes: Vec<serde_json::Value> = standard_sizes()
        .map(|(w, h)| {
            serde_json::json!({
                "width": w,
                "height": h,
                "cpm": get_cpm(w, h)
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
        assert!(first["cpm"].is_f64());
    }
}
