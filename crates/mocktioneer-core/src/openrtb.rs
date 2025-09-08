use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum MediaType {
    Banner = 1,
    Video = 2,
    Native = 4,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OpenRTBRequest {
    pub id: String,
    pub imp: Vec<Imp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<Vec<String>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Imp {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Banner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ImpExt>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImpExt {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mocktioneer: Option<ExtMocktioneer>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ExtMocktioneer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bid: Option<f64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Banner {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Vec<Format>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Format {
    pub w: i64,
    pub h: i64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OpenRTBResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<String>,
    pub seatbid: Vec<SeatBid>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SeatBid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    pub bid: Vec<Bid>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Bid {
    pub id: String,
    pub impid: String,
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtype: Option<MediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adomain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}
