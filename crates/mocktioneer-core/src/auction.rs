use crate::aps::{ApsBidRequest, ApsBidResponse, ApsContextual, ApsSlotResponse};
use crate::openrtb::{
    Bid as OpenrtbBid, Imp as OpenrtbImp, MediaType, OpenRTBRequest, OpenRTBResponse, SeatBid,
};
use crate::render::{iframe_html, CreativeMetadata, SignatureStatus};
use uuid::Uuid;

// ============================================================================
// Standard Ad Sizes - single source of truth for supported sizes
// ============================================================================

/// Fixed CPM used for all Mocktioneer-generated bids.
pub const FIXED_BID_CPM: f64 = 0.20;

/// Standard IAB ad sizes supported by Mocktioneer.
/// Sorted by (width, height) for deterministic iteration order.
const STANDARD_SIZES: [(i64, i64); 13] = [
    // Desktop & General Display Sizes
    (160, 600), // Wide Skyscraper
    (300, 50),  // Mobile Banner (alternative)
    (300, 250), // Medium Rectangle
    (300, 600), // Half Page
    (320, 50),  // Mobile Leaderboard
    (320, 100), // Large Mobile Banner
    (320, 480), // Mobile Interstitial Portrait
    (336, 280), // Large Rectangle
    (468, 60),  // Banner
    (480, 320), // Mobile Interstitial Landscape
    (728, 90),  // Leaderboard
    (970, 90),  // Large Leaderboard
    (970, 250), // Billboard
];

/// Check if dimensions match a standard ad size.
pub fn is_standard_size(w: i64, h: i64) -> bool {
    STANDARD_SIZES.iter().any(|&(sw, sh)| sw == w && sh == h)
}

/// Returns an iterator over all standard ad sizes as (width, height) tuples.
/// Useful for generating test fixtures or validating external configurations.
pub fn standard_sizes() -> impl Iterator<Item = (i64, i64)> {
    STANDARD_SIZES.iter().copied()
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
/// - Uses a fixed CPM price ($0.20)
/// - Embeds signature verification status, the original request, and a preview
///   of the response as HTML comments in each creative
/// - The signature badge is rendered inside the creative via the `sig` query param
pub fn build_openrtb_response(
    req: &OpenRTBRequest,
    base_host: &str,
    signature_status: SignatureStatus,
) -> OpenRTBResponse {
    // Build bids without adm
    let mut bids: Vec<OpenrtbBid> = Vec::new();
    for imp in req.imp.iter() {
        let (w, h) = standard_or_default(size_from_imp(imp));
        let bid_id = new_id();
        let crid = format!("mocktioneer-{}", imp.id);

        // Warn when callers supply a bid override that is no longer honored
        if imp
            .ext
            .as_ref()
            .and_then(|e| e.mocktioneer.as_ref())
            .and_then(|m| m.bid)
            .is_some()
        {
            log::warn!(
                "imp[{}].ext.mocktioneer.bid is deprecated and ignored; \
                 all bids use fixed price ${}",
                imp.id,
                FIXED_BID_CPM
            );
        }

        let price = FIXED_BID_CPM;

        bids.push(OpenrtbBid {
            id: bid_id,
            impid: imp.id.clone(),
            price,
            adm: None, // Filled after metadata is built
            crid: Some(crid),
            w: Some(w),
            h: Some(h),
            mtype: Some(MediaType::Banner),
            adomain: Some(vec!["example.com".to_string()]),
            ext: None,
            ..Default::default()
        });
    }

    // Build preview response for metadata
    let response_id = if req.id.is_empty() {
        "req".to_string()
    } else {
        req.id.clone()
    };

    let preview_response = OpenRTBResponse {
        id: response_id.clone(),
        cur: Some("USD".to_string()),
        seatbid: vec![SeatBid {
            seat: Some("mocktioneer".to_string()),
            bid: bids.clone(),
            ..Default::default()
        }],
        ..Default::default()
    };

    // Serialize response for metadata
    let sanitized_response = serde_json::to_value(&preview_response).ok();

    // Build metadata with sanitized response
    let metadata = CreativeMetadata {
        signature: signature_status,
        request: req,
        response: sanitized_response,
    };

    // Fill in adm for each bid
    let final_bids: Vec<OpenrtbBid> = bids
        .into_iter()
        .map(|mut bid| {
            let crid = bid.crid.as_deref().unwrap_or("unknown");
            let w = bid.w.unwrap_or(300);
            let h = bid.h.unwrap_or(250);
            bid.adm = Some(iframe_html(base_host, crid, w, h, None, &metadata));
            bid
        })
        .collect();

    OpenRTBResponse {
        id: response_id,
        cur: Some("USD".to_string()),
        seatbid: vec![SeatBid {
            seat: Some("mocktioneer".to_string()),
            bid: final_bids,
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
/// Example: `echo "MC4y" | base64 -d` → `0.2`
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
/// - Fixed bid price of $0.20 CPM
/// - 100% fill rate for standard sizes
/// - Returns contextual format matching real Amazon APS API
/// - No creative HTML (APS doesn't return adm field)
/// - Generates base64-encoded price strings (recoverable in mock, unlike real APS)
pub fn build_aps_response(req: &ApsBidRequest, base_host: &str) -> ApsBidResponse {
    let mut slots: Vec<ApsSlotResponse> = Vec::new();

    for slot in req.slots.iter() {
        // Find the standard size with the largest area from all sizes in the slot
        let best_size = slot
            .sizes
            .iter()
            .filter_map(|&[w, h]| {
                let w_i64 = w as i64;
                let h_i64 = h as i64;
                if is_standard_size(w_i64, h_i64) {
                    let area = w_i64 * h_i64;
                    Some((w, h, area))
                } else {
                    None
                }
            })
            .max_by_key(|&(_, _, area)| area);

        let Some((w, h, _)) = best_size else {
            // No standard sizes found, skip this slot
            log::debug!(
                "APS: Skipping slot '{}' - no standard sizes in {:?}",
                slot.slot_id,
                slot.sizes
            );
            continue;
        };

        // Generate bid components using fixed CPM pricing
        let price = FIXED_BID_CPM;
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

    fn test_signature() -> SignatureStatus {
        SignatureStatus::NotPresent {
            reason: "test".to_string(),
        }
    }

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
        let resp = build_openrtb_response(&req, "host.test", test_signature());
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
        let resp = build_openrtb_response(&req, "host.test", test_signature());
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
        let resp = build_openrtb_response(&req, "host.test", test_signature());
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
    fn test_ext_bid_override_is_ignored() {
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
        let resp = build_openrtb_response(&req, "host.test", test_signature());
        let bid = &resp.seatbid[0].bid[0];
        assert_eq!(bid.price, FIXED_BID_CPM);
        assert!(bid.ext.is_none());
        // Iframe should not include request-provided bid override
        let adm = bid.adm.as_ref().unwrap();
        assert!(!adm.contains("bid=2.50"));
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
    fn test_build_aps_response_selects_largest_area() {
        let req = ApsBidRequest {
            pub_id: "test".to_string(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_string(),
                sizes: vec![[300, 250], [970, 250]], // 970x250 has larger area (242500 vs 75000)
                slot_name: None,
            }],
            page_url: None,
            user_agent: None,
            timeout: None,
        };
        let resp = build_aps_response(&req, "mock.test");

        assert_eq!(resp.contextual.slots.len(), 1);
        let slot = &resp.contextual.slots[0];
        assert_eq!(slot.size, "970x250"); // Should pick largest area
    }

    #[test]
    fn test_build_aps_response_price_encoding_is_base64() {
        let req = ApsBidRequest {
            pub_id: "test".to_string(),
            slots: vec![ApsSlot {
                slot_id: "slot1".to_string(),
                sizes: vec![[300, 250]], // CPM is fixed at $0.20
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
        assert_eq!(price, FIXED_BID_CPM);
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
