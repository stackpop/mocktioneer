use anyedge_core::{header, App as EdgeApp, Request as ARequest, Response as AResponse};

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

    app
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_param_parses_suffix() {
        assert_eq!(parse_size_param("300x250.svg", ".svg"), Some((300, 250)));
        assert_eq!(parse_size_param("300x250.html", ".svg"), None);
        assert_eq!(parse_size_param("bad", ".svg"), None);
    }
}
