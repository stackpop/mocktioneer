use mocktioneer_core::aps::{ApsBidRequest, ApsSlot};
use mocktioneer_core::auction::build_aps_response;

#[test]
fn test_build_aps_response_single_slot() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "header-banner".to_string(),
            sizes: vec![[300, 250]],
            slot_name: Some("header-banner".to_string()),
        }],
        page_url: Some("https://example.com".to_string()),
        user_agent: Some("Mozilla/5.0".to_string()),
        timeout: Some(800),
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Check contextual wrapper exists
    assert_eq!(resp.contextual.status, Some("ok".to_string()));
    assert_eq!(resp.contextual.slots.len(), 1);

    let slot = &resp.contextual.slots[0];
    assert_eq!(slot.slot_id, "header-banner");
    assert_eq!(slot.size, "300x250");
    assert_eq!(slot.media_type, Some("d".to_string()));
    assert_eq!(slot.fif, Some("1".to_string()));
    assert!(slot.crid.is_some());

    // Check targeting keys
    assert!(slot.amzniid.is_some());
    assert!(slot.amznbid.is_some());
    assert!(slot.amznp.is_some());
    assert_eq!(slot.amznsz, Some("300x250".to_string()));
    assert_eq!(slot.amznactt, Some("OPEN".to_string()));

    // Check targeting array lists the keys
    assert!(slot.targeting.contains(&"amzniid".to_string()));
    assert!(slot.targeting.contains(&"amznbid".to_string()));
    assert!(slot.targeting.contains(&"amznsz".to_string()));

    // Check meta array
    assert!(slot.meta.contains(&"slotID".to_string()));
    assert!(slot.meta.contains(&"mediaType".to_string()));
    assert!(slot.meta.contains(&"size".to_string()));
}

#[test]
fn test_build_aps_response_multiple_slots() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![
            ApsSlot {
                slot_id: "header".to_string(),
                sizes: vec![[728, 90]],
                slot_name: Some("header".to_string()),
            },
            ApsSlot {
                slot_id: "sidebar".to_string(),
                sizes: vec![[300, 250]],
                slot_name: Some("sidebar".to_string()),
            },
            ApsSlot {
                slot_id: "footer".to_string(),
                sizes: vec![[970, 250]],
                slot_name: Some("footer".to_string()),
            },
        ],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    assert_eq!(resp.contextual.slots.len(), 3);
    assert_eq!(resp.contextual.slots[0].slot_id, "header");
    assert_eq!(resp.contextual.slots[0].size, "728x90");
    assert_eq!(resp.contextual.slots[1].slot_id, "sidebar");
    assert_eq!(resp.contextual.slots[1].size, "300x250");
    assert_eq!(resp.contextual.slots[2].slot_id, "footer");
    assert_eq!(resp.contextual.slots[2].size, "970x250");

    // All should have encoded prices
    for slot in &resp.contextual.slots {
        assert!(slot.amznbid.is_some());
        assert!(slot.amznp.is_some());
    }
}

#[test]
fn test_build_aps_response_nonstandard_size_skipped() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "custom-slot".to_string(),
            sizes: vec![[333, 222]], // Non-standard size
            slot_name: Some("custom-slot".to_string()),
        }],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Should skip the non-standard size
    assert_eq!(resp.contextual.slots.len(), 0);
}

#[test]
fn test_build_aps_response_mixed_sizes() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![
            ApsSlot {
                slot_id: "standard".to_string(),
                sizes: vec![[300, 250]],
                slot_name: Some("standard".to_string()),
            },
            ApsSlot {
                slot_id: "nonstandard".to_string(),
                sizes: vec![[333, 222]],
                slot_name: Some("nonstandard".to_string()),
            },
        ],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Only the standard size should get a bid
    assert_eq!(resp.contextual.slots.len(), 1);
    assert_eq!(resp.contextual.slots[0].slot_id, "standard");
}

#[test]
fn test_aps_targeting_structure() {
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

    let resp = build_aps_response(&req, "test.host");

    assert_eq!(resp.contextual.slots.len(), 1);
    let slot = &resp.contextual.slots[0];

    // Check all expected targeting keys exist
    assert!(slot.amzniid.is_some());
    assert!(slot.amznbid.is_some());
    assert!(slot.amznp.is_some());
    assert!(slot.amznsz.is_some());
    assert!(slot.amznactt.is_some());

    // Check bid ID is non-empty
    let impression_id = slot.amzniid.as_ref().unwrap();
    assert!(!impression_id.is_empty());

    // Check encoded price is non-empty (real APS uses encoded strings like "pgafb4")
    let encoded_price = slot.amznbid.as_ref().unwrap();
    assert!(!encoded_price.is_empty());

    // Check size format
    assert_eq!(slot.amznsz.as_ref().unwrap(), "728x90");

    // Check auction context type
    assert_eq!(slot.amznactt.as_ref().unwrap(), "OPEN");
}

#[test]
fn test_aps_bid_multiple_sizes_per_slot() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "multi-size".to_string(),
            sizes: vec![[728, 90], [970, 250]], // Multiple sizes
            slot_name: Some("multi-size".to_string()),
        }],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Should bid on the highest CPM size (970x250 = $4.20 > 728x90 = $3.00)
    assert_eq!(resp.contextual.slots.len(), 1);
    assert_eq!(resp.contextual.slots[0].size, "970x250");
}

#[test]
fn test_aps_bid_non_standard_first_then_standard() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "mixed-slot".to_string(),
            sizes: vec![[999, 999], [300, 250]], // Non-standard first, standard second
            slot_name: Some("mixed-slot".to_string()),
        }],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Should skip non-standard size and bid on 300x250
    assert_eq!(resp.contextual.slots.len(), 1);
    assert_eq!(resp.contextual.slots[0].size, "300x250");
}

#[test]
fn test_aps_bid_selects_highest_cpm_from_multiple_standard_sizes() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "multi-standard".to_string(),
            sizes: vec![[320, 50], [300, 250], [970, 250]], // 320x50=$1.80, 300x250=$2.50, 970x250=$4.20
            slot_name: Some("multi-standard".to_string()),
        }],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Should select 970x250 with highest CPM ($4.20)
    assert_eq!(resp.contextual.slots.len(), 1);
    assert_eq!(resp.contextual.slots[0].size, "970x250");
}

#[test]
fn test_aps_bid_all_non_standard_sizes_skips_slot() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "all-non-standard".to_string(),
            sizes: vec![[999, 999], [888, 888], [777, 777]], // All non-standard
            slot_name: Some("all-non-standard".to_string()),
        }],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Should skip slot entirely when no standard sizes found
    assert_eq!(resp.contextual.slots.len(), 0);
}

#[test]
fn test_aps_response_empty_slots() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Empty slots should return empty slots array
    assert_eq!(resp.contextual.slots.len(), 0);
    assert_eq!(resp.contextual.status, Some("ok".to_string()));
}

#[test]
fn test_aps_contextual_metadata() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "test-slot".to_string(),
            sizes: vec![[300, 250]],
            slot_name: None,
        }],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Check contextual-level metadata
    let contextual = &resp.contextual;
    assert_eq!(contextual.status, Some("ok".to_string()));
    assert_eq!(contextual.cfe, Some(true));
    assert_eq!(contextual.ev, Some(true));
    assert!(contextual.host.is_some());
    assert!(contextual
        .host
        .as_ref()
        .unwrap()
        .contains("mocktioneer.test"));
    assert_eq!(contextual.cb, Some("6".to_string()));
    assert_eq!(
        contextual.cfn,
        Some("bao-csm/direct/csm_othersv6.js".to_string())
    );
}

#[test]
fn test_aps_response_no_adm_field() {
    let req = ApsBidRequest {
        pub_id: "5555".to_string(),
        slots: vec![ApsSlot {
            slot_id: "test-slot".to_string(),
            sizes: vec![[300, 250]],
            slot_name: None,
        }],
        page_url: None,
        user_agent: None,
        timeout: None,
    };

    let resp = build_aps_response(&req, "mocktioneer.test");

    // Real APS doesn't return creative HTML (adm field)
    // Creative is rendered client-side by the publisher
    assert_eq!(resp.contextual.slots.len(), 1);
    // ApsSlotResponse doesn't have an adm field
}
