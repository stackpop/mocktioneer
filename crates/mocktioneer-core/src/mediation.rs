//! Mock Ad Server Mediation
//!
//! Provides a simple mediation endpoint that accepts bids from multiple bidders
//! and selects winners based on price (highest price wins).

use crate::openrtb::{Bid as OpenRTBBid, Imp, MediaType, OpenRTBRequest, OpenRTBResponse, SeatBid};
use crate::render::{CreativeMetadata, SignatureStatus};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

fn new_id() -> String {
    Uuid::now_v7().simple().to_string()
}

/// Mediation request containing impression definitions and bidder responses
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MediationRequest {
    /// Auction ID
    #[validate(length(min = 1))]
    pub id: String,

    /// Impression definitions (from original auction request)
    #[validate(length(min = 1))]
    pub imp: Vec<Imp>,

    /// Mediation-specific extensions
    #[validate(nested)]
    pub ext: MediationExt,
}

/// Extensions for mediation request
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MediationExt {
    /// Responses from all bidders
    #[validate(length(min = 1))]
    #[validate(nested)]
    pub bidder_responses: Vec<BidderResponse>,

    /// Optional mediation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(nested)]
    pub config: Option<MediationConfig>,
}

/// Response from a single bidder
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct BidderResponse {
    /// Bidder name/identifier (e.g., "amazon-aps", "prebid")
    #[validate(length(min = 1))]
    pub bidder: String,

    /// Bids from this bidder
    #[validate(nested)]
    pub bids: Vec<MediationBid>,
}

/// A single bid from a bidder
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MediationBid {
    /// Impression ID this bid is for
    #[validate(length(min = 1))]
    pub imp_id: String,

    /// Bid price (CPM)
    #[validate(range(min = 0.0))]
    pub price: f64,

    /// Creative markup (HTML)
    /// Optional - if not provided, mediation will generate an iframe creative
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adm: Option<String>,

    /// Creative width
    #[validate(range(min = 1))]
    pub w: i64,

    /// Creative height
    #[validate(range(min = 1))]
    pub h: i64,

    /// Creative ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crid: Option<String>,

    /// Advertiser domains
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adomain: Option<Vec<String>>,
}

/// Mediation configuration
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MediationConfig {
    /// Minimum acceptable bid price (CPM)
    /// Bids below this floor will be rejected
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(range(min = 0.0))]
    pub price_floor: Option<f64>,
}

/// Run mediation algorithm and return winning bids
///
/// Algorithm:
/// 1. Collect all bids grouped by impression ID
/// 2. For each impression, select highest price bid (above floor if set)
/// 3. On price tie, first bidder in array wins
/// 4. Generate creatives for winning bids that don't have adm
/// 5. Return OpenRTB response with winning bids grouped by seat
pub fn mediate_auction(request: MediationRequest, base_host: &str) -> OpenRTBResponse {
    log::info!(
        "Mediation: processing {} impressions with {} bidder responses",
        request.imp.len(),
        request.ext.bidder_responses.len()
    );

    // Step 1: Collect all bids grouped by impression ID
    let mut bids_by_imp: HashMap<String, Vec<(String, MediationBid)>> = HashMap::new();

    for bidder_response in request.ext.bidder_responses {
        for bid in bidder_response.bids {
            bids_by_imp
                .entry(bid.imp_id.clone())
                .or_default()
                .push((bidder_response.bidder.clone(), bid));
        }
    }

    log::debug!(
        "Mediation: collected bids for {} impression(s)",
        bids_by_imp.len()
    );

    // Step 2: Select winner per impression (highest price)
    let mut winning_bids: HashMap<String, (String, MediationBid)> = HashMap::new();
    let price_floor = request
        .ext
        .config
        .and_then(|c| c.price_floor)
        .unwrap_or(0.0);

    for (imp_id, mut bids) in bids_by_imp {
        log::debug!(
            "Mediation: selecting winner for impression '{}' from {} bid(s)",
            imp_id,
            bids.len()
        );

        // Filter by price floor
        bids.retain(|(_, bid)| bid.price >= price_floor);

        if bids.is_empty() {
            log::debug!(
                "Mediation: no bids above floor (${:.2}) for impression '{}'",
                price_floor,
                imp_id
            );
            continue;
        }

        // Select highest price (first bidder wins on tie)
        // Use fold to ensure first bidder wins on price tie
        let winner = bids
            .into_iter()
            .reduce(|acc, current| {
                match current.1.price.partial_cmp(&acc.1.price) {
                    Some(Ordering::Greater) => current,
                    _ => acc, // Keep first on tie or equal
                }
            })
            .unwrap(); // Safe: we checked bids is not empty

        log::info!(
            "Mediation: '{}' wins impression '{}' at ${:.2}",
            winner.0,
            imp_id,
            winner.1.price
        );

        winning_bids.insert(imp_id, winner);
    }

    // Step 3: Build OpenRTB response grouped by seat (bidder)
    build_openrtb_response(request.id, request.imp, winning_bids, base_host)
}

/// Build OpenRTB response from winning bids
fn build_openrtb_response(
    id: String,
    imps: Vec<Imp>,
    winning_bids: HashMap<String, (String, MediationBid)>,
    base_host: &str,
) -> OpenRTBResponse {
    // Build a minimal OpenRTBRequest for metadata
    let ortb_request = OpenRTBRequest {
        id: id.clone(),
        imp: imps,
        ..Default::default()
    };

    // Create metadata with NotPresent signature status for mediation
    let metadata = CreativeMetadata {
        signature: SignatureStatus::NotPresent {
            reason: "Mediation response".to_string(),
        },
        request: &ortb_request,
        response: None,
    };

    // Group winning bids by seat/bidder
    let mut seats: HashMap<String, Vec<OpenRTBBid>> = HashMap::new();

    for (imp_id, (bidder, bid)) in winning_bids {
        // Generate creative if missing (e.g., for APS bids)
        let adm = if let Some(existing_adm) = bid.adm {
            existing_adm
        } else {
            // Generate iframe creative using same logic as OpenRTB endpoint
            let crid = bid.crid.as_deref().unwrap_or(&imp_id);
            let bid_price = Some(bid.price);
            crate::render::iframe_html(base_host, crid, bid.w, bid.h, bid_price, &metadata)
        };

        let ortb_bid = OpenRTBBid {
            id: new_id(),
            impid: imp_id,
            price: bid.price,
            adm: Some(adm),
            w: Some(bid.w),
            h: Some(bid.h),
            crid: bid.crid,
            adomain: bid.adomain,
            mtype: Some(MediaType::Banner),
            ..Default::default()
        };

        seats.entry(bidder).or_default().push(ortb_bid);
    }

    // Build seatbid array
    let seatbid: Vec<SeatBid> = seats
        .into_iter()
        .map(|(seat, bids)| SeatBid {
            seat: Some(seat),
            bid: bids,
            ..Default::default()
        })
        .collect();

    log::info!(
        "Mediation: returning {} seatbid(s) for auction '{}'",
        seatbid.len(),
        id
    );

    OpenRTBResponse {
        id,
        seatbid,
        cur: Some("USD".to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mediate_single_bidder_single_impression() {
        let request = MediationRequest {
            id: "test-auction-1".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: 2.50,
                        adm: Some("<div>Ad A</div>".to_string()),
                        w: 300,
                        h: 250,
                        crid: Some("creative-a".to_string()),
                        adomain: Some(vec!["example.com".to_string()]),
                    }],
                }],
                config: None,
            },
        };

        let response = mediate_auction(request, "test.host");

        assert_eq!(response.id, "test-auction-1");
        assert_eq!(response.seatbid.len(), 1);
        assert_eq!(response.seatbid[0].seat, Some("bidder-a".to_string()));
        assert_eq!(response.seatbid[0].bid.len(), 1);

        let bid = &response.seatbid[0].bid[0];
        assert_eq!(bid.impid, "imp1");
        assert_eq!(bid.price, 2.50);
        assert_eq!(bid.w, Some(300));
        assert_eq!(bid.h, Some(250));
    }

    #[test]
    fn test_mediate_multiple_bidders_highest_price_wins() {
        let request = MediationRequest {
            id: "test-auction-2".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![
                    BidderResponse {
                        bidder: "bidder-a".to_string(),
                        bids: vec![MediationBid {
                            imp_id: "imp1".to_string(),
                            price: 2.50,
                            adm: Some("<div>Ad A</div>".to_string()),
                            w: 300,
                            h: 250,
                            crid: None,
                            adomain: None,
                        }],
                    },
                    BidderResponse {
                        bidder: "bidder-b".to_string(),
                        bids: vec![MediationBid {
                            imp_id: "imp1".to_string(),
                            price: 3.50,
                            adm: Some("<div>Ad B</div>".to_string()),
                            w: 300,
                            h: 250,
                            crid: None,
                            adomain: None,
                        }],
                    },
                ],
                config: None,
            },
        };

        let response = mediate_auction(request, "test.host");

        assert_eq!(response.seatbid.len(), 1);
        assert_eq!(response.seatbid[0].seat, Some("bidder-b".to_string()));
        assert_eq!(response.seatbid[0].bid[0].price, 3.50);
    }

    #[test]
    fn test_mediate_price_tie_first_bidder_wins() {
        let request = MediationRequest {
            id: "test-auction-3".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![
                    BidderResponse {
                        bidder: "bidder-a".to_string(),
                        bids: vec![MediationBid {
                            imp_id: "imp1".to_string(),
                            price: 2.50,
                            adm: Some("<div>Ad A</div>".to_string()),
                            w: 300,
                            h: 250,
                            crid: None,
                            adomain: None,
                        }],
                    },
                    BidderResponse {
                        bidder: "bidder-b".to_string(),
                        bids: vec![MediationBid {
                            imp_id: "imp1".to_string(),
                            price: 2.50,
                            adm: Some("<div>Ad B</div>".to_string()),
                            w: 300,
                            h: 250,
                            crid: None,
                            adomain: None,
                        }],
                    },
                ],
                config: None,
            },
        };

        let response = mediate_auction(request, "test.host");

        // First bidder should win on tie
        assert_eq!(response.seatbid.len(), 1);
        assert_eq!(response.seatbid[0].seat, Some("bidder-a".to_string()));
    }

    #[test]
    fn test_mediate_with_price_floor() {
        let request = MediationRequest {
            id: "test-auction-4".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![
                    BidderResponse {
                        bidder: "bidder-a".to_string(),
                        bids: vec![MediationBid {
                            imp_id: "imp1".to_string(),
                            price: 0.50, // Below floor
                            adm: Some("<div>Ad A</div>".to_string()),
                            w: 300,
                            h: 250,
                            crid: None,
                            adomain: None,
                        }],
                    },
                    BidderResponse {
                        bidder: "bidder-b".to_string(),
                        bids: vec![MediationBid {
                            imp_id: "imp1".to_string(),
                            price: 2.00, // Above floor
                            adm: Some("<div>Ad B</div>".to_string()),
                            w: 300,
                            h: 250,
                            crid: None,
                            adomain: None,
                        }],
                    },
                ],
                config: Some(MediationConfig {
                    price_floor: Some(1.00),
                }),
            },
        };

        let response = mediate_auction(request, "test.host");

        // Only bidder-b should win (above floor)
        assert_eq!(response.seatbid.len(), 1);
        assert_eq!(response.seatbid[0].seat, Some("bidder-b".to_string()));
        assert_eq!(response.seatbid[0].bid[0].price, 2.00);
    }

    #[test]
    fn test_mediate_all_bids_below_floor() {
        let request = MediationRequest {
            id: "test-auction-5".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: 0.50,
                        adm: Some("<div>Ad A</div>".to_string()),
                        w: 300,
                        h: 250,
                        crid: None,
                        adomain: None,
                    }],
                }],
                config: Some(MediationConfig {
                    price_floor: Some(1.00),
                }),
            },
        };

        let response = mediate_auction(request, "test.host");

        // No winners (all below floor)
        assert_eq!(response.seatbid.len(), 0);
    }

    #[test]
    fn test_mediate_multiple_impressions() {
        let request = MediationRequest {
            id: "test-auction-6".to_string(),
            imp: vec![
                Imp {
                    id: "imp1".to_string(),
                    ..Default::default()
                },
                Imp {
                    id: "imp2".to_string(),
                    ..Default::default()
                },
            ],
            ext: MediationExt {
                bidder_responses: vec![
                    BidderResponse {
                        bidder: "bidder-a".to_string(),
                        bids: vec![
                            MediationBid {
                                imp_id: "imp1".to_string(),
                                price: 2.50,
                                adm: Some("<div>Ad A1</div>".to_string()),
                                w: 300,
                                h: 250,
                                crid: None,
                                adomain: None,
                            },
                            MediationBid {
                                imp_id: "imp2".to_string(),
                                price: 3.00,
                                adm: Some("<div>Ad A2</div>".to_string()),
                                w: 728,
                                h: 90,
                                crid: None,
                                adomain: None,
                            },
                        ],
                    },
                    BidderResponse {
                        bidder: "bidder-b".to_string(),
                        bids: vec![
                            MediationBid {
                                imp_id: "imp1".to_string(),
                                price: 3.50, // Higher for imp1
                                adm: Some("<div>Ad B1</div>".to_string()),
                                w: 300,
                                h: 250,
                                crid: None,
                                adomain: None,
                            },
                            MediationBid {
                                imp_id: "imp2".to_string(),
                                price: 2.00, // Lower for imp2
                                adm: Some("<div>Ad B2</div>".to_string()),
                                w: 728,
                                h: 90,
                                crid: None,
                                adomain: None,
                            },
                        ],
                    },
                ],
                config: None,
            },
        };

        let response = mediate_auction(request, "test.host");

        // Both bidders should have winning bids (different impressions)
        assert_eq!(response.seatbid.len(), 2);

        // Find bidder-b's seatbid (should have imp1)
        let bidder_b_seat = response
            .seatbid
            .iter()
            .find(|s| s.seat == Some("bidder-b".to_string()))
            .unwrap();
        assert_eq!(bidder_b_seat.bid.len(), 1);
        assert_eq!(bidder_b_seat.bid[0].impid, "imp1");
        assert_eq!(bidder_b_seat.bid[0].price, 3.50);

        // Find bidder-a's seatbid (should have imp2)
        let bidder_a_seat = response
            .seatbid
            .iter()
            .find(|s| s.seat == Some("bidder-a".to_string()))
            .unwrap();
        assert_eq!(bidder_a_seat.bid.len(), 1);
        assert_eq!(bidder_a_seat.bid[0].impid, "imp2");
        assert_eq!(bidder_a_seat.bid[0].price, 3.00);
    }

    #[test]
    fn test_mediate_no_bidder_responses() {
        let request = MediationRequest {
            id: "test-auction-7".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![],
                config: None,
            },
        };

        let response = mediate_auction(request, "test.host");

        // No bids
        assert_eq!(response.seatbid.len(), 0);
    }

    #[test]
    fn test_mediate_missing_adm_generates_creative() {
        // Test APS-style bid without creative markup
        let request = MediationRequest {
            id: "test-auction-8".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "amazon-aps".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: 3.00,
                        adm: None, // No creative provided (like APS)
                        w: 300,
                        h: 250,
                        crid: Some("aps-creative-123".to_string()),
                        adomain: None,
                    }],
                }],
                config: None,
            },
        };

        let response = mediate_auction(request, "mocktioneer.test");

        // Should have one winning bid
        assert_eq!(response.seatbid.len(), 1);
        assert_eq!(response.seatbid[0].seat, Some("amazon-aps".to_string()));
        assert_eq!(response.seatbid[0].bid.len(), 1);

        let bid = &response.seatbid[0].bid[0];
        assert_eq!(bid.impid, "imp1");
        assert_eq!(bid.price, 3.00);
        assert_eq!(bid.w, Some(300));
        assert_eq!(bid.h, Some(250));

        // Should have generated creative
        assert!(bid.adm.is_some());
        let adm = bid.adm.as_ref().unwrap();

        // Check that generated creative is an iframe
        assert!(adm.contains("<iframe"));
        assert!(adm.contains("//mocktioneer.test/static/creatives/300x250.html"));
        assert!(adm.contains("crid=aps-creative-123"));
        assert!(adm.contains("bid=3.00"));
    }

    #[test]
    fn test_mediate_mixed_bids_with_and_without_adm() {
        // Test mediation with both traditional bids (with adm) and APS-style bids (without adm)
        let request = MediationRequest {
            id: "test-auction-9".to_string(),
            imp: vec![
                Imp {
                    id: "imp1".to_string(),
                    ..Default::default()
                },
                Imp {
                    id: "imp2".to_string(),
                    ..Default::default()
                },
            ],
            ext: MediationExt {
                bidder_responses: vec![
                    BidderResponse {
                        bidder: "amazon-aps".to_string(),
                        bids: vec![
                            MediationBid {
                                imp_id: "imp1".to_string(),
                                price: 3.50, // APS wins imp1
                                adm: None,   // No creative
                                w: 300,
                                h: 250,
                                crid: Some("aps-1".to_string()),
                                adomain: None,
                            },
                            MediationBid {
                                imp_id: "imp2".to_string(),
                                price: 2.00, // APS loses imp2
                                adm: None,
                                w: 728,
                                h: 90,
                                crid: Some("aps-2".to_string()),
                                adomain: None,
                            },
                        ],
                    },
                    BidderResponse {
                        bidder: "prebid".to_string(),
                        bids: vec![
                            MediationBid {
                                imp_id: "imp1".to_string(),
                                price: 2.50, // Prebid loses imp1
                                adm: Some("<div>Prebid Ad 1</div>".to_string()),
                                w: 300,
                                h: 250,
                                crid: None,
                                adomain: None,
                            },
                            MediationBid {
                                imp_id: "imp2".to_string(),
                                price: 3.00, // Prebid wins imp2
                                adm: Some("<div>Prebid Ad 2</div>".to_string()),
                                w: 728,
                                h: 90,
                                crid: None,
                                adomain: None,
                            },
                        ],
                    },
                ],
                config: None,
            },
        };

        let response = mediate_auction(request, "test.example.com");

        // Both bidders should have winning bids
        assert_eq!(response.seatbid.len(), 2);

        // Find APS seat (should win imp1)
        let aps_seat = response
            .seatbid
            .iter()
            .find(|s| s.seat == Some("amazon-aps".to_string()))
            .unwrap();
        assert_eq!(aps_seat.bid.len(), 1);
        assert_eq!(aps_seat.bid[0].impid, "imp1");
        assert_eq!(aps_seat.bid[0].price, 3.50);

        // APS bid should have generated creative
        let aps_adm = aps_seat.bid[0].adm.as_ref().unwrap();
        assert!(aps_adm.contains("<iframe"));
        assert!(aps_adm.contains("//test.example.com/static/creatives/300x250.html"));
        assert!(aps_adm.contains("crid=aps-1"));

        // Find Prebid seat (should win imp2)
        let prebid_seat = response
            .seatbid
            .iter()
            .find(|s| s.seat == Some("prebid".to_string()))
            .unwrap();
        assert_eq!(prebid_seat.bid.len(), 1);
        assert_eq!(prebid_seat.bid[0].impid, "imp2");
        assert_eq!(prebid_seat.bid[0].price, 3.00);

        // Prebid bid should have original creative
        let prebid_adm = prebid_seat.bid[0].adm.as_ref().unwrap();
        assert_eq!(prebid_adm, "<div>Prebid Ad 2</div>");
    }

    #[test]
    fn test_validation_empty_auction_id() {
        let request = MediationRequest {
            id: "".to_string(), // Empty ID should fail
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: 2.50,
                        adm: Some("<div>Ad</div>".to_string()),
                        w: 300,
                        h: 250,
                        crid: None,
                        adomain: None,
                    }],
                }],
                config: None,
            },
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_validation_empty_impressions() {
        let request = MediationRequest {
            id: "test-auction".to_string(),
            imp: vec![], // Empty impressions should fail
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![],
                }],
                config: None,
            },
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_validation_empty_bidder_responses() {
        let request = MediationRequest {
            id: "test-auction".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![], // Empty bidder responses should fail
                config: None,
            },
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_validation_negative_price() {
        let request = MediationRequest {
            id: "test-auction".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: -1.0, // Negative price should fail
                        adm: Some("<div>Ad</div>".to_string()),
                        w: 300,
                        h: 250,
                        crid: None,
                        adomain: None,
                    }],
                }],
                config: None,
            },
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_validation_negative_price_floor() {
        let request = MediationRequest {
            id: "test-auction".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: 2.50,
                        adm: Some("<div>Ad</div>".to_string()),
                        w: 300,
                        h: 250,
                        crid: None,
                        adomain: None,
                    }],
                }],
                config: Some(MediationConfig {
                    price_floor: Some(-1.0), // Negative floor should fail
                }),
            },
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_dimensions() {
        let request = MediationRequest {
            id: "test-auction".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: 2.50,
                        adm: Some("<div>Ad</div>".to_string()),
                        w: 0, // Zero width should fail
                        h: 250,
                        crid: None,
                        adomain: None,
                    }],
                }],
                config: None,
            },
        };

        assert!(request.validate().is_err());
    }

    #[test]
    fn test_validation_valid_request() {
        let request = MediationRequest {
            id: "test-auction".to_string(),
            imp: vec![Imp {
                id: "imp1".to_string(),
                ..Default::default()
            }],
            ext: MediationExt {
                bidder_responses: vec![BidderResponse {
                    bidder: "bidder-a".to_string(),
                    bids: vec![MediationBid {
                        imp_id: "imp1".to_string(),
                        price: 2.50,
                        adm: Some("<div>Ad</div>".to_string()),
                        w: 300,
                        h: 250,
                        crid: None,
                        adomain: None,
                    }],
                }],
                config: Some(MediationConfig {
                    price_floor: Some(1.0),
                }),
            },
        };

        assert!(request.validate().is_ok());
    }
}
