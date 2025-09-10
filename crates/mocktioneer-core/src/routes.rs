use anyedge_core::{header, App as EdgeApp, Request as ARequest, Response as AResponse};
use uuid::Uuid;

use crate::auction::{build_openrtb_response_with_base_typed, is_standard_size};
use crate::openrtb::OpenRTBRequest;
use crate::render::{creative_html, info_html, render_svg, render_template_str};

fn parse_size_param(size: &str, suffix: &str) -> Option<(i64, i64)> {
    if !size.ends_with(suffix) {
        return None;
    }
    let core = &size[..size.len().saturating_sub(suffix.len())];
    let mut it = core.split('x');
    let w = it.next()?.parse::<i64>().ok()?;
    let h = it.next()?.parse::<i64>().ok()?;
    Some((w, h))
}

pub struct Cors;
impl anyedge_core::Middleware for Cors {
    fn handle(&self, req: ARequest, next: anyedge_core::Next) -> AResponse {
        let res = next(req);
        res.with_header("Access-Control-Allow-Origin", "*")
            .with_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
            .with_header("Access-Control-Allow-Headers", "*, content-type")
    }
}

pub fn handle_root(req: ARequest) -> AResponse {
    let host = req.header("Host").unwrap_or("");
    let html = info_html(host);
    AResponse::ok()
        .with_header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .with_body(html)
}

pub fn handle_openrtb_auction(req: ARequest) -> AResponse {
    let parsed: Result<OpenRTBRequest, _> = serde_json::from_slice(&req.body);
    let host = req.header("Host").unwrap_or("mocktioneer.edgecompute.app");
    match parsed {
        Ok(v) => {
            log::info!("auction id={}, imps={}", v.id, v.imp.len());
            let resp = build_openrtb_response_with_base_typed(&v, host);
            let body = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
            AResponse::ok()
                .with_header(header::CONTENT_TYPE, "application/json")
                .with_body(body)
        }
        Err(e) => {
            log::error!("invalid JSON: {}", e);
            let err = serde_json::json!({"error":"invalid_json","message": e.to_string()});
            AResponse::new(400)
                .with_header(header::CONTENT_TYPE, "application/json")
                .with_body(err.to_string())
        }
    }
}

pub fn handle_static_img(req: ARequest) -> AResponse {
    let size = req.param("size").unwrap_or("");
    if let Some((w, h)) = parse_size_param(size, ".svg") {
        if !is_standard_size(w, h) {
            log::warn!("non-standard image size {}x{}", w, h);
            return AResponse::not_found();
        }
        let bid = req.query("bid").and_then(|s| s.parse::<f64>().ok());
        let svg = render_svg(w, h, bid);
        return AResponse::ok()
            .with_header(header::CONTENT_TYPE, "image/svg+xml")
            .with_body(svg);
    }
    AResponse::not_found()
}

pub fn handle_static_creatives(req: ARequest) -> AResponse {
    let size = req.param("size").unwrap_or("");
    if let Some((w, h)) = parse_size_param(size, ".html") {
        if !is_standard_size(w, h) {
            log::warn!("non-standard creative size {}x{}", w, h);
            return AResponse::not_found();
        }
        let html = creative_html(w, h);
        return AResponse::ok()
            .with_header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .with_body(html);
    }
    AResponse::not_found()
}

fn parse_cookie<'a>(cookie_header: &'a str, name: &str) -> Option<&'a str> {
    // Very small cookie parser; looks for `name=value` pairs split by ';'
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

// 1x1 transparent GIF content included from a static file
const PIXEL_GIF: &[u8] = include_bytes!("../static/templates/pixel.gif");

pub fn handle_pixel(req: ARequest) -> AResponse {
    let cookie_name = "mtkid";
    let mut set_cookie = None;

    // If cookie is absent, set a new one
    let existing = req
        .header("Cookie")
        .and_then(|c| parse_cookie(c, cookie_name));

    if existing.is_none() {
        let id = Uuid::now_v7().as_simple().to_string();
        // 1 year
        let max_age = 60 * 60 * 24 * 365;
        let cookie_val = format!(
            "{}={}; Path=/; Max-Age={}; SameSite=None; Secure; HttpOnly",
            cookie_name, id, max_age
        );
        set_cookie = Some(cookie_val);
    }

    let mut res = AResponse::ok()
        .with_header(header::CONTENT_TYPE, "image/gif")
        .with_header(
            header::CACHE_CONTROL,
            "no-store, no-cache, must-revalidate, max-age=0",
        )
        .with_header("Pragma", "no-cache")
        .with_body(PIXEL_GIF);

    if let Some(v) = set_cookie {
        res = res.append_header("Set-Cookie", v);
    }

    res
}

pub fn handle_click(req: ARequest) -> AResponse {
    let crid = req.query("crid").unwrap_or("");
    let w = req.query("w").unwrap_or("");
    let h = req.query("h").unwrap_or("");
    log::info!("click crid={}, size={}x{}", crid, w, h);
    const CLICK_TMPL: &str = include_str!("../static/templates/click.html");
    let html = render_template_str(
        CLICK_TMPL,
        &serde_json::json!({"CRID": crid, "W": w, "H": h}),
    );
    AResponse::ok()
        .with_header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .with_body(html)
}

pub fn build_app() -> EdgeApp {
    let mut app = EdgeApp::new();

    // CORS + basic logging middleware
    app.middleware(Cors);
    app.middleware(anyedge_core::middleware::Logger);

    // Root info
    app.get("/", handle_root);

    // OpenRTB auction
    app.post("/openrtb2/auction", handle_openrtb_auction);

    // Static image as SVG
    app.get("/static/img/:size", handle_static_img);

    // Static creative HTML
    app.get("/static/creatives/:size", handle_static_creatives);

    // Click landing
    app.get("/click", handle_click);

    // Tracking pixel (sets tracking cookie if absent)
    app.get("/pixel", handle_pixel);

    app
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyedge_core::Method;

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
        let req = ARequest::new(Method::GET, "/pixel");
        let res = handle_pixel(req);
        assert_eq!(res.status.as_u16(), 200);
        let ct = res
            .headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/gif");
        let cookies = res.headers_all("set-cookie");
        assert!(cookies.iter().any(|c| c.starts_with("mtkid=")));
    }

    #[test]
    fn handle_pixel_does_not_reset_cookie_when_present() {
        let mut req = ARequest::new(Method::GET, "/pixel");
        req.set_header("Cookie", "mtkid=abc");
        let res = handle_pixel(req);
        assert_eq!(res.status.as_u16(), 200);
        assert!(res.headers.get("set-cookie").is_none());
    }

    #[test]
    fn handle_openrtb_auction_invalid_json_400() {
        let req = ARequest::new(Method::POST, "/openrtb2/auction").with_body("not-json");
        let res = handle_openrtb_auction(req);
        assert_eq!(res.status.as_u16(), 400);
        let ct = res
            .headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "application/json");
    }

    #[test]
    fn handle_static_img_svg_ok_and_nonstandard_404() {
        let mut req = ARequest::new(Method::GET, "/static/img/300x250.svg");
        req.params.insert("size".into(), "300x250.svg".into());
        req.query_params.insert("bid".into(), "2.50".into());
        let res = handle_static_img(req);
        assert_eq!(res.status.as_u16(), 200);
        let ct = res
            .headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "image/svg+xml");

        let mut req2 = ARequest::new(Method::GET, "/static/img/333x222.svg");
        req2.params.insert("size".into(), "333x222.svg".into());
        let res2 = handle_static_img(req2);
        assert_eq!(res2.status.as_u16(), 404);
    }

    #[test]
    fn handle_static_creatives_html_ok() {
        let mut req = ARequest::new(Method::GET, "/static/creatives/300x250.html");
        req.params.insert("size".into(), "300x250.html".into());
        let res = handle_static_creatives(req);
        assert_eq!(res.status.as_u16(), 200);
        let ct = res
            .headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.starts_with("text/html"));
    }

    #[test]
    fn handle_click_echoes_params() {
        let mut req = ARequest::new(Method::GET, "/click");
        req.query_params.insert("crid".into(), "abc".into());
        req.query_params.insert("w".into(), "300".into());
        req.query_params.insert("h".into(), "250".into());
        let res = handle_click(req);
        assert_eq!(res.status.as_u16(), 200);
        let body = String::from_utf8(res.body).unwrap();
        assert!(body.contains("abc"));
        assert!(body.contains("300"));
        assert!(body.contains("250"));
    }

    #[test]
    fn handle_root_returns_html() {
        let req = ARequest::new(Method::GET, "/");
        let res = handle_root(req);
        assert_eq!(res.status.as_u16(), 200);
        let ct = res
            .headers
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.starts_with("text/html"));
    }
}
