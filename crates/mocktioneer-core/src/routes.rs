use std::marker::PhantomData;

use anyedge_core::FromRequest;
use anyedge_core::{
    action, header, App as EdgeApp, Body, EdgeError, HeaderValue, Headers, Hooks, Method,
    Middleware, Next, RequestContext, RequestLogger, Response, RouterService, StatusCode,
    ValidatedJson, ValidatedQuery,
};
use async_trait::async_trait;
use serde::Deserialize;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use crate::auction::{build_openrtb_response_with_base_typed, is_standard_size};
use crate::openrtb::OpenRTBRequest;
use crate::render::{creative_html, info_html, render_svg, render_template_str};

#[derive(Deserialize, Validate)]
struct StaticImgQuery {
    #[validate(range(min = 0.0))]
    bid: Option<f64>,
}

#[derive(Deserialize, Validate)]
struct StaticCreativeQuery {
    #[serde(default)]
    pixel: Option<bool>,
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
    let cleaned = size.split(|c| c == '?' || c == '&').next().unwrap_or(size);

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
    let mut builder = anyedge_core::response_builder().status(status);
    if let Body::Once(bytes) = &body {
        if !bytes.is_empty() {
            builder = builder.header(header::CONTENT_LENGTH, bytes.len().to_string());
        }
    }
    builder
        .body(body)
        .expect("static response builder should not fail")
}

pub struct Cors;

#[async_trait(?Send)]
impl Middleware for Cors {
    async fn handle(&self, ctx: RequestContext, next: Next<'_>) -> Result<Response, EdgeError> {
        let method = ctx.request().method().clone();
        let mut response = if method == Method::OPTIONS {
            let mut response = build_response(StatusCode::NO_CONTENT, Body::empty());
            response.headers_mut().insert(
                header::ALLOW,
                HeaderValue::from_static("GET, POST, OPTIONS"),
            );
            response
        } else {
            next.run(ctx).await?
        };

        let headers = response.headers_mut();
        headers.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
        headers.insert(
            "Access-Control-Allow-Methods",
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        headers.insert(
            "Access-Control-Allow-Headers",
            HeaderValue::from_static("*, content-type"),
        );
        Ok(response)
    }
}

#[action]
async fn handle_options() -> Response {
    let mut response = build_response(StatusCode::NO_CONTENT, Body::empty());
    response.headers_mut().insert(
        header::ALLOW,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response
}

#[action]
async fn handle_root(Headers(headers): Headers) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let html = info_html(host);
    let mut response = build_response(StatusCode::OK, Body::text(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

#[action]
async fn handle_openrtb_auction(
    Headers(headers): Headers,
    ValidatedJson(payload): ValidatedJson<OpenRTBRequest>,
) -> Response {
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("mocktioneer.edgecompute.app");
    log::info!("auction id={}, imps={}", payload.id, payload.imp.len());
    let resp = build_openrtb_response_with_base_typed(&payload, host);
    let body = Body::json(&resp).unwrap_or_else(|_| Body::text("{}"));
    let mut response = build_response(StatusCode::OK, body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

#[action]
async fn handle_static_img(
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
async fn handle_static_creatives(
    ValidatedSize(size, _): ValidatedSize<HtmlSize>,
    ValidatedQuery(query): ValidatedQuery<StaticCreativeQuery>,
    Headers(headers): Headers,
) -> Response {
    let SizeDimensions {
        width: w,
        height: h,
    } = size;
    let pixel = query.pixel.unwrap_or(true);
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("mocktioneer.edgecompute.app");
    let html = creative_html(w, h, pixel, host);
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
async fn handle_pixel(Headers(headers): Headers) -> Response {
    let cookie_name = "mtkid";
    let mut set_cookie = None;

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

    let body = Body::from(&PIXEL_GIF[..]);
    let mut response = anyedge_core::response_builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/gif")
        .header(
            header::CACHE_CONTROL,
            "no-store, no-cache, must-revalidate, max-age=0",
        )
        .header("Pragma", "no-cache")
        .header(header::CONTENT_LENGTH, PIXEL_GIF.len().to_string())
        .body(body)
        .expect("pixel response");

    if let Some(cookie) = set_cookie {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append("Set-Cookie", value);
        }
    }

    response
}

#[action]
async fn handle_click(ValidatedQuery(params): ValidatedQuery<ClickQueryParams>) -> Response {
    let ClickQueryParams { crid, w, h } = params;
    let crid = crid.unwrap_or_default();
    let w = w.map(|v| v.to_string()).unwrap_or_default();
    let h = h.map(|v| v.to_string()).unwrap_or_default();
    log::info!("click crid={}, size={}x{}", crid, w, h);
    const CLICK_TMPL: &str = include_str!("../static/templates/click.html");
    let html = render_template_str(
        CLICK_TMPL,
        &serde_json::json!({"CRID": crid, "W": w, "H": h}),
    );
    let mut response = build_response(StatusCode::OK, Body::from(html));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

pub struct MocktioneerApp;

impl Hooks for MocktioneerApp {
    fn routes() -> RouterService {
        RouterService::builder()
            .middleware(Cors)
            .middleware(RequestLogger)
            .get("/", handle_root)
            .route("/", Method::OPTIONS, handle_options)
            .post("/openrtb2/auction", handle_openrtb_auction)
            .route("/openrtb2/auction", Method::OPTIONS, handle_options)
            .get("/static/img/{size}", handle_static_img)
            .route("/static/img/{size}", Method::OPTIONS, handle_options)
            .get("/static/creatives/{size}", handle_static_creatives)
            .route("/static/creatives/{size}", Method::OPTIONS, handle_options)
            .get("/click", handle_click)
            .route("/click", Method::OPTIONS, handle_options)
            .get("/pixel", handle_pixel)
            .route("/pixel", Method::OPTIONS, handle_options)
            .build()
    }
}

pub fn build_app() -> EdgeApp {
    MocktioneerApp::build_app()
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyedge_core::{
        request_builder, Body, EdgeError, IntoResponse, Method, PathParams, RequestContext,
    };
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
        let ctx = ctx(Method::GET, "/pixel", Body::empty(), &[]);
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
    fn handle_pixel_does_not_reset_cookie_when_present() {
        let mut builder = request_builder();
        builder = builder
            .method(Method::GET)
            .uri("/pixel")
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
        assert!(String::from_utf8(body.to_vec())
            .unwrap()
            .contains("//mocktioneer.edgecompute.app/pixel"));
    }

    #[test]
    fn handle_static_creatives_html_ok_without_pixel() {
        let ctx = ctx(
            Method::GET,
            "/static/creatives/300x250.html?pixel=false",
            Body::empty(),
            &[("size", "300x250.html")],
        );
        let response = response_from(block_on(handle_static_creatives(ctx)));
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().into_bytes();
        assert!(!String::from_utf8(body.to_vec()).unwrap().contains("/pixel"));
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
    fn handle_static_creatives_rejects_invalid_pixel_toggle() {
        let ctx = ctx(
            Method::GET,
            "/static/creatives/300x250.html?pixel=maybe",
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
}
