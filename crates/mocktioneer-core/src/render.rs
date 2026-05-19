use handlebars::Handlebars;
use serde::Serialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::openrtb::OpenRTBRequest;

const CREATIVE_HTML_TMPL: &str = include_str!("../static/templates/creative.html.hbs");
const IFRAME_HTML_TMPL: &str = include_str!("../static/templates/iframe.html.hbs");
const INFO_TMPL: &str = include_str!("../static/templates/info.html.hbs");
const SVG_TMPL: &str = include_str!("../static/templates/image.svg.hbs");

/// Signature verification status for creative metadata.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", content = "details")]
pub enum SignatureStatus {
    /// Signature verification failed.
    Failed { reason: String },
    /// No signature was present in the request.
    NotPresent { reason: String },
    /// Signature was present and successfully verified.
    Verified { kid: String },
}

/// Metadata to embed in creative HTML comments.
#[derive(Debug, Clone, Serialize)]
pub struct CreativeMetadata<'req> {
    pub request: &'req OpenRTBRequest,
    /// The `OpenRTB` response with `adm` fields stripped (to avoid recursion).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<JsonValue>,
    pub signature: SignatureStatus,
}

impl SignatureStatus {
    /// Return the URL parameter value for this signature status.
    /// Used to pass signature status to the creative template via query param.
    #[inline]
    #[must_use]
    pub fn url_param(&self) -> &'static str {
        match self {
            SignatureStatus::Verified { .. } => "verified",
            SignatureStatus::Failed { .. } => "failed",
            SignatureStatus::NotPresent { .. } => "not_present",
        }
    }
}

#[inline]
#[must_use]
pub fn creative_html(
    width: i64,
    height: i64,
    pixel_html: bool,
    pixel_js: bool,
    host: &str,
) -> String {
    let html_pid = Uuid::now_v7().as_simple().to_string();
    let js_pid = Uuid::now_v7().as_simple().to_string();
    let data = serde_json::json!({
        "H": height,
        "HOST": host,
        "PID_HTML": html_pid,
        "PID_JS": js_pid,
        "PIXEL_HTML": pixel_html,
        "PIXEL_JS": pixel_js,
        "W": width,
    });
    render_template_str(CREATIVE_HTML_TMPL, &data)
}

/// Render iframe HTML with embedded metadata as an HTML comment.
///
/// The metadata is serialized as pretty-printed JSON and wrapped in an HTML comment.
/// Any `--` sequences in the JSON are escaped to prevent breaking the HTML comment
/// syntax. The iframe is wrapped in a positioned container. The signature verification
/// badge is rendered inside the creative template (not in the wrapper).
#[inline]
#[must_use]
pub fn iframe_html(
    base_host: &str,
    crid: &str,
    width: i64,
    height: i64,
    bid: Option<f64>,
    metadata: &CreativeMetadata,
) -> String {
    let sig_param = metadata.signature.url_param();

    let meta_json = serde_json::to_string_pretty(metadata)
        .unwrap_or_else(|err| format!("{{\"error\": \"Failed to serialize metadata: {err}\"}}"));

    let safe_json = meta_json.replace("--", "- -");

    let bid_str = bid.map(|price| format!("{price:.2}")).unwrap_or_default();

    let data = serde_json::json!({
        "BID": bid_str,
        "CRID": crid,
        "H": height,
        "HOST": base_host,
        "METADATA_JSON": safe_json,
        "SIG": sig_param,
        "W": width,
    });
    render_template_str(IFRAME_HTML_TMPL, &data)
}

#[inline]
#[must_use]
pub fn info_html(host: &str) -> String {
    use std::env;
    let service_id = env::var("FASTLY_SERVICE_ID").unwrap_or_else(|_| String::new());
    let service_version = env::var("FASTLY_SERVICE_VERSION").unwrap_or_else(|_| String::new());
    let datacenter = env::var("FASTLY_DATACENTER")
        .or_else(|_| env::var("FASTLY_REGION"))
        .unwrap_or_else(|_| String::new());
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

#[inline]
#[must_use]
pub fn render_svg(width: i64, height: i64, bid: Option<f64>) -> String {
    let (font, cap_y, cap_font) = svg_layout(width, height);
    let bid_label = bid
        .map(|price| format!(" \u{2014} ${price:.2}"))
        .unwrap_or_default();
    let data = serde_json::json!({
        "BIDLBL": bid_label,
        "CAPFONT": cap_font,
        "CAPY": cap_y,
        "FONT": font,
        "H": height,
        "W": width,
    });
    render_template_str(SVG_TMPL, &data)
}

#[inline]
#[must_use]
pub fn render_template_str(tmpl: &str, data: &JsonValue) -> String {
    let mut reg = Handlebars::new();
    if reg.register_template_string("t", tmpl).is_err() {
        return String::new();
    }
    reg.render("t", data).unwrap_or_default()
}

/// Compute SVG font size, caption y-offset, and caption font size from creative
/// dimensions. Banner dimensions are small positive integers (always well within
/// `f64` precision) so the lossy `as` casts and integer division here are
/// intentional layout math.
#[expect(
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::float_arithmetic,
    clippy::integer_division,
    clippy::integer_division_remainder_used,
    clippy::arithmetic_side_effects,
    reason = "layout math on banner dimensions that always fit in f64 precision"
)]
fn svg_layout(width: i64, height: i64) -> (i64, i64, i64) {
    let font = (width as f64 / 5.0)
        .min(height as f64 / 2.0)
        .round()
        .max(12.0) as i64;
    let cap_y = height / 2 + (font as f64 * 0.7).round() as i64;
    let cap_font = ((width.min(height) as f64) * 0.06)
        .clamp(10.0, 16.0)
        .round() as i64;
    (font, cap_y, cap_font)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openrtb::OpenRTBRequest;

    fn metadata_fixture(signature: SignatureStatus) -> (OpenRTBRequest, CreativeMetadata<'static>) {
        // Use a leaked request to get a 'static lifetime for tests
        let req: &'static OpenRTBRequest = Box::leak(Box::new(
            serde_json::from_value(serde_json::json!({
                "id": "test-req",
                "imp": [{"id": "1", "banner": {"w": 300_i32, "h": 250_i32}}]
            }))
            .unwrap(),
        ));

        let metadata = CreativeMetadata {
            request: req,
            response: None,
            signature,
        };
        (req.clone(), metadata)
    }

    #[test]
    fn banner_adm_iframe_contains_expected_src_and_escapes() {
        let (_, metadata) = metadata_fixture(SignatureStatus::NotPresent {
            reason: "test".to_owned(),
        });
        let adm = iframe_html("host.test", "abc&def\"", 300, 250, None, &metadata);
        assert!(adm.contains("//host.test/static/creatives/300x250.html?crid=abc&amp;def&quot;"));
        assert!(adm.contains("width=\"300\""));
        assert!(adm.contains("height=\"250\""));
    }

    #[test]
    fn render_svg_includes_bid_label_when_present() {
        let svg = render_svg(300, 250, Some(2.5_f64));
        assert!(svg.contains("$2.50"));
        let svg2 = render_svg(300, 250, None);
        assert!(!svg2.contains('$'));
    }

    #[test]
    fn banner_adm_iframe_includes_bid_param_when_present() {
        let (_, metadata) = metadata_fixture(SignatureStatus::NotPresent {
            reason: "test".to_owned(),
        });
        let adm = iframe_html("host.test", "crid123", 320, 50, Some(3.75_f64), &metadata);
        assert!(adm.contains("//host.test/static/creatives/320x50.html"));
        assert!(adm.contains("bid=3.75"));
    }

    #[test]
    fn iframe_html_includes_metadata_comment() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "test-req-123",
            "imp": [{"id": "1", "banner": {"w": 300_i32, "h": 250_i32}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            request: &req,
            response: None,
            signature: SignatureStatus::Verified {
                kid: "key-001".to_owned(),
            },
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, Some(1.23_f64), &metadata);

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

        // Check the sig param is passed to iframe for badge rendering in creative
        assert!(adm.contains("&sig=verified"));
    }

    #[test]
    fn iframe_html_escapes_dashes() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "test--with--dashes",
            "imp": [{"id": "1", "banner": {"w": 300_i32, "h": 250_i32}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            request: &req,
            response: None,
            signature: SignatureStatus::Failed {
                reason: "Test--failure--reason".to_owned(),
            },
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, None, &metadata);

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
            "Metadata should not contain -- sequence: {metadata_content}"
        );

        // Check the sig param is passed to iframe for badge rendering in creative
        assert!(adm.contains("&sig=failed"));
    }

    #[test]
    fn iframe_html_signature_not_present() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "no-sig-req",
            "imp": [{"id": "1", "banner": {"w": 300_i32, "h": 250_i32}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            request: &req,
            response: None,
            signature: SignatureStatus::NotPresent {
                reason: "No site.domain present".to_owned(),
            },
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, None, &metadata);

        assert!(adm.contains("\"status\": \"NotPresent\""));
        assert!(adm.contains("No site.domain present"));

        // Check the sig param is passed to iframe for badge rendering in creative
        assert!(adm.contains("&sig=not_present"));
    }

    #[test]
    fn iframe_html_includes_response() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "req-with-response",
            "imp": [{"id": "1", "banner": {"w": 300_i32, "h": 250_i32}}]
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
                    "price": 1.23_f64,
                    "crid": "mocktioneer-1",
                    "w": 300_i32,
                    "h": 250_i32
                }]
            }]
        });

        let metadata = CreativeMetadata {
            request: &req,
            response: Some(response),
            signature: SignatureStatus::Verified {
                kid: "key-001".to_owned(),
            },
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, None, &metadata);

        // Check response is included
        assert!(adm.contains("\"response\":"));
        assert!(adm.contains("\"seatbid\":"));
        assert!(adm.contains("\"seat\": \"mocktioneer\""));
        assert!(adm.contains("\"price\": 1.23"));
    }
}
