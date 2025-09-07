use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaType {
    Banner = 1,
    Video = 2,
    Native = 4,
}

impl Serialize for MediaType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i32(*self as i32)
    }
}

impl<'de> Deserialize<'de> for MediaType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = i64::deserialize(deserializer)?;
        match v {
            1 => Ok(MediaType::Banner),
            2 => Ok(MediaType::Video),
            4 => Ok(MediaType::Native),
            other => Err(de::Error::custom(format!("invalid mtype: {}", other))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenRTBRequest {
    pub id: String,
    #[serde(default)]
    pub imp: Vec<Imp>,
    #[serde(default)]
    pub cur: Option<Vec<String>>,
    #[serde(default)]
    pub test: Option<i32>,
    #[serde(default)]
    pub tmax: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Imp {
    pub id: String,
    #[serde(default)]
    pub banner: Option<Banner>,
    #[serde(default)]
    pub secure: Option<i32>,
    #[serde(default)]
    pub ext: Option<ImpExt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImpExt {
    #[serde(default)]
    pub mocktioneer: Option<ExtMocktioneer>,
    #[serde(flatten, default)]
    pub other: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtMocktioneer {
    #[serde(default)]
    pub bid: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Banner {
    #[serde(default)]
    pub w: Option<i64>,
    #[serde(default)]
    pub h: Option<i64>,
    #[serde(default)]
    pub format: Option<Vec<Format>>, // ORTB 2.x banner.format
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Format {
    pub w: i64,
    pub h: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenRTBResponse {
    pub id: String,
    #[serde(default)]
    pub cur: Option<String>,
    #[serde(default)]
    pub seatbid: Vec<SeatBid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeatBid {
    #[serde(default)]
    pub seat: Option<String>,
    #[serde(default)]
    pub bid: Vec<Bid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Bid {
    pub id: String,
    pub impid: String,
    pub price: f64,
    #[serde(default)]
    pub adm: Option<String>,
    #[serde(default)]
    pub crid: Option<String>,
    #[serde(default)]
    pub w: Option<i64>,
    #[serde(default)]
    pub h: Option<i64>,
    #[serde(default)]
    pub adomain: Option<Vec<String>>,
    #[serde(default)]
    pub mtype: Option<MediaType>, // ORTB 2.6: 1=banner, 2=video, 4=native
    #[serde(default)]
    pub burl: Option<String>,
    #[serde(default)]
    pub exp: Option<i32>,
    #[serde(default)]
    pub ext: Option<serde_json::Value>,
}
