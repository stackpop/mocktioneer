//! Amazon Publisher Services (APS) TAM API types.
//!
//! This module provides types for the APS Transparent Ad Marketplace (TAM) API,
//! compatible with the `/e/dtb/bid` endpoint format.

use serde::{Deserialize, Serialize};
use validator::Validate;

// ============================================================================
// APS TAM API Request Types
// ============================================================================

/// APS TAM bid request format based on /e/dtb/bid endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ApsBidRequest {
    /// Publisher ID (e.g., "5555")
    #[serde(rename = "pubId")]
    pub pub_id: String,

    /// Slot configurations
    #[validate(length(min = 1))]
    pub slots: Vec<ApsSlot>,

    /// Page URL
    #[serde(rename = "pageUrl", skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,

    /// User agent
    #[serde(rename = "ua", skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,

    /// Timeout in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u32>,
}

/// APS slot configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApsSlot {
    /// Slot identifier
    #[serde(rename = "slotID")]
    pub slot_id: String,

    /// Ad sizes [[width, height], ...]
    pub sizes: Vec<[u32; 2]>,

    /// Slot name (optional)
    #[serde(rename = "slotName", skip_serializing_if = "Option::is_none")]
    pub slot_name: Option<String>,
}

// ============================================================================
// APS TAM API Response Types (Real Amazon Format)
// ============================================================================

/// APS TAM bid response format matching real Amazon API.
/// Example response from https://aax.amazon-adsystem.com/e/dtb/bid
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApsBidResponse {
    /// Contextual wrapper containing all response data
    pub contextual: ApsContextual,
}

/// APS Contextual response containing slots and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApsContextual {
    /// Array of slot responses (one per requested slot)
    #[serde(default)]
    pub slots: Vec<ApsSlotResponse>,

    /// Event tracking host
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,

    /// Response status ("ok", "error", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Client-side feature enablement flag
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfe: Option<bool>,

    /// Event tracking enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ev: Option<bool>,

    /// Client feature name (CSM script path)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cfn: Option<String>,

    /// Callback version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cb: Option<String>,

    /// Campaign tracking URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmp: Option<String>,
}

/// Individual APS slot response matching real Amazon format.
/// Note: All targeting keys are returned as flat fields on the slot object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApsSlotResponse {
    /// Slot ID this response is for
    #[serde(rename = "slotID")]
    pub slot_id: String,

    /// Creative size (e.g., "300x250")
    pub size: String,

    /// Creative ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crid: Option<String>,

    /// Media type ("d" for display, "v" for video)
    #[serde(rename = "mediaType", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,

    /// Fill indicator flag ("1" = filled, "0" = no fill)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fif: Option<String>,

    /// List of targeting key names that are set on this slot
    #[serde(default)]
    pub targeting: Vec<String>,

    /// List of metadata field names
    #[serde(default)]
    pub meta: Vec<String>,

    // ========================================================================
    // Targeting Key-Value Pairs (returned as flat fields)
    // ========================================================================
    /// Amazon impression ID (unique identifier for this bid)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amzniid: Option<String>,

    /// Amazon encoded bid price
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amznbid: Option<String>,

    /// Amazon encoded price (alternative encoding)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amznp: Option<String>,

    /// Amazon size in WxH format (e.g., "300x250")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amznsz: Option<String>,

    /// Amazon auction context type ("OPEN", "PRIVATE", etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub amznactt: Option<String>,
}
