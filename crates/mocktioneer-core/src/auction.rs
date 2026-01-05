use crate::aps::{ApsBidRequest, ApsBidResponse, ApsContextual, ApsSlotResponse};
use crate::openrtb::{
    Bid as OpenrtbBid, Imp as OpenrtbImp, MediaType, OpenRTBRequest, OpenRTBResponse, SeatBid,
};
use crate::render::iframe_html;
use serde_json::json;
use uuid::Uuid;

fn new_id() -> String {
    Uuid::now_v7().simple().to_string()
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

pub fn standard_or_default(w: i64, h: i64) -> (i64, i64) {
    if is_standard_size(w, h) {
        (w, h)
    } else {
        (300, 250)
    }
}

pub fn build_openrtb_response_typed(req: &OpenRTBRequest, base_host: &str) -> OpenRTBResponse {
    let mut bids: Vec<OpenrtbBid> = Vec::new();
    for imp in req.imp.iter() {
        let impid = if imp.id.is_empty() { "1" } else { &imp.id };
        let (w, h) = size_from_imp(imp);
        let bid_id = new_id();
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
        let adm_html = iframe_html(base_host, &crid, w, h, bid_for_iframe);
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
            ..Default::default()
        }],
        ..Default::default()
    }
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
        let bid_id = new_id();
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
        let adm_html = iframe_html(base_host, &crid, w, h, bid_for_iframe);
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
            ..Default::default()
        }],
        ..Default::default()
    }
}

// ============================================================================
// APS TAM API Response Builder
// ============================================================================

/// Calculate mock APS price based on ad size.
/// Larger ad sizes typically command higher CPMs.
fn calculate_aps_price(width: i64, height: i64) -> f64 {
    let area = (width * height) as f64;

    // Base price calculation: larger ads = higher CPM
    // Standard ranges: $1.50 - $4.50
    let base_cpm = match (width, height) {
        // Premium large formats
        (970, 250) => 4.20,
        (970, 90) => 3.80,
        (300, 600) => 3.50,
        (160, 600) => 3.20,

        // Standard leaderboard
        (728, 90) => 3.00,

        // Medium rectangle (most common)
        (300, 250) => 2.50,
        (336, 280) => 2.60,

        // Mobile/smaller formats
        (320, 100) => 2.20,
        (320, 50) => 1.80,
        (300, 50) => 1.70,

        // Banner
        (468, 60) => 2.00,

        // Fallback based on area
        _ => 1.50 + (area / 100000.0).min(3.0),
    };

    // Round to 2 decimal places
    (base_cpm * 100.0).round() / 100.0
}

/// Encode APS price using base64 for mock testing.
/// Note: Real APS uses proprietary encoding that cannot be decoded without Amazon's keys.
/// This base64 encoding is only for mock/testing purposes.
fn encode_aps_price(price: f64) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    // Encode just the number as a string
    let price_str = price.to_string();
    STANDARD.encode(price_str.as_bytes())
}

/// Build APS TAM response from an APS bid request matching real Amazon API format.
///
/// This function generates mock bids for all slots with standard sizes:
/// - Fixed bid price: $2.50 CPM
/// - 100% fill rate for standard sizes
/// - Returns contextual format matching real Amazon APS API
/// - No creative HTML (APS doesn't return adm field)
/// - Generates encoded price strings and targeting keys
pub fn build_aps_response(req: &ApsBidRequest, base_host: &str) -> ApsBidResponse {
    let mut slots: Vec<ApsSlotResponse> = Vec::new();

    for slot in req.slots.iter() {
        // Take the first size from the sizes array
        let size_option = slot.sizes.first();
        if size_option.is_none() {
            // No sizes provided, skip this slot
            continue;
        }

        let [w, h] = *size_option.unwrap();
        let w_i64 = w as i64;
        let h_i64 = h as i64;

        // Only bid on standard sizes
        if !is_standard_size(w_i64, h_i64) {
            log::debug!(
                "APS: Skipping non-standard size {}x{} for slot '{}'",
                w,
                h,
                slot.slot_id
            );
            continue;
        }

        // Generate bid components
        let impression_id = new_id();
        let price = calculate_aps_price(w_i64, h_i64);
        let crid = format!("{}-{}", new_id(), "mocktioneer");
        let size_str = format!("{}x{}", w, h);

        // Generate base64-encoded price string (mock only - real APS uses proprietary encoding)
        let encoded_price = encode_aps_price(price);

        // Build slot response matching real Amazon format
        slots.push(ApsSlotResponse {
            slot_id: slot.slot_id.clone(),
            size: size_str.clone(),
            crid: Some(crid),
            media_type: Some("d".to_string()), // "d" for display
            fif: Some("1".to_string()),        // "1" = filled
            targeting: vec![
                "amzniid".to_string(),
                "amznp".to_string(),
                "amznsz".to_string(),
                "amznbid".to_string(),
                "amznactt".to_string(),
            ],
            meta: vec![
                "slotID".to_string(),
                "mediaType".to_string(),
                "size".to_string(),
            ],
            // Targeting key-value pairs (flat on slot object)
            amzniid: Some(impression_id),
            amznbid: Some(encoded_price.clone()),
            amznp: Some(encoded_price), // Same encoding for both fields
            amznsz: Some(size_str),
            amznactt: Some("OPEN".to_string()),
        });

        log::debug!(
            "APS: Generated bid for slot '{}' ({}x{}) at ${:.2}",
            slot.slot_id,
            w,
            h,
            price
        );
    }

    ApsBidResponse {
        contextual: ApsContextual {
            slots,
            host: Some(format!("https://{}", base_host)),
            status: Some("ok".to_string()),
            cfe: Some(true),
            ev: Some(true),
            cfn: Some("bao-csm/direct/csm_othersv6.js".to_string()),
            cb: Some("6".to_string()),
            cmp: None, // Optional campaign tracking URL
        },
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
        assert_eq!(standard_or_default(333, 222), (300, 250));
        assert_eq!(standard_or_default(320, 50), (320, 50));
    }

    #[test]
    fn test_build_openrtb_response_with_base_enforces_standard_sizes() {
        let req_v: serde_json::Value = serde_json::json!({
            "id": "r2",
            "imp": [{"id":"1","banner":{"w":333,"h":222}}]
        });
        let req: OpenRTBRequest = serde_json::from_value(req_v).unwrap();
        let resp = build_openrtb_response_with_base_typed(&req, "host.test");
        let bid = &resp.seatbid[0].bid[0];
        // Non-standard should default to 300x250
        assert_eq!(bid.w, Some(300));
        assert_eq!(bid.h, Some(250));
    }

    #[test]
    fn test_bid_id_is_hex_like_uuid() {
        let req_v: serde_json::Value = serde_json::json!({
            "id": "r3",
            "imp": [{"id":"1","banner":{"w":300,"h":250}}]
        });
        let req: OpenRTBRequest = serde_json::from_value(req_v).unwrap();
        let resp = build_openrtb_response_typed(&req, "host.test");
        let bid_id = &resp.seatbid[0].bid[0].id;
        assert_eq!(bid_id.len(), 32);
        assert!(
            bid_id
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "bid id not lower-hex32: {}",
            bid_id
        );
    }

    #[test]
    fn test_price_from_ext_and_iframe_bid_param() {
        let req_v: serde_json::Value = serde_json::json!({
            "id": "r4",
            "imp": [{
                "id":"1",
                "banner":{"w":300,"h":250},
                "ext": {"mocktioneer": {"bid": 2.5}}
            }]
        });
        let req: OpenRTBRequest = serde_json::from_value(req_v).unwrap();
        let resp = build_openrtb_response_with_base_typed(&req, "host.test");
        let bid = &resp.seatbid[0].bid[0];
        assert_eq!(bid.price, 2.5);
        let ext_bid = bid
            .ext
            .as_ref()
            .and_then(|e| e.get("mocktioneer"))
            .and_then(|m| m.get("bid"))
            .and_then(|v| v.as_f64())
            .unwrap();
        assert_eq!(ext_bid, 2.5);
        // Iframe should include bid=2.50 parameter
        let adm = bid.adm.as_ref().unwrap();
        assert!(adm.contains("bid=2.50"));
    }
}
