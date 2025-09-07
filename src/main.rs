#[cfg(target_arch = "wasm32")]
use fastly::{mime, Error, Request, Response};
#[cfg(target_arch = "wasm32")]
use mocktioneer::openrtb::OpenRTBRequest;
#[cfg(target_arch = "wasm32")]
use mocktioneer::{build_openrtb_response_with_base_typed, escape_html, is_standard_size, render_svg};

#[cfg(target_arch = "wasm32")]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    // CORS preflight support
    if req.get_method().as_str() == "OPTIONS" {
        return Ok(cors(Response::from_status(204)));
    }

    let method = req.get_method().to_string();
    let path = req.get_path().to_string();
    match (method.as_str(), path.as_str()) {
        ("GET", "/") => Ok(cors(Response::from_body("mocktioneer up"))),
        ("POST", "/openrtb2/auction") => handle_openrtb_auction(req),
        ("GET", "/click") => handle_click(req),
        ("GET", _p) => handle_static(req),
        _ => Ok(cors(Response::from_status(404).with_body("Not Found"))),
    }
}

#[cfg(target_arch = "wasm32")]
fn cors(mut resp: Response) -> Response {
    resp.set_header("Access-Control-Allow-Origin", "*");
    resp.set_header("Access-Control-Allow-Methods", "GET, POST, OPTIONS");
    resp.set_header("Access-Control-Allow-Headers", "*, content-type");
    resp
}

#[cfg(target_arch = "wasm32")]
fn handle_openrtb_auction(mut req: Request) -> Result<Response, Error> {
    let body = req.take_body_bytes();
    let parsed: OpenRTBRequest = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            let err = serde_json::json!({"error":"invalid_json","message": e.to_string()});
            return Ok(cors(
                Response::from_status(400)
                    .with_body(err.to_string())
                    .with_content_type(mime::APPLICATION_JSON),
            ));
        }
    };

    let host: &str = req
        .get_header("Host")
        .and_then(|hv| hv.to_str().ok())
        .unwrap_or("mocktioneer.edgecompute.app");
    let resp = build_openrtb_response_with_base_typed(&parsed, host);
    let body = serde_json::to_string(&resp).unwrap_or_else(|_| "{}".to_string());
    Ok(cors(
        Response::from_body(body).with_content_type(mime::APPLICATION_JSON),
    ))
}

#[cfg(target_arch = "wasm32")]
fn handle_static(req: Request) -> Result<Response, Error> {
    let path = req.get_path();
    if let Some((w, h)) = parse_size(path, "/static/img/", ".svg") {
        if !is_standard_size(w, h) {
            return Ok(cors(Response::from_status(404).with_body("Not Found")));
        }
        let bid = req
            .get_query::<std::collections::HashMap<String, String>>()
            .ok()
            .and_then(|m| m.get("bid").cloned())
            .and_then(|s| s.parse::<f64>().ok());
        let svg = svg_image(w, h, bid);
        return Ok(cors(
            Response::from_body(svg).with_content_type(mime::IMAGE_SVG),
        ));
    }
    if let Some((w, h)) = parse_size(path, "/static/creatives/", ".html") {
        if !is_standard_size(w, h) {
            return Ok(cors(Response::from_status(404).with_body("Not Found")));
        }
        let html = creative_html(w, h);
        return Ok(cors(
            Response::from_body(html).with_content_type(mime::TEXT_HTML_UTF_8),
        ));
    }
    Ok(cors(Response::from_status(404).with_body("Not Found")))
}

#[cfg(target_arch = "wasm32")]
fn handle_click(req: Request) -> Result<Response, Error> {
    let params = req
        .get_query::<std::collections::HashMap<String, String>>()
        .unwrap_or_default();
    let crid = escape_html(params.get("crid").map(String::as_str).unwrap_or(""));
    let w = escape_html(params.get("w").map(String::as_str).unwrap_or(""));
    let h = escape_html(params.get("h").map(String::as_str).unwrap_or(""));
    const CLICK_TMPL: &str = include_str!("../static/templates/click.html");
    let html = CLICK_TMPL
        .replace("{{CRID}}", &crid)
        .replace("{{W}}", &w)
        .replace("{{H}}", &h);
    Ok(cors(
        Response::from_body(html).with_content_type(mime::TEXT_HTML_UTF_8),
    ))
}

#[cfg(target_arch = "wasm32")]
fn parse_size(path: &str, prefix: &str, suffix: &str) -> Option<(i64, i64)> {
    if !path.starts_with(prefix) || !path.ends_with(suffix) {
        return None;
    }
    let without_prefix = &path[prefix.len()..];
    let size_part = without_prefix.strip_suffix(suffix)?; // e.g. "300x250"
    let mut parts = size_part.split('x');
    let w = parts.next()?.parse::<i64>().ok()?;
    let h = parts.next()?.parse::<i64>().ok()?;
    Some((w, h))
}

#[cfg(target_arch = "wasm32")]
fn svg_image(w: i64, h: i64, bid: Option<f64>) -> String {
    render_svg(w, h, bid)
}

#[cfg(target_arch = "wasm32")]
fn creative_html(w: i64, h: i64) -> String {
    const HTML_TMPL: &str = include_str!("../static/templates/creative.html");
    HTML_TMPL
        .replace("{{W}}", &w.to_string())
        .replace("{{H}}", &h.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "mocktioneer is a Fastly Compute WASM app. Build for wasm32-wasi: `cargo build --target wasm32-wasi` or run locally with Fastly CLI."
    );
}
