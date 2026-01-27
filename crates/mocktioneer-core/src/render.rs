use handlebars::Handlebars;
use serde_json::Value as JsonValue;
use uuid::Uuid;

pub fn render_template_str(tmpl: &str, data: &JsonValue) -> String {
    let mut reg = Handlebars::new();
    // We want HTML escaping on by default (to protect attribute injection)
    reg.register_template_string("t", tmpl).ok();
    reg.render("t", data).unwrap_or_default()
}

const IFRAME_HTML_TMPL: &str = include_str!("../static/templates/iframe.html.hbs");
pub fn iframe_html(base_host: &str, crid: &str, w: i64, h: i64, bid: Option<f64>) -> String {
    let bid_str = bid.map(|b| format!("{:.2}", b)).unwrap_or_default();
    let data = serde_json::json!({
        "BID": bid_str,
        "CRID": crid,
        "H": h,
        "HOST": base_host,
        "W": w,
    });
    render_template_str(IFRAME_HTML_TMPL, &data)
}

pub fn render_svg(w: i64, h: i64, bid: Option<f64>) -> String {
    const SVG_TMPL: &str = include_str!("../static/templates/image.svg.hbs");
    // Font size: fit "WxH" text (~7 chars) within width, also limit by height
    let font = (w as f64 / 5.0).min(h as f64 / 2.0).round().max(12.0) as i64;
    // Caption positioned below main title
    let cap_y = h / 2 + (font as f64 * 0.7).round() as i64;
    let bid_label = bid.map(|b| format!(" — ${:.2}", b)).unwrap_or_default();
    let data = serde_json::json!({
        "BIDLBL": bid_label,
        "CAPFONT": ((w.min(h) as f64) * 0.06).clamp(10.0, 16.0).round() as i64,
        "CAPY": cap_y,
        "FONT": font,
        "H": h,
        "W": w,
    });
    render_template_str(SVG_TMPL, &data)
}

const CREATIVE_HTML_TMPL: &str = include_str!("../static/templates/creative.html.hbs");
pub fn creative_html(w: i64, h: i64, pixel_html: bool, pixel_js: bool, host: &str) -> String {
    let html_pid = Uuid::now_v7().as_simple().to_string();
    let js_pid = Uuid::now_v7().as_simple().to_string();
    let data = serde_json::json!({
        "H": h,
        "HOST": host,
        "PID_HTML": html_pid,
        "PID_JS": js_pid,
        "PIXEL_HTML": pixel_html,
        "PIXEL_JS": pixel_js,
        "W": w,
    });
    render_template_str(CREATIVE_HTML_TMPL, &data)
}

const INFO_TMPL: &str = include_str!("../static/templates/info.html.hbs");
pub fn info_html(host: &str) -> String {
    use std::env;
    let service_id = env::var("FASTLY_SERVICE_ID").unwrap_or_else(|_| "".to_string());
    let service_version = env::var("FASTLY_SERVICE_VERSION").unwrap_or_else(|_| "".to_string());
    let datacenter = env::var("FASTLY_DATACENTER")
        .or_else(|_| env::var("FASTLY_REGION"))
        .unwrap_or_else(|_| "".to_string());
    let pkg_version = env!("CARGO_PKG_VERSION");
    let data = serde_json::json!({
        "DATACENTER": datacenter,
        "HOST": host,
        "PKG_VERSION": pkg_version,
        "SERVICE_ID": service_id,
        "SERVICE_VERSION": service_version,
        "TITLE": "Mocktioneer Up",
    });
    render_template_str(INFO_TMPL, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_adm_iframe_contains_expected_src_and_escapes() {
        let adm = iframe_html("host.test", "abc&def\"", 300, 250, None);
        assert!(adm.contains("//host.test/static/creatives/300x250.html?crid=abc&amp;def&quot;"));
        assert!(adm.contains("width=\"300\""));
        assert!(adm.contains("height=\"250\""));
    }

    #[test]
    fn test_render_svg_includes_bid_label_when_present() {
        let svg = render_svg(300, 250, Some(2.5));
        assert!(svg.contains("$2.50"));
        let svg2 = render_svg(300, 250, None);
        assert!(!svg2.contains("$"));
    }

    #[test]
    fn test_banner_adm_iframe_includes_bid_param_when_present() {
        let adm = iframe_html("host.test", "crid123", 320, 50, Some(3.75));
        assert!(adm.contains("//host.test/static/creatives/320x50.html"));
        assert!(adm.contains("bid=3.75"));
    }
}
