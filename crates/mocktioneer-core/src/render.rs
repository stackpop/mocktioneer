use handlebars::Handlebars;
use serde::Serialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::openrtb::OpenRTBRequest;

/// Signature verification status for creative metadata
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", content = "details")]
pub enum SignatureStatus {
    /// Signature was present and successfully verified
    Verified { kid: String },
    /// Signature verification failed
    Failed { reason: String },
    /// No signature was present in the request
    NotPresent { reason: String },
}

impl SignatureStatus {
    /// Return an inline-styled HTML badge indicating the verification outcome.
    pub fn badge_html(&self) -> &'static str {
        match self {
            SignatureStatus::Verified { .. } => concat!(
                "<div style=\"position:absolute;bottom:0;right:0;font-size:9px;",
                "padding:1px 6px;background:rgba(0,128,0,.85);color:#fff;",
                "pointer-events:none;z-index:1;font-family:system-ui,sans-serif\">",
                "\u{2714}\u{FE0E} Request signature verified</div>",
            ),
            SignatureStatus::Failed { .. } => concat!(
                "<div style=\"position:absolute;bottom:0;right:0;font-size:9px;",
                "padding:1px 6px;background:rgba(200,0,0,.85);color:#fff;",
                "pointer-events:none;z-index:1;font-family:system-ui,sans-serif\">",
                "\u{274C} Request signature not verified</div>",
            ),
            SignatureStatus::NotPresent { .. } => concat!(
                "<div style=\"position:absolute;bottom:0;right:0;font-size:9px;",
                "padding:1px 6px;background:rgba(128,128,128,.75);color:#fff;",
                "pointer-events:none;z-index:1;font-family:system-ui,sans-serif\">",
                "\u{2014} No signature present</div>",
            ),
        }
    }
}

/// Metadata to embed in creative HTML comments
#[derive(Debug, Clone, Serialize)]
pub struct CreativeMetadata<'a> {
    pub signature: SignatureStatus,
    pub request: &'a OpenRTBRequest,
    /// The OpenRTB response with `adm` fields stripped (to avoid recursion)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<JsonValue>,
}
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

/// Render iframe HTML with embedded metadata as an HTML comment and a visible
/// verification badge.
///
/// The metadata is serialized as pretty-printed JSON and wrapped in an HTML comment.
/// Any `--` sequences in the JSON are escaped to prevent breaking the HTML comment
/// syntax. The iframe is wrapped in a positioned container with a small overlay badge
/// showing the signature verification status.
pub fn iframe_html_with_metadata(
    base_host: &str,
    crid: &str,
    w: i64,
    h: i64,
    bid: Option<f64>,
    metadata: &CreativeMetadata,
) -> String {
    let base_html = iframe_html(base_host, crid, w, h, bid);

    // Serialize metadata as pretty JSON
    let meta_json = serde_json::to_string_pretty(metadata)
        .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize metadata: {}\"}}", e));

    // Escape -- sequences to prevent breaking HTML comment syntax
    let safe_json = meta_json.replace("--", "- -");

    let badge = metadata.signature.badge_html();

    format!(
        "<!-- MOCKTIONEER_METADATA\n{}\n-->\n\
         <div style=\"position:relative;display:inline-block;width:{}px;height:{}px\">\
         {}\
         {}\
         </div>",
        safe_json, w, h, base_html, badge
    )
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

    #[test]
    fn test_iframe_html_with_metadata_includes_comment() {
        use crate::openrtb::OpenRTBRequest;

        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "test-req-123",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            signature: SignatureStatus::Verified {
                kid: "key-001".to_string(),
            },
            request: &req,
            response: None,
        };

        let adm =
            iframe_html_with_metadata("host.test", "crid123", 300, 250, Some(1.23), &metadata);

        // Check the comment structure
        assert!(adm.starts_with("<!-- MOCKTIONEER_METADATA"));
        assert!(adm.contains("-->\n<div"));

        // Check signature status is included in metadata comment
        assert!(adm.contains("\"status\": \"Verified\""));
        assert!(adm.contains("\"kid\": \"key-001\""));

        // Check request data is included
        assert!(adm.contains("\"id\": \"test-req-123\""));

        // Check the iframe is wrapped in a positioned container
        assert!(adm.contains("position:relative;display:inline-block;width:300px;height:250px"));
        assert!(adm.contains("//host.test/static/creatives/300x250.html"));
        assert!(adm.contains("</div>"));

        // Check the visible verification badge
        assert!(adm.contains("\u{2714}\u{FE0E} Request signature verified"));
        assert!(adm.contains("background:rgba(0,128,0,.85)"));
    }

    #[test]
    fn test_iframe_html_with_metadata_escapes_dashes() {
        use crate::openrtb::OpenRTBRequest;

        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "test--with--dashes",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            signature: SignatureStatus::Failed {
                reason: "Test--failure--reason".to_string(),
            },
            request: &req,
            response: None,
        };

        let adm = iframe_html_with_metadata("host.test", "crid123", 300, 250, None, &metadata);

        // The -- sequences should be escaped to "- -" to not break HTML comments
        // "test--with--dashes" becomes "test- -with- -dashes"
        assert!(adm.contains("test- -with- -dashes"));
        assert!(adm.contains("Test- -failure- -reason"));

        // The metadata section should not contain "--" (except for the comment delimiters)
        let metadata_content = adm
            .strip_prefix("<!-- MOCKTIONEER_METADATA\n")
            .unwrap()
            .split("\n-->")
            .next()
            .unwrap();
        assert!(
            !metadata_content.contains("--"),
            "Metadata should not contain -- sequence: {}",
            metadata_content
        );

        // Check the visible failure badge
        assert!(adm.contains("\u{274C} Request signature not verified"));
        assert!(adm.contains("background:rgba(200,0,0,.85)"));
    }

    #[test]
    fn test_iframe_html_with_metadata_signature_not_present() {
        use crate::openrtb::OpenRTBRequest;

        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "no-sig-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            signature: SignatureStatus::NotPresent {
                reason: "No site.domain present".to_string(),
            },
            request: &req,
            response: None,
        };

        let adm = iframe_html_with_metadata("host.test", "crid123", 300, 250, None, &metadata);

        assert!(adm.contains("\"status\": \"NotPresent\""));
        assert!(adm.contains("No site.domain present"));

        // Check the visible not-present badge
        assert!(adm.contains("\u{2014} No signature present"));
        assert!(adm.contains("background:rgba(128,128,128,.75)"));
    }

    #[test]
    fn test_iframe_html_with_metadata_includes_response() {
        use crate::openrtb::OpenRTBRequest;

        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "req-with-response",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let response = serde_json::json!({
            "id": "req-with-response",
            "cur": "USD",
            "seatbid": [{
                "seat": "mocktioneer",
                "bid": [{
                    "id": "bid-1",
                    "impid": "1",
                    "price": 1.23,
                    "crid": "mocktioneer-1",
                    "w": 300,
                    "h": 250
                }]
            }]
        });

        let metadata = CreativeMetadata {
            signature: SignatureStatus::Verified {
                kid: "key-001".to_string(),
            },
            request: &req,
            response: Some(response),
        };

        let adm = iframe_html_with_metadata("host.test", "crid123", 300, 250, None, &metadata);

        // Check response is included
        assert!(adm.contains("\"response\":"));
        assert!(adm.contains("\"seatbid\":"));
        assert!(adm.contains("\"seat\": \"mocktioneer\""));
        assert!(adm.contains("\"price\": 1.23"));
    }
}
