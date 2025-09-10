use handlebars::Handlebars;
use serde_json::Value as JsonValue;

pub fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn render_template_str(tmpl: &str, data: &JsonValue) -> String {
    let mut reg = Handlebars::new();
    // We want HTML escaping on by default (to protect attribute injection)
    reg.register_template_string("t", tmpl).ok();
    reg.render("t", data).unwrap_or_default()
}

pub fn banner_adm_iframe(base_host: &str, crid: &str, w: i64, h: i64, bid: Option<f64>) -> String {
    const IFRAME_TMPL: &str = include_str!("../static/templates/iframe.html");
    let bid_str = bid.map(|b| format!("{:.2}", b)).unwrap_or_default();
    let data = serde_json::json!({
        "HOST": base_host,
        "W": w,
        "H": h,
        "CRID": crid,
        "BID": bid_str,
    });
    render_template_str(IFRAME_TMPL, &data)
}

pub fn render_svg(w: i64, h: i64, bid: Option<f64>) -> String {
    const SVG_TMPL: &str = include_str!("../static/templates/image.svg");
    let pad = ((w.min(h) as f64) * 0.08).round() as i64;
    let mut text_len = w - 2 * pad;
    if text_len < 1 {
        text_len = 1;
    }
    let font = (h as f64 * 0.28).round() as i64;
    let mut cap_font = ((w.min(h) as f64) * 0.12).round() as i64;
    if cap_font < 10 {
        cap_font = 10;
    }
    let mut stroke = ((w.min(h) as f64) * 0.03).round() as i64;
    if stroke < 2 {
        stroke = 2;
    }
    let xbr = (w - pad - stroke).max(0);
    let ybr = (h - pad - stroke).max(0);
    let xtl = (pad + stroke).max(0);
    let ytl = (pad + stroke).max(0);
    // Compute numeric Y for caption: place it under the main title
    let title_y = h / 2; // main title uses 50% with baseline=middle
    let cap_y = title_y + ((font as f64 * 0.7).round() as i64);
    let bid_label = bid.map(|b| format!(" — ${:.2}", b)).unwrap_or_default();
    let data = serde_json::json!({
        "W": w,
        "H": h,
        "FONT": font,
        "TEXTLEN": text_len,
        "PADDING": pad,
        "CAPFONT": cap_font,
        "STROKE": stroke,
        "XBR": xbr,
        "YBR": ybr,
        "XTL": xtl,
        "YTL": ytl,
        "CAPY": cap_y,
        "BIDLBL": bid_label,
    });
    render_template_str(SVG_TMPL, &data)
}

pub fn creative_html(w: i64, h: i64) -> String {
    const HTML_TMPL: &str = include_str!("../static/templates/creative.html");
    let data = serde_json::json!({"W": w, "H": h});
    render_template_str(HTML_TMPL, &data)
}

pub fn info_html(host: &str) -> String {
    use std::env;
    const INFO_TMPL: &str = include_str!("../static/templates/info.html");
    let service_id = env::var("FASTLY_SERVICE_ID").unwrap_or_else(|_| "".to_string());
    let service_version = env::var("FASTLY_SERVICE_VERSION").unwrap_or_else(|_| "".to_string());
    let datacenter = env::var("FASTLY_DATACENTER")
        .or_else(|_| env::var("FASTLY_REGION"))
        .unwrap_or_else(|_| "".to_string());
    let pkg_version = env!("CARGO_PKG_VERSION");
    let data = serde_json::json!({
        "TITLE": "Mocktioneer Up",
        "HOST": host,
        "SERVICE_ID": service_id,
        "SERVICE_VERSION": service_version,
        "DATACENTER": datacenter,
        "PKG_VERSION": pkg_version,
    });
    render_template_str(INFO_TMPL, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_html_basic() {
        assert_eq!(escape_html("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn test_banner_adm_iframe_contains_expected_src_and_escapes() {
        let adm = banner_adm_iframe("host.test", "abc&def\"", 300, 250, None);
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
}
