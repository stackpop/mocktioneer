use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use validator::{Validate, ValidationError, ValidationErrors};

// OpenRTB 2.x MarkupType for Bid.mtype (aka media/markup type)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum MediaType {
    Audio = 3,
    Banner = 1,
    Native = 4,
    Video = 2,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate)]
pub struct OpenRTBRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allimps: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<App>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badv: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bseat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<Device>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[validate(length(min = 1_u64))]
    pub id: String,
    #[validate(length(min = 1_u64))]
    #[validate(nested)]
    pub imp: Vec<Imp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regs: Option<Regs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<Site>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmax: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wlang: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wseat: Option<Vec<String>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Imp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<Audio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Banner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloorcur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<ImpExt>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instl: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<Native>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pmp: Option<Pmp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<Video>,
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
    pub api: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btype: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expdir: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Vec<Format>>,
    #[serde(rename = "h", skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topframe: Option<i64>,
    #[serde(rename = "w", skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate)]
pub struct Format {
    #[serde(rename = "h")]
    #[validate(range(min = 1_i64))]
    pub height: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hmin: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hratio: Option<i64>,
    #[serde(rename = "w")]
    #[validate(range(min = 1_i64))]
    pub width: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmin: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wratio: Option<i64>,
}

impl Validate for Imp {
    #[inline]
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();

        if self.id.trim().is_empty() {
            let mut error = ValidationError::new("required");
            error.message = Some("imp.id must be non-empty".into());
            errors.add("id", error);
        }

        let has_media = self.banner.is_some()
            || self.video.is_some()
            || self.audio.is_some()
            || self.native.is_some();
        if !has_media {
            let mut error = ValidationError::new("missing_media");
            error.message = Some(
                "imp requires at least one creative object (banner/video/audio/native)".into(),
            );
            errors.add("media", error);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OpenRTBResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customdata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbr: Option<i64>,
    pub seatbid: Vec<SeatBid>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SeatBid {
    pub bid: Vec<Bid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Bid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adomain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(rename = "h", skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    pub id: String,
    pub impid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtype: Option<MediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nurl: Option<String>,
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qagmediarating: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tactic: Option<String>,
    #[serde(rename = "w", skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
}

// ---------- Additional OpenRTB Objects ----------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Site {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    /// `OpenRTB` spec `Site.ref` — the referrer URL. Raw identifier `r#ref`
    /// serializes to the JSON key `ref`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#ref: Option<String>,
    /// Non-spec compatibility field: captures a literal `ref_` JSON key
    /// emitted by some legacy callers. Distinct from `r#ref` (the spec
    /// `ref`); retained so such payloads round-trip without data loss.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct App {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storeurl: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Publisher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contentrating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livestream: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qagmediarating: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Device {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devicetype: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub didsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnt: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpidsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geofetch: Option<i64>,
    #[serde(rename = "h", skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ifa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lmt: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osv: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pxratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ua: Option<String>,
    #[serde(rename = "w", skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Geo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipservice: Option<i64>,
    /// Non-spec compatibility field: captures a literal `_type` JSON key
    /// emitted by some legacy callers. The `OpenRTB` spec `Geo.type`
    /// (location source) is carried by [`Geo::type2`] instead.
    #[serde(rename = "_type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lastfix: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// `OpenRTB` spec `Geo.type` — source of the location data
    /// (1 = GPS, 2 = IP, 3 = user-provided). Named `type2` because `type`
    /// is a Rust keyword; serializes to the JSON key `type`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type2: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyeruid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yob: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Regs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coppa: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fd: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pchain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Metric {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Pmp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deals: Option<Vec<Deal>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_auction: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Deal {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloorcur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wadomain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wseat: Option<Vec<String>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Video {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companionad: Option<Vec<Banner>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(rename = "h", skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linearity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playbackmethod: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipafter: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipmin: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<i64>,
    #[serde(rename = "w", skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Audio {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<i64>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Native {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
    // In practice this can be a JSON object or a string; use Value for flexibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(value: &impl Serialize) -> serde_json::Value {
        serde_json::to_value(value).expect("serialize")
    }

    #[test]
    fn banner_round_trip_uses_w_h_json_keys() {
        let banner = Banner {
            height: Some(250_i64),
            width: Some(300_i64),
            ..Default::default()
        };
        let value = json(&banner);
        assert_eq!(value["w"], 300_i32);
        assert_eq!(value["h"], 250_i32);
        assert!(value.get("width").is_none());
        assert!(value.get("height").is_none());

        let parsed: Banner =
            serde_json::from_value(serde_json::json!({"w": 300_i32, "h": 250_i32}))
                .expect("deserialize");
        assert_eq!(parsed.width, Some(300_i64));
        assert_eq!(parsed.height, Some(250_i64));
    }

    #[test]
    fn format_round_trip_uses_w_h_json_keys() {
        let format = Format {
            height: 90_i64,
            width: 728_i64,
            ..Default::default()
        };
        let value = json(&format);
        assert_eq!(value["w"], 728_i32);
        assert_eq!(value["h"], 90_i32);

        let parsed: Format = serde_json::from_value(serde_json::json!({"w": 728_i32, "h": 90_i32}))
            .expect("deserialize");
        assert_eq!(parsed.width, 728_i64);
        assert_eq!(parsed.height, 90_i64);
    }

    #[test]
    fn bid_round_trip_uses_w_h_json_keys() {
        let bid = Bid {
            id: "b1".to_owned(),
            impid: "i1".to_owned(),
            price: 1.0_f64,
            height: Some(600_i64),
            width: Some(160_i64),
            ..Default::default()
        };
        let value = json(&bid);
        assert_eq!(value["w"], 160_i32);
        assert_eq!(value["h"], 600_i32);

        let parsed: Bid = serde_json::from_value(serde_json::json!({
            "id": "b1", "impid": "i1", "price": 1.0_f64, "w": 160_i32, "h": 600_i32
        }))
        .expect("deserialize");
        assert_eq!(parsed.width, Some(160_i64));
        assert_eq!(parsed.height, Some(600_i64));
    }

    #[test]
    fn device_round_trip_uses_w_h_json_keys() {
        let device = Device {
            height: Some(800_i64),
            width: Some(1200_i64),
            ..Default::default()
        };
        let value = json(&device);
        assert_eq!(value["w"], 1200_i32);
        assert_eq!(value["h"], 800_i32);
    }

    #[test]
    fn video_round_trip_uses_w_h_json_keys() {
        let video = Video {
            height: Some(480_i64),
            width: Some(640_i64),
            ..Default::default()
        };
        let value = json(&video);
        assert_eq!(value["w"], 640_i32);
        assert_eq!(value["h"], 480_i32);
    }

    #[test]
    fn site_round_trip_maps_r_ref_to_ref_json_key() {
        let site = Site {
            r#ref: Some("https://referrer.example".to_owned()),
            ref_: Some("legacy".to_owned()),
            ..Default::default()
        };
        let value = json(&site);
        assert_eq!(value["ref"], "https://referrer.example");
        assert_eq!(value["ref_"], "legacy");

        let parsed: Site = serde_json::from_value(serde_json::json!({
            "ref": "https://referrer.example",
            "ref_": "legacy"
        }))
        .expect("deserialize");
        assert_eq!(parsed.r#ref.as_deref(), Some("https://referrer.example"));
        assert_eq!(parsed.ref_.as_deref(), Some("legacy"));
    }

    #[test]
    fn geo_round_trip_maps_kind_to_underscore_type_json_key() {
        let geo = Geo {
            kind: Some(1_i64),
            type2: Some(2_i64),
            ..Default::default()
        };
        let value = json(&geo);
        assert_eq!(value["_type"], 1_i32);
        assert_eq!(value["type"], 2_i32);
        assert!(value.get("kind").is_none());
        assert!(value.get("type2").is_none());

        let parsed: Geo =
            serde_json::from_value(serde_json::json!({"_type": 1_i32, "type": 2_i32}))
                .expect("deserialize");
        assert_eq!(parsed.kind, Some(1_i64));
        assert_eq!(parsed.type2, Some(2_i64));
    }
}
