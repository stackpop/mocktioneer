#[cfg(target_arch = "wasm32")]
use fastly::{mime, Error, Request, Response};

#[cfg(target_arch = "wasm32")]
#[fastly::main]
pub fn main(req: Request) -> Result<Response, Error> {
    // CORS preflight support
    if req.get_method().as_str() == "OPTIONS" {
        return Ok(cors(Response::from_status(204)));
    }

    match (req.get_method().as_str(), req.get_path()) {
        ("GET", "/") => Ok(cors(Response::from_body("mocktioneer up"))),
        ("POST", "/openrtb2/auction") => handle_openrtb_auction(req),
        ("GET", "/click") => handle_click(req),
        ("GET", path) if path.starts_with("/static/") => handle_static(path),
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
fn handle_static(path: &str) -> Result<Response, Error> {
    if let Some((w, h)) = parse_size(path, "/static/img/", ".svg") {
        if !is_standard_size(w, h) {
            return Ok(cors(Response::from_status(404).with_body("Not Found")));
        }
        let svg = svg_image(w, h);
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
fn svg_image(w: i64, h: i64) -> String {
    const SVG_TMPL: &str = include_str!("../static/templates/image.svg");
    // Padding to keep text away from edges (~8% of min side)
    let pad = ((w.min(h) as f64) * 0.08).round() as i64;
    // Ensure text length fits horizontally within padding
    let mut text_len = w - 2 * pad;
    if text_len < 1 {
        text_len = 1;
    }
    // Main heading font: sized by height so it fits vertically
    let font = (h as f64 * 0.28).round() as i64;
    // Caption font for bottom-right size label
    let mut cap_font = ((w.min(h) as f64) * 0.16).round() as i64; // ~16% of min side
    if cap_font < 10 {
        cap_font = 10;
    }
    // Stroke width for caption outline (stronger min for readability)
    let mut stroke = ((w.min(h) as f64) * 0.03).round() as i64;
    if stroke < 2 {
        stroke = 2;
    }
    // Account for stroke so text outlines don't get clipped at edges
    let xbr = (w - pad - stroke).max(0);
    let ybr = (h - pad - stroke).max(0);
    let xtl = (pad + stroke).max(0);
    let ytl = (pad + stroke).max(0);
    SVG_TMPL
        .replace("{{W}}", &w.to_string())
        .replace("{{H}}", &h.to_string())
        .replace("{{FONT}}", &font.to_string())
        .replace("{{TEXTLEN}}", &text_len.to_string())
        .replace("{{PADDING}}", &pad.to_string())
        .replace("{{CAPFONT}}", &cap_font.to_string())
        .replace("{{STROKE}}", &stroke.to_string())
        .replace("{{XBR}}", &xbr.to_string())
        .replace("{{YBR}}", &ybr.to_string())
        .replace("{{XTL}}", &xtl.to_string())
        .replace("{{YTL}}", &ytl.to_string())
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
