use crate::aps::{ApsBidRequest, ApsBidResponse, ApsContextual, ApsSlotResponse};
use crate::openrtb::{
    Bid as OpenrtbBid, Imp as OpenrtbImp, MediaType, OpenRTBRequest, OpenRTBResponse, SeatBid,
};
use crate::render::iframe_html;
use phf::phf_map;
use serde_json::json;
use uuid::Uuid;

// ============================================================================
// Standard Ad Sizes - single source of truth for supported sizes and pricing
// ============================================================================

/// Default CPM for non-standard sizes (base price before area adjustment).
pub const DEFAULT_CPM: f64 = 1.50;

/// Maximum area-based bonus added to DEFAULT_CPM for non-standard sizes.
/// Final CPM = DEFAULT_CPM + min(area/100000, MAX_AREA_BONUS)
pub const MAX_AREA_BONUS: f64 = 3.00;

/// Compile-time perfect hash map for standard sizes: "WxH" -> cpm.
/// Zero runtime initialization cost.
static SIZE_MAP: phf::Map<&'static str, f64> = phf_map! {
    // Desktop & General Display Sizes
    "300x250" => 2.50,  // Medium Rectangle
    "336x280" => 2.60,  // Large Rectangle
    "728x90" => 3.00,   // Leaderboard
    "970x90" => 3.80,   // Large Leaderboard
    "160x600" => 3.20,  // Wide Skyscraper
    "300x600" => 3.50,  // Half Page
    "970x250" => 4.20,  // Billboard
    "468x60" => 2.00,   // Banner
    // Mobile-Specific Sizes
    "320x50" => 1.80,   // Mobile Leaderboard
    "300x50" => 1.70,   // Mobile Banner (alternative)
    "320x100" => 2.20,  // Large Mobile Banner
    "320x480" => 2.80,  // Mobile Interstitial Portrait
    "480x320" => 2.80,  // Mobile Interstitial Landscape
};

/// Format dimensions as lookup key.
#[inline]
fn size_key(w: i64, h: i64) -> String {
    format!("{}x{}", w, h)
}

/// Check if dimensions match a standard ad size.
pub fn is_standard_size(w: i64, h: i64) -> bool {
    SIZE_MAP.contains_key(size_key(w, h).as_str())
}

/// Get CPM for a size. Returns configured CPM for standard sizes, area-based fallback otherwise.
pub fn get_cpm(w: i64, h: i64) -> f64 {
    SIZE_MAP.get(size_key(w, h).as_str()).copied().unwrap_or_else(|| {
        // Fallback: area-based pricing for non-standard sizes
        let area = (w * h) as f64;
        ((DEFAULT_CPM + (area / 100000.0).min(MAX_AREA_BONUS)) * 100.0).round() / 100.0
    })
}

/// Returns an iterator over all standard ad sizes as (width, height) tuples.
/// Useful for generating test fixtures or validating external configurations.
pub fn standard_sizes() -> impl Iterator<Item = (i64, i64)> {
    let mut sizes: Vec<(i64, i64)> = SIZE_MAP
        .keys()
        .filter_map(|key| {
            let (w_str, h_str) = key.split_once('x')?;
            let w = w_str.parse::<i64>().ok()?;
            let h = h_str.parse::<i64>().ok()?;
            Some((w, h))
        })
        .collect();
    sizes.sort_unstable();
    debug_assert_eq!(
        sizes.len(),
        SIZE_MAP.len(),
        "SIZE_MAP contains invalid size keys"
    );
    sizes.into_iter()
}

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

pub fn standard_or_default((w, h): (i64, i64)) -> (i64, i64) {
    if is_standard_size(w, h) {
        (w, h)
    } else {
        (300, 250)
    }
}

/// Build an OpenRTB bid response for the given request.
///
/// - Enforces standard ad sizes (non-standard sizes default to 300x250)
/// - Uses size-based CPM pricing ($1.70 - $4.20 depending on size)
/// - Price can be overridden via `imp.ext.mocktioneer.bid`
pub fn build_openrtb_response(
    req: &OpenRTBRequest,
    base_host: &str,
) -> OpenRTBResponse {
    let mut bids: Vec<OpenrtbBid> = Vec::new();
    for imp in req.imp.iter() {
        let impid = if imp.id.is_empty() { "1" } else { &imp.id };
        let (w, h) = standard_or_default(size_from_imp(imp));
        let bid_id = new_id();
        let crid = format!("mocktioneer-{}", impid);
        // Extract custom bid from imp.ext.mocktioneer.bid if present
        let custom_bid = imp
            .ext
            .as_ref()
            .and_then(|e| e.mocktioneer.as_ref())
            .and_then(|m| m.bid);

        // Use custom bid if provided, otherwise use size-based CPM
        let price = custom_bid.unwrap_or_else(|| get_cpm(w, h));
        let bid_ext = custom_bid.map(|b| json!({"mocktioneer": {"bid": b}}));
        let bid_for_iframe = custom_bid;
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

/// Encode APS price using base64 for mock testing.
///
/// Note: Real Amazon APS uses proprietary encoding that cannot be decoded without Amazon's keys.
/// Our mock uses transparent base64 encoding that CAN be decoded for testing/debugging purposes.
/// Example: `echo "Mi41MA==" | base64 -d` → `2.50`
fn encode_aps_price(price: f64) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let price_str = price.to_string();
    STANDARD.encode(price_str.as_bytes())
}

/// Decode APS price from base64 (mock format only).
///
/// Returns `None` if the string is not valid base64 or doesn't contain a valid price.
/// This only works with mocktioneer-encoded prices; real APS prices cannot be decoded.
pub fn decode_aps_price(encoded: &str) -> Option<f64> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let decoded = STANDARD.decode(encoded).ok()?;
    let price_str = String::from_utf8(decoded).ok()?;
    price_str.parse().ok()
}

/// Build APS TAM response from an APS bid request matching real Amazon API format.
///
/// This function generates mock bids for all slots with standard sizes:
/// - Variable bid prices based on ad size (via `get_cpm()`)
///   - Ranges from $1.70 - $4.20 CPM for standard sizes
///   - Example: 300x250 = $2.50, 970x250 = $4.20, 320x50 = $1.80
/// - 100% fill rate for standard sizes
/// - Returns contextual format matching real Amazon APS API
/// - No creative HTML (APS doesn't return adm field)
/// - Generates base64-encoded price strings (recoverable in mock, unlike real APS)
pub fn build_aps_response(req: &ApsBidRequest, base_host: &str) -> ApsBidResponse {
    let mut slots: Vec<ApsSlotResponse> = Vec::new();

    for slot in req.slots.iter() {
        // Find the standard size with the highest CPM from all sizes in the slot
        let best_size = slot
            .sizes
            .iter()
            .filter_map(|&[w, h]| {
                let w_i64 = w as i64;
                let h_i64 = h as i64;
                if is_standard_size(w_i64, h_i64) {
                    let price = get_cpm(w_i64, h_i64);
                    Some((w, h, price))
                } else {
                    None
                }
            })
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        let Some((w, h, price)) = best_size else {
            // No standard sizes found, skip this slot
            log::debug!(
                "APS: Skipping slot '{}' - no standard sizes in {:?}",
                slot.slot_id,
                slot.sizes
            );
            continue;
        };

        // Generate bid components (price already calculated in best_size selection)
        let impression_id = new_id();
        let crid = format!("{}-{}", new_id(), "mocktioneer");
        let size_str = format!("{}x{}", w, h);

        // Generate base64-encoded price string (recoverable in mock - real APS uses proprietary encoding)
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
    use crate::aps::ApsSlot;
    use crate::openrtb::{Banner, ExtMocktioneer, Format, ImpExt};

    #[test]
    fn test_size_from_imp_defaults_and_format() {
        // Empty banner defaults to 300x250
        let imp = OpenrtbImp {
            id: "1".to_string(),
            banner: Some(Banner::default()),
            ..Default::default()
        };
        assert_eq!(size_from_imp(&imp), (300, 250));

        // Uses format[0] when w/h not set
        let imp = OpenrtbImp {
            id: "1".to_string(),
            banner: Some(Banner {
                format: Some(vec![Format {
                    w: 320,
                    h: 50,
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(size_from_imp(&imp), (320, 50));

        // Prefers explicit w/h over format
        let imp = OpenrtbImp {
            id: "1".to_string(),
            banner: Some(Banner {
                w: Some(728),
                h: Some(90),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(size_from_imp(&imp), (728, 90));
    }

    #[test]
    fn test_build_openrtb_response_structure() {
        let req = OpenRTBRequest {
            id: "r1".to_string(),
            imp: vec![OpenrtbImp {
                id: "1".to_string(),
                banner: Some(Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let resp = build_openrtb_response(&req, "host.test");
        assert_eq!(resp.id, "r1");
        assert_eq!(resp.cur.as_deref(), Some("USD"));
        assert_eq!(resp.seatbid.len(), 1);
        assert!(!resp.seatbid[0].bid.is_empty());
        let bid = &resp.seatbid[0].bid[0];
        assert_eq!(bid.impid, "1");
        assert_eq!(bid.w, Some(300));
        assert_eq!(bid.h, Some(250));
        assert_eq!(bid.mtype, Some(MediaType::Banner));
        assert!(bid.adm.is_some());
    }

    #[test]
    fn test_is_standard_size() {
        // Standard sizes should be recognized
        assert!(is_standard_size(300, 250));
        assert!(is_standard_size(728, 90));
        // Non-standard sizes should not
        assert!(!is_standard_size(333, 222));
        assert!(!is_standard_size(0, 0));
        assert!(!is_standard_size(300, 251));
    }

    #[test]
    fn test_standard_or_default_behavior() {
        assert_eq!(standard_or_default((333, 222)), (300, 250));
        assert_eq!(standard_or_default((320, 50)), (320, 50));
    }

    #[test]
    fn test_build_openrtb_response_enforces_standard_sizes() {
        let req = OpenRTBRequest {
            id: "r2".to_string(),
            imp: vec![OpenrtbImp {
                id: "1".to_string(),
                banner: Some(Banner {
                    w: Some(333),
                    h: Some(222),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let resp = build_openrtb_response(&req, "host.test");
        let bid = &resp.seatbid[0].bid[0];
        // Non-standard should default to 300x250
        assert_eq!(bid.w, Some(300));
        assert_eq!(bid.h, Some(250));
    }

    #[test]
    fn test_bid_id_is_hex_like_uuid() {
        let req = OpenRTBRequest {
            id: "r3".to_string(),
            imp: vec![OpenrtbImp {
                id: "1".to_string(),
                banner: Some(Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let resp = build_openrtb_response(&req, "host.test");
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
        let req = OpenRTBRequest {
            id: "r4".to_string(),
            imp: vec![OpenrtbImp {
                id: "1".to_string(),
                banner: Some(Banner {
                    w: Some(300),
                    h: Some(250),
                    ..Default::default()
                }),
                ext: Some(ImpExt {
                    mocktioneer: Some(ExtMocktioneer { bid: Some(2.5) }),
                }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let resp = build_openrtb_response(&req, "host.test");
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

    // ========================================================================
    // APS build_aps_response tests
    // ========================================================================

    #[test]
    fn test_build_aps_response_single_standard_size() {
        let req = ApsBidRequest {
            pub_id: "test".to_string(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_string(),
                sizes: vec![[300, 250]],
                slot_name: None,
            }],
            page_url: None,
            user_agent: None,
            timeout: None,
        };
        let resp = build_aps_response(&req, "mock.test");

        assert_eq!(resp.contextual.status, Some("ok".to_string()));
        assert_eq!(resp.contextual.slots.len(), 1);

        let slot = &resp.contextual.slots[0];
        assert_eq!(slot.slot_id, "slot1");
        assert_eq!(slot.size, "300x250");
        assert_eq!(slot.media_type, Some("d".to_string()));
        assert_eq!(slot.fif, Some("1".to_string()));
        assert!(slot.amzniid.is_some());
        assert!(slot.amznbid.is_some());
    }

    #[test]
    fn test_build_aps_response_skips_non_standard_sizes() {
        let req = ApsBidRequest {
            pub_id: "test".to_string(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_string(),
                sizes: vec![[333, 222]], // Non-standard
                slot_name: None,
            }],
            page_url: None,
            user_agent: None,
            timeout: None,
        };
        let resp = build_aps_response(&req, "mock.test");

        // Non-standard sizes should be skipped
        assert!(resp.contextual.slots.is_empty());
    }

    #[test]
    fn test_build_aps_response_selects_highest_cpm() {
        let req = ApsBidRequest {
            pub_id: "test".to_string(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_string(),
                sizes: vec![[300, 250], [970, 250]], // 970x250 has higher CPM ($4.20 vs $2.50)
                slot_name: None,
            }],
            page_url: None,
            user_agent: None,
            timeout: None,
        };
        let resp = build_aps_response(&req, "mock.test");

        assert_eq!(resp.contextual.slots.len(), 1);
        let slot = &resp.contextual.slots[0];
        assert_eq!(slot.size, "970x250"); // Should pick higher CPM size
    }

    #[test]
    fn test_build_aps_response_price_encoding_is_base64() {
        let req = ApsBidRequest {
            pub_id: "test".to_string(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_string(),
                sizes: vec![[300, 250]], // CPM is $2.50
                slot_name: None,
            }],
            page_url: None,
            user_agent: None,
            timeout: None,
        };
        let resp = build_aps_response(&req, "mock.test");
        let slot = &resp.contextual.slots[0];

        // Use decode_aps_price to verify the encoded price
        let price = decode_aps_price(slot.amznbid.as_ref().unwrap()).unwrap();
        assert_eq!(price, 2.5);
    }

    #[test]
    fn test_build_aps_response_targeting_keys() {
        let req = ApsBidRequest {
            pub_id: "test".to_string(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_string(),
                sizes: vec![[728, 90]],
                slot_name: None,
            }],
            page_url: None,
            user_agent: None,
            timeout: None,
        };
        let resp = build_aps_response(&req, "mock.test");
        let slot = &resp.contextual.slots[0];

        // Verify targeting keys list
        assert!(slot.targeting.contains(&"amzniid".to_string()));
        assert!(slot.targeting.contains(&"amznbid".to_string()));
        assert!(slot.targeting.contains(&"amznp".to_string()));
        assert!(slot.targeting.contains(&"amznsz".to_string()));
        assert!(slot.targeting.contains(&"amznactt".to_string()));

        // Verify corresponding values are set
        assert!(slot.amzniid.is_some());
        assert!(slot.amznbid.is_some());
        assert!(slot.amznp.is_some());
        assert_eq!(slot.amznsz, Some("728x90".to_string()));
        assert_eq!(slot.amznactt, Some("OPEN".to_string()));
    }

    #[test]
    fn test_decode_aps_price_roundtrip() {
        // Valid encoded prices
        assert_eq!(decode_aps_price("Mi41"), Some(2.5));
        assert_eq!(decode_aps_price("NC4y"), Some(4.2));
        assert_eq!(decode_aps_price("MS43"), Some(1.7));

        // Invalid inputs
        assert_eq!(decode_aps_price("not-base64!!!"), None);
        assert_eq!(decode_aps_price("aGVsbG8="), None); // "hello" - not a number
        assert_eq!(decode_aps_price(""), None);
    }
}
