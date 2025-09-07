use rand::{distributions::Alphanumeric, Rng};

pub mod openrtb;
use openrtb::{
    Bid as OpenrtbBid, Imp as OpenrtbImp, MediaType, OpenRTBRequest, OpenRTBResponse, SeatBid,
};
use serde_json::json;

pub fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn rand_id(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub fn size_from_imp(imp: &OpenrtbImp) -> (i64, i64) {
    // Prefer imp.banner.w/h; fallback to banner.format[0].w/h; default 300x250
    if let Some(banner) = &imp.banner {
        if let (Some(w), Some(h)) = (banner.w, banner.h) {
            return (w, h);
        }
        if let Some(fmt) = &banner.format {
            if let Some(fmt0) = fmt.first() {
                let w = fmt0.w;
                let h = fmt0.h;
                return (w, h);
            }
        }
    }
    (300, 250)
}

pub fn build_openrtb_response_typed(req: &OpenRTBRequest, base_host: &str) -> OpenRTBResponse {
    let mut bids: Vec<OpenrtbBid> = Vec::new();
    for imp in req.imp.iter() {
        let impid = if imp.id.is_empty() { "1" } else { &imp.id };
        let (w, h) = size_from_imp(imp);
        let bid_id = rand_id(12);
        let crid = format!("mocktioneer-{}", impid);
        // Extract numeric bid param from imp.ext.mocktioneer.bid if present; use as price
        let mut price = 1.23_f64;
        let bid_ext = imp
            .ext
            .as_ref()
            .and_then(|e| e.mocktioneer.as_ref())
            .and_then(|m| m.bid)
            .map(|f| {
                price = f;
                json!({"mocktioneer": {"bid": f}})
            });
        let bid_for_iframe = if bid_ext.is_some() { Some(price) } else { None };
        let adm_html = banner_adm_iframe(base_host, &crid, w, h, bid_for_iframe);
        bids.push(OpenrtbBid {
            id: bid_id,
            impid: impid.to_string(),
            price,
            adm: Some(adm_html),
            crid: Some(crid),
            w: Some(w),
            h: Some(h),
            mtype: Some(MediaType::Banner),
            adomain: Some(vec!["example.com".to_string()]),
            ext: bid_ext,
            ..Default::default()
        });
    }
    OpenRTBResponse {
        id: if req.id.is_empty() {
            "req".to_string()
        } else {
            req.id.clone()
        },
        cur: Some("USD".to_string()),
        seatbid: vec![SeatBid {
            seat: Some("mocktioneer".to_string()),
            bid: bids,
        }],
    }
}

pub fn is_standard_size(w: i64, h: i64) -> bool {
    matches!(
        (w, h),
        (300, 250)
            | (320, 50)
            | (728, 90)
            | (160, 600)
            | (300, 50)
            | (300, 600)
            | (970, 250)
            | (468, 60)
            | (336, 280)
            | (320, 100)
    )
}

fn standard_or_default(w: i64, h: i64) -> (i64, i64) {
    if is_standard_size(w, h) {
        (w, h)
    } else {
        (300, 250)
    }
}

pub fn banner_adm_iframe(base_host: &str, crid: &str, w: i64, h: i64, bid: Option<f64>) -> String {
    const IFRAME_TMPL: &str = include_str!("../static/templates/iframe.html");
    let safe_crid = escape_html(crid);
    let bid_str = bid.map(|b| format!("{:.2}", b)).unwrap_or_default();
    IFRAME_TMPL
        .replace("{{HOST}}", base_host)
        .replace("{{W}}", &w.to_string())
        .replace("{{H}}", &h.to_string())
        .replace("{{CRID}}", &safe_crid)
        .replace("{{BID}}", &bid_str)
}

pub fn render_svg(w: i64, h: i64, bid: Option<f64>) -> String {
    const SVG_TMPL: &str = include_str!("../static/templates/image.svg");
    let pad = ((w.min(h) as f64) * 0.08).round() as i64;
    let mut text_len = w - 2 * pad;
    if text_len < 1 { text_len = 1; }
    let font = (h as f64 * 0.28).round() as i64;
    let mut cap_font = ((w.min(h) as f64) * 0.16).round() as i64;
    if cap_font < 10 { cap_font = 10; }
    let mut stroke = ((w.min(h) as f64) * 0.03).round() as i64;
    if stroke < 2 { stroke = 2; }
    let xbr = (w - pad - stroke).max(0);
    let ybr = (h - pad - stroke).max(0);
    let xtl = (pad + stroke).max(0);
    let ytl = (pad + stroke).max(0);
    let bid_label = bid.map(|b| format!(" — ${:.2}", b)).unwrap_or_default();
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
        .replace("{{BIDLBL}}", &bid_label)
}

pub fn build_openrtb_response_with_base_typed(
    req: &OpenRTBRequest,
    base_host: &str,
) -> OpenRTBResponse {
    let mut bids: Vec<OpenrtbBid> = Vec::new();
    for imp in req.imp.iter() {
        let impid = if imp.id.is_empty() { "1" } else { &imp.id };
        let (mut w, mut h) = size_from_imp(imp);
        (w, h) = standard_or_default(w, h);
        let bid_id = rand_id(12);
        let crid = format!("mocktioneer-{}", impid);
        let mut price = 1.23_f64;
        let bid_ext = imp
            .ext
            .as_ref()
            .and_then(|e| e.mocktioneer.as_ref())
            .and_then(|m| m.bid)
            .map(|f| {
                price = f;
                json!({"mocktioneer": {"bid": f}})
            });
        let bid_for_iframe = if bid_ext.is_some() { Some(price) } else { None };
        let adm_html = banner_adm_iframe(base_host, &crid, w, h, bid_for_iframe);
        bids.push(OpenrtbBid {
            id: bid_id,
            impid: impid.to_string(),
            price,
            adm: Some(adm_html),
            crid: Some(crid),
            w: Some(w),
            h: Some(h),
            mtype: Some(MediaType::Banner),
            adomain: Some(vec!["example.com".to_string()]),
            ext: bid_ext,
            ..Default::default()
        });
    }
    OpenRTBResponse {
        id: if req.id.is_empty() {
            "req".to_string()
        } else {
            req.id.clone()
        },
        cur: Some("USD".to_string()),
        seatbid: vec![SeatBid {
            seat: Some("mocktioneer".to_string()),
            bid: bids,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_from_imp_defaults_and_format() {
        let v: serde_json::Value = serde_json::json!({"id":"1","banner":{}});
        let imp: OpenrtbImp = serde_json::from_value(v).unwrap();
        assert_eq!(size_from_imp(&imp), (300, 250));

        let v: serde_json::Value =
            serde_json::json!({"id":"1","banner":{"format":[{"w":320,"h":50}]}});
        let imp: OpenrtbImp = serde_json::from_value(v).unwrap();
        assert_eq!(size_from_imp(&imp), (320, 50));

        let v: serde_json::Value = serde_json::json!({"id":"1","banner":{"w":728,"h":90}});
        let imp: OpenrtbImp = serde_json::from_value(v).unwrap();
        assert_eq!(size_from_imp(&imp), (728, 90));
    }

    #[test]
    fn test_build_openrtb_response_structure() {
        let req_v: serde_json::Value = serde_json::json!({
            "id": "r1",
            "imp": [{"id":"1","banner":{"w":300,"h":250}}]
        });
        let req: OpenRTBRequest = serde_json::from_value(req_v).unwrap();
        let resp = build_openrtb_response_typed(&req, "host.test");
        assert_eq!(resp.id, "r1");
        assert_eq!(resp.cur.as_deref(), Some("USD"));
        assert_eq!(resp.seatbid.len(), 1);
        assert!(!resp.seatbid[0].bid.is_empty());
        let bid = &resp.seatbid[0].bid[0];
        assert_eq!(bid.impid, "1");
        assert_eq!(bid.w, Some(300));
        assert_eq!(bid.h, Some(250));
        assert_eq!(bid.mtype, Some(MediaType::Banner));
        assert!(bid.adm.as_ref().is_some());
    }

    #[test]
    fn test_is_standard_size_true() {
        let sizes = [
            (300, 250),
            (320, 50),
            (728, 90),
            (160, 600),
            (300, 50),
            (300, 600),
            (970, 250),
            (468, 60),
            (336, 280),
            (320, 100),
        ];
        for (w, h) in sizes {
            assert!(is_standard_size(w, h), "{}x{} should be standard", w, h);
        }
    }

    #[test]
    fn test_is_standard_size_false() {
        assert!(!is_standard_size(333, 222));
        assert!(!is_standard_size(0, 0));
        assert!(!is_standard_size(300, 251));
    }

    #[test]
    fn test_standard_or_default_behavior() {
        assert_eq!(super::standard_or_default(300, 250), (300, 250));
        assert_eq!(super::standard_or_default(333, 222), (300, 250));
    }

    #[test]
    fn test_banner_adm_iframe_contains_expected_src_and_escapes() {
        let adm = banner_adm_iframe("host.test", "abc&def\"", 300, 250, None);
        assert!(adm.contains("//host.test/static/creatives/300x250.html?crid=abc&amp;def&quot;"));
        assert!(adm.contains("width=\"300\""));
        assert!(adm.contains("height=\"250\""));
    }

    #[test]
    fn test_build_openrtb_response_with_base_standard_and_defaulted_sizes() {
        // standard size
        let req_std_v: serde_json::Value = serde_json::json!({
            "id": "r2",
            "imp": [{"id":"1","banner":{"w":320,"h":50}}]
        });
        let req_std: OpenRTBRequest = serde_json::from_value(req_std_v).unwrap();
        let resp_std = build_openrtb_response_with_base_typed(&req_std, "host.test");
        let adm_std = resp_std.seatbid[0].bid[0].adm.as_ref().unwrap();
        assert!(adm_std.contains("//host.test/static/creatives/320x50.html"));
        assert_eq!(resp_std.seatbid[0].bid[0].mtype, Some(MediaType::Banner));

        // non-standard should default to 300x250
        let req_def_v: serde_json::Value = serde_json::json!({
            "id": "r3",
            "imp": [{"id":"1","banner":{"w":333,"h":222}}]
        });
        let req_def: OpenRTBRequest = serde_json::from_value(req_def_v).unwrap();
        let resp_def = build_openrtb_response_with_base_typed(&req_def, "host.test");
        let adm_def = resp_def.seatbid[0].bid[0].adm.as_ref().unwrap();
        assert!(adm_def.contains("//host.test/static/creatives/300x250.html"));
    }

    #[test]
    fn test_escape_html_basic() {
        assert_eq!(escape_html("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn test_bid_ext_echo_present_and_absent() {
        // present: imp.ext.mocktioneer.bid should be echoed to bid.ext.mocktioneer.bid
        let req_with_bid_v: serde_json::Value = serde_json::json!({
            "id": "r_with_bid",
            "imp": [{
                "id": "1",
                "banner": {"w": 300, "h": 250},
                "ext": {"mocktioneer": {"bid": 2.34}}
            }]
        });
        let req_with_bid: OpenRTBRequest = serde_json::from_value(req_with_bid_v).unwrap();
        let resp_with_bid = build_openrtb_response_typed(&req_with_bid, "host.test");
        let bid = &resp_with_bid.seatbid[0].bid[0];
        let echoed = bid.ext.as_ref().unwrap();
        assert_eq!(echoed["mocktioneer"]["bid"], 2.34);
        assert!((bid.price - 2.34).abs() < 0.0001);

        // absent: no imp.ext => bid.ext should be None
        let req_no_bid_v: serde_json::Value = serde_json::json!({
            "id": "r_no_bid",
            "imp": [{
                "id": "1",
                "banner": {"w": 300, "h": 250}
            }]
        });
        let req_no_bid: OpenRTBRequest = serde_json::from_value(req_no_bid_v).unwrap();
        let resp_no_bid = build_openrtb_response_typed(&req_no_bid, "host.test");
        assert!(resp_no_bid.seatbid[0].bid[0].ext.is_none());
        assert!((resp_no_bid.seatbid[0].bid[0].price - 1.23).abs() < 0.0001);
    }

    #[test]
    fn test_render_svg_includes_bid_label_when_present() {
        let svg = render_svg(300, 250, Some(2.5));
        assert!(svg.contains("$2.50"));
        let svg2 = render_svg(300, 250, None);
        assert!(!svg2.contains("$"));
    }
}

// wasm entrypoint lives in src/main.rs for Fastly Compute build
