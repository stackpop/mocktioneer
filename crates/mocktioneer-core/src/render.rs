use handlebars::Handlebars;
use serde::Serialize;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::openrtb::{Eid, OpenRTBRequest};

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
    /// Return the URL parameter value for this signature status.
    /// Used to pass signature status to the creative template via query param.
    pub fn url_param(&self) -> &'static str {
        match self {
            SignatureStatus::Verified { .. } => "verified",
            SignatureStatus::Failed { .. } => "failed",
            SignatureStatus::NotPresent { .. } => "not_present",
        }
    }
}

/// Edge Cookie identity information extracted from an OpenRTB bid request.
///
/// Populated from `user.id` (the EC value), `user.eids` (synced partner IDs),
/// `user.consent` (TCF string), and `user.buyeruid`. When trusted-server
/// decorates bid requests with EC data (§12 of the EC spec), this struct
/// captures that identity pipeline state for embedding in creative metadata.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeCookieInfo {
    /// The full EC identifier from `user.id` (format: `{64-hex}.{6-alnum}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ec_id: Option<String>,
    /// The buyer UID from `user.buyeruid` or matched from `user.eids`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyer_uid: Option<String>,
    /// TCF consent string from `user.consent`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent: Option<String>,
    /// Number of EID sources in the bid request.
    pub eids_count: usize,
    /// Full EIDs array for inspection.
    pub eids: Vec<Eid>,
    /// Whether mocktioneer's own UID appeared in `user.eids`.
    pub mocktioneer_matched: bool,
}

/// Extract the stable 64-char hex prefix from a full EC value.
///
/// Returns `None` if the value is not in `{64-hex}.{6-alnum}` format.
pub fn extract_ec_hash(ec_value: &str) -> Option<&str> {
    let (prefix, suffix) = ec_value.split_once('.')?;
    if prefix.len() != 64
        || !prefix.chars().all(|c| c.is_ascii_hexdigit())
        || suffix.len() != 6
        || !suffix.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(prefix)
}

const MOCKTIONEER_SOURCE_DOMAIN: &str = "mocktioneer.dev";

/// Build `EdgeCookieInfo` from an OpenRTB request's user object.
///
/// Checks both `user.eids` (OpenRTB 2.6 top-level) and `user.ext.eids`
/// (Prebid Server / OpenRTB 2.5 convention). The top-level field takes
/// priority; `ext.eids` is used as a fallback when the top-level is empty.
pub fn extract_ec_info(req: &OpenRTBRequest) -> EdgeCookieInfo {
    let user = req.user.as_ref();

    let ec_id = user.and_then(|u| u.id.clone());

    // Try top-level user.eids (OpenRTB 2.6), fall back to user.ext.eids (Prebid/2.5)
    let eids = user
        .map(|u| {
            if !u.eids.is_empty() {
                u.eids.clone()
            } else {
                u.ext
                    .as_ref()
                    .and_then(|ext| ext.get("eids"))
                    .and_then(|v| serde_json::from_value::<Vec<Eid>>(v.clone()).ok())
                    .unwrap_or_default()
            }
        })
        .unwrap_or_default();

    let mocktioneer_eid_uid = eids.iter().find_map(|eid| {
        if eid.source == MOCKTIONEER_SOURCE_DOMAIN {
            eid.uids.first().map(|u| u.id.clone())
        } else {
            None
        }
    });

    // Prefer buyeruid, fall back to matched EID
    let buyer_uid = user
        .and_then(|u| u.buyeruid.clone())
        .or(mocktioneer_eid_uid.clone());

    let mocktioneer_matched = mocktioneer_eid_uid.is_some();

    EdgeCookieInfo {
        ec_id,
        buyer_uid,
        consent: user.and_then(|u| u.consent.clone()),
        eids_count: eids.len(),
        eids,
        mocktioneer_matched,
    }
}

/// Metadata to embed in creative HTML comments
#[derive(Debug, Clone, Serialize)]
pub struct CreativeMetadata<'a> {
    pub signature: SignatureStatus,
    pub edge_cookie: EdgeCookieInfo,
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

/// Render iframe HTML with embedded metadata as an HTML comment.
///
/// The metadata is serialized as pretty-printed JSON and wrapped in an HTML comment.
/// Any `--` sequences in the JSON are escaped to prevent breaking the HTML comment
/// syntax. The iframe is wrapped in a positioned container. The signature verification
/// badge is rendered inside the creative template (not in the wrapper).
pub fn iframe_html(
    base_host: &str,
    crid: &str,
    w: i64,
    h: i64,
    bid: Option<f64>,
    metadata: &CreativeMetadata,
) -> String {
    // Get signature status URL param for the creative to render the badge
    let sig_param = metadata.signature.url_param();

    // Serialize metadata as pretty JSON
    let meta_json = serde_json::to_string_pretty(metadata)
        .unwrap_or_else(|e| format!("{{\"error\": \"Failed to serialize metadata: {}\"}}", e));

    // Escape -- sequences to prevent breaking HTML comment syntax
    let safe_json = meta_json.replace("--", "- -");

    let bid_str = bid.map(|b| format!("{:.2}", b)).unwrap_or_default();

    let data = serde_json::json!({
        "BID": bid_str,
        "CRID": crid,
        "H": h,
        "HOST": base_host,
        "METADATA_JSON": safe_json,
        "SIG": sig_param,
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
    use crate::openrtb::OpenRTBRequest;

    fn test_metadata(signature: SignatureStatus) -> (OpenRTBRequest, CreativeMetadata<'static>) {
        // Use a leaked request to get a 'static lifetime for tests
        let req: &'static OpenRTBRequest = Box::leak(Box::new(
            serde_json::from_value(serde_json::json!({
                "id": "test-req",
                "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
            }))
            .unwrap(),
        ));

        let metadata = CreativeMetadata {
            signature,
            edge_cookie: extract_ec_info(req),
            request: req,
            response: None,
        };
        (req.clone(), metadata)
    }

    #[test]
    fn test_banner_adm_iframe_contains_expected_src_and_escapes() {
        let (_, metadata) = test_metadata(SignatureStatus::NotPresent {
            reason: "test".to_string(),
        });
        let adm = iframe_html("host.test", "abc&def\"", 300, 250, None, &metadata);
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
        let (_, metadata) = test_metadata(SignatureStatus::NotPresent {
            reason: "test".to_string(),
        });
        let adm = iframe_html("host.test", "crid123", 320, 50, Some(3.75), &metadata);
        assert!(adm.contains("//host.test/static/creatives/320x50.html"));
        assert!(adm.contains("bid=3.75"));
    }

    #[test]
    fn test_iframe_html_includes_metadata_comment() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "test-req-123",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            signature: SignatureStatus::Verified {
                kid: "key-001".to_string(),
            },
            edge_cookie: extract_ec_info(&req),
            request: &req,
            response: None,
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, Some(1.23), &metadata);

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
    fn test_iframe_html_escapes_dashes() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "test--with--dashes",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            signature: SignatureStatus::Failed {
                reason: "Test--failure--reason".to_string(),
            },
            edge_cookie: extract_ec_info(&req),
            request: &req,
            response: None,
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
            "Metadata should not contain -- sequence: {}",
            metadata_content
        );

        // Check the sig param is passed to iframe for badge rendering in creative
        assert!(adm.contains("&sig=failed"));
    }

    #[test]
    fn test_iframe_html_signature_not_present() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "no-sig-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            signature: SignatureStatus::NotPresent {
                reason: "No site.domain present".to_string(),
            },
            edge_cookie: extract_ec_info(&req),
            request: &req,
            response: None,
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, None, &metadata);

        assert!(adm.contains("\"status\": \"NotPresent\""));
        assert!(adm.contains("No site.domain present"));

        // Check the sig param is passed to iframe for badge rendering in creative
        assert!(adm.contains("&sig=not_present"));
    }

    #[test]
    fn test_iframe_html_includes_response() {
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
            edge_cookie: extract_ec_info(&req),
            request: &req,
            response: Some(response),
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, None, &metadata);

        // Check response is included
        assert!(adm.contains("\"response\":"));
        assert!(adm.contains("\"seatbid\":"));
        assert!(adm.contains("\"seat\": \"mocktioneer\""));
        assert!(adm.contains("\"price\": 1.23"));
    }

    #[test]
    fn test_creative_html_always_shows_debug_badge() {
        let html = creative_html(728, 90, true, false, "host.test");

        assert!(html.contains("var sig = validSig[sigParam] ? sigParam : \"not_present\";"));
        assert!(html.contains("badge.style.display = \"block\";"));
        assert!(html.contains("No signature present"));
    }

    #[test]
    fn test_extract_ec_hash_valid() {
        let ec = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123";
        assert_eq!(
            extract_ec_hash(ec),
            Some("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2")
        );
    }

    #[test]
    fn test_extract_ec_hash_invalid_formats() {
        assert_eq!(extract_ec_hash("too-short.abc123"), None);
        assert_eq!(extract_ec_hash("not-hex-at-all"), None);
        assert_eq!(
            extract_ec_hash("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.ab"),
            None
        ); // suffix too short
        assert_eq!(
            extract_ec_hash("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"),
            None
        ); // no dot
    }

    #[test]
    fn test_extract_ec_info_with_ec_user() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "ec-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}],
            "user": {
                "id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123",
                "buyeruid": "mtk-abc123",
                "consent": "CPtest123",
                "eids": [
                    {
                        "source": "mocktioneer.dev",
                        "uids": [{"id": "mtk-abc123", "atype": 3}]
                    },
                    {
                        "source": "liveramp.com",
                        "uids": [{"id": "LR_xyz", "atype": 3}]
                    }
                ]
            }
        }))
        .unwrap();

        let info = extract_ec_info(&req);
        assert_eq!(
            info.ec_id.as_deref(),
            Some("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123")
        );
        assert_eq!(info.buyer_uid.as_deref(), Some("mtk-abc123"));
        assert_eq!(info.consent.as_deref(), Some("CPtest123"));
        assert_eq!(info.eids_count, 2);
        assert!(
            info.mocktioneer_matched,
            "should match mocktioneer.dev source"
        );
    }

    #[test]
    fn test_extract_ec_info_no_user() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "no-user-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}]
        }))
        .unwrap();

        let info = extract_ec_info(&req);
        assert!(info.ec_id.is_none());
        assert!(info.buyer_uid.is_none());
        assert!(info.consent.is_none());
        assert_eq!(info.eids_count, 0);
        assert!(!info.mocktioneer_matched);
    }

    #[test]
    fn test_extract_ec_info_eids_without_mocktioneer() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "other-eids-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}],
            "user": {
                "eids": [
                    {
                        "source": "liveramp.com",
                        "uids": [{"id": "LR_xyz", "atype": 3}]
                    }
                ]
            }
        }))
        .unwrap();

        let info = extract_ec_info(&req);
        assert_eq!(info.eids_count, 1);
        assert!(!info.mocktioneer_matched);
        assert!(info.buyer_uid.is_none());
    }

    #[test]
    fn test_iframe_html_includes_ec_metadata() {
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "ec-metadata-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}],
            "user": {
                "id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123",
                "eids": [
                    {
                        "source": "mocktioneer.dev",
                        "uids": [{"id": "mtk-abc123", "atype": 3}]
                    }
                ]
            }
        }))
        .unwrap();

        let metadata = CreativeMetadata {
            signature: SignatureStatus::NotPresent {
                reason: "test".to_string(),
            },
            edge_cookie: extract_ec_info(&req),
            request: &req,
            response: None,
        };

        let adm = iframe_html("host.test", "crid123", 300, 250, None, &metadata);
        assert!(
            adm.contains("\"edge_cookie\":"),
            "should contain edge_cookie section"
        );
        assert!(adm.contains("\"mocktioneer_matched\": true"));
        assert!(adm.contains("\"eids_count\": 1"));
    }

    #[test]
    fn test_extract_ec_info_from_ext_eids_prebid_style() {
        // Prebid Server puts eids under user.ext.eids (OpenRTB 2.5 convention)
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "prebid-eids-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}],
            "user": {
                "id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123",
                "ext": {
                    "eids": [
                        {
                            "source": "mocktioneer.dev",
                            "uids": [{"id": "mtk-476b99ce5ff5", "atype": 3}]
                        },
                        {
                            "source": "liveramp.com",
                            "uids": [{"id": "LR_xyz", "atype": 3}]
                        }
                    ]
                }
            }
        }))
        .unwrap();

        let info = extract_ec_info(&req);
        assert_eq!(info.eids_count, 2, "should find eids from user.ext.eids");
        assert!(
            info.mocktioneer_matched,
            "should match mocktioneer.dev in ext.eids"
        );
        assert_eq!(
            info.buyer_uid.as_deref(),
            Some("mtk-476b99ce5ff5"),
            "should extract buyer_uid from ext.eids"
        );
        assert_eq!(
            info.ec_id.as_deref(),
            Some("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2.AbC123")
        );
    }

    #[test]
    fn test_extract_ec_info_top_level_eids_takes_priority_over_ext() {
        // When both user.eids and user.ext.eids are present, top-level wins
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "both-eids-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}],
            "user": {
                "eids": [
                    {"source": "top-level.com", "uids": [{"id": "top-uid", "atype": 3}]}
                ],
                "ext": {
                    "eids": [
                        {"source": "mocktioneer.dev", "uids": [{"id": "ext-uid", "atype": 3}]}
                    ]
                }
            }
        }))
        .unwrap();

        let info = extract_ec_info(&req);
        assert_eq!(info.eids_count, 1, "should use top-level eids");
        assert_eq!(info.eids[0].source, "top-level.com");
        assert!(
            !info.mocktioneer_matched,
            "ext.eids should be ignored when top-level is present"
        );
    }

    #[test]
    fn test_extract_ec_info_ext_eids_malformed_ignored() {
        // Malformed ext.eids should not crash — just produce empty eids
        let req: OpenRTBRequest = serde_json::from_value(serde_json::json!({
            "id": "bad-ext-req",
            "imp": [{"id": "1", "banner": {"w": 300, "h": 250}}],
            "user": {
                "ext": {
                    "eids": "not-an-array"
                }
            }
        }))
        .unwrap();

        let info = extract_ec_info(&req);
        assert_eq!(info.eids_count, 0);
        assert!(!info.mocktioneer_matched);
    }
}
