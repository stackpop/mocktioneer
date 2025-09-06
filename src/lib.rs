use rand::{distributions::Alphanumeric, Rng};
use serde_json::{json, Value};

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

pub fn banner_adm_html(crid: &str, w: i64, h: i64) -> String {
    format!(
        "<html><body style=\"margin:0;padding:0;\"><a href=\"/click?crid={crid}&w={w}&h={h}\" target=\"_blank\"><div style=\"display:flex;align-items:center;justify-content:center;width:{w}px;height:{h}px;background:#dbeafe;border:1px solid #93c5fd;font:14px/1.2 system-ui,sans-serif;color:#1e3a8a\">Mocktioneer {w}x{h} (crid={crid})</div></a></body></html>",
        crid=escape_html(crid), w=w, h=h
    )
}

pub fn size_from_imp(imp: &Value) -> (i64, i64) {
    // Prefer imp.banner.w/h; fallback to banner.format[0].w/h; default 300x250
    let banner = imp.get("banner");
    if let Some(banner) = banner {
        let w = banner.get("w").and_then(|v| v.as_i64());
        let h = banner.get("h").and_then(|v| v.as_i64());
        if let (Some(w), Some(h)) = (w, h) {
            return (w, h);
        }
        if let Some(fmt) = banner.get("format").and_then(|v| v.as_array()) {
            if let Some(fmt0) = fmt.first() {
                let w = fmt0.get("w").and_then(|v| v.as_i64()).unwrap_or(300);
                let h = fmt0.get("h").and_then(|v| v.as_i64()).unwrap_or(250);
                return (w, h);
            }
        }
    }
    (300, 250)
}

// duplicate removed

pub fn build_openrtb_response(parsed: &Value) -> Value {
    let req_id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("req")
        .to_string();
    let imps = parsed
        .get("imp")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut bids = vec![];
    for imp in imps.iter() {
        let impid = imp.get("id").and_then(|v| v.as_str()).unwrap_or("1");
        let (w, h) = size_from_imp(imp);
        let bid_id = rand_id(12);
        let crid = format!("mocktioneer-{}", impid);
        let adm_html = banner_adm_html(&crid, w, h);

        bids.push(json!({
            "id": bid_id,
            "impid": impid,
            "price": 1.23,
            "adm": adm_html,
            "crid": crid,
            "w": w,
            "h": h,
            "adomain": ["example.com"],
        }));
    }

    json!({
        "id": req_id,
        "cur": "USD",
        "seatbid": [ { "seat": "mocktioneer", "bid": bids } ]
    })
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

pub fn banner_adm_iframe(base_host: &str, crid: &str, w: i64, h: i64) -> String {
    let safe_crid = escape_html(crid);
    format!(
        "<iframe src=\"//{host}/static/creatives/{w}x{h}.html?crid={crid}\" width=\"{w}\" height=\"{h}\" frameborder=\"0\" scrolling=\"no\"></iframe>",
        host = base_host,
        w = w,
        h = h,
        crid = safe_crid
    )
}

pub fn build_openrtb_response_with_base(parsed: &Value, base_host: &str) -> Value {
    let req_id = parsed
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("req")
        .to_string();
    let imps = parsed
        .get("imp")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut bids = vec![];
    for imp in imps.iter() {
        let impid = imp.get("id").and_then(|v| v.as_str()).unwrap_or("1");
        let (mut w, mut h) = size_from_imp(imp);
        let bid_id = rand_id(12);
        (w, h) = standard_or_default(w, h);
        let crid = format!("mocktioneer-{}", impid);
        let adm_html = banner_adm_iframe(base_host, &crid, w, h);

        bids.push(json!({
            "id": bid_id,
            "impid": impid,
            "price": 1.23,
            "adm": adm_html,
            "crid": crid,
            "w": w,
            "h": h,
            "adomain": ["example.com"],
        }));
    }

    json!({
        "id": req_id,
        "cur": "USD",
        "seatbid": [ { "seat": "mocktioneer", "bid": bids } ]
    })
}
// (duplicate removed)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_from_imp_defaults_and_format() {
        let v: Value = serde_json::json!({"id":"1","banner":{}});
        assert_eq!(size_from_imp(&v), (300, 250));

        let v: Value = serde_json::json!({"id":"1","banner":{"format":[{"w":320,"h":50}]}});
        assert_eq!(size_from_imp(&v), (320, 50));

        let v: Value = serde_json::json!({"id":"1","banner":{"w":728,"h":90}});
        assert_eq!(size_from_imp(&v), (728, 90));
    }

    #[test]
    fn test_build_openrtb_response_structure() {
        let req: Value = serde_json::json!({
            "id": "r1",
            "imp": [{"id":"1","banner":{"w":300,"h":250}}]
        });
        let resp = build_openrtb_response(&req);
        assert_eq!(resp["id"], "r1");
        assert_eq!(resp["cur"], "USD");
        assert!(resp["seatbid"].is_array());
        assert!(resp["seatbid"][0]["bid"].is_array());
        let bid = &resp["seatbid"][0]["bid"][0];
        assert_eq!(bid["impid"], "1");
        assert_eq!(bid["w"], 300);
        assert_eq!(bid["h"], 250);
        assert!(bid["adm"].is_string());
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
        let adm = banner_adm_iframe("host.test", "abc&def\"", 300, 250);
        assert!(adm.contains("//host.test/static/creatives/300x250.html?crid=abc&amp;def&quot;"));
        assert!(adm.contains("width=\"300\""));
        assert!(adm.contains("height=\"250\""));
    }

    #[test]
    fn test_build_openrtb_response_with_base_standard_and_defaulted_sizes() {
        // standard size
        let req_std: Value = serde_json::json!({
            "id": "r2",
            "imp": [{"id":"1","banner":{"w":320,"h":50}}]
        });
        let resp_std = build_openrtb_response_with_base(&req_std, "host.test");
        let adm_std = resp_std["seatbid"][0]["bid"][0]["adm"].as_str().unwrap();
        assert!(adm_std.contains("//host.test/static/creatives/320x50.html"));

        // non-standard should default to 300x250
        let req_def: Value = serde_json::json!({
            "id": "r3",
            "imp": [{"id":"1","banner":{"w":333,"h":222}}]
        });
        let resp_def = build_openrtb_response_with_base(&req_def, "host.test");
        let adm_def = resp_def["seatbid"][0]["bid"][0]["adm"].as_str().unwrap();
        assert!(adm_def.contains("//host.test/static/creatives/300x250.html"));
    }

    #[test]
    fn test_escape_html_basic() {
        assert_eq!(escape_html("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }
}

// wasm entrypoint lives in src/main.rs for Fastly Compute build
