use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use validator::{Validate, ValidationError, ValidationErrors};

// OpenRTB 2.x MarkupType for Bid.mtype (aka media/markup type)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
pub enum MediaType {
    Banner = 1,
    Video = 2,
    Audio = 3,
    Native = 4,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate)]
pub struct OpenRTBRequest {
    #[validate(length(min = 1))]
    pub id: String,
    #[validate(length(min = 1))]
    #[validate(nested)]
    pub imp: Vec<Imp>,
    // Common optional request fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmax: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bcat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badv: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bseat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wseat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wlang: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allimps: Option<i64>,
    // Contextual objects
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<Site>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app: Option<App>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<Device>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regs: Option<Regs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Imp {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Banner>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<Video>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<Audio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native: Option<Native>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pmp: Option<Pmp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instl: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloorcur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secure: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub btype: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topframe: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expdir: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i64>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate)]
pub struct Format {
    #[validate(range(min = 1))]
    pub w: i64,
    #[validate(range(min = 1))]
    pub h: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wratio: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hratio: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wmin: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hmin: Option<i64>,
}

impl Validate for Imp {
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
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cur: Option<String>,
    pub seatbid: Vec<SeatBid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customdata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nbr: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SeatBid {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seat: Option<String>,
    pub bid: Vec<Bid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Bid {
    pub id: String,
    pub impid: String,
    pub price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adid: Option<String>,
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
    pub bundle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tactic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qagmediarating: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dealid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

// ---------- Additional OpenRTB Objects ----------

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Site {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub _ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct App {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storeurl: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Publisher>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Publisher {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Content {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contentrating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livestream: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qagmediarating: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Device {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ua: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnt: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lmt: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub devicetype: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub make: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub osv: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pxratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub js: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geofetch: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ifa: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub didsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpidsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macsha1: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Geo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lon: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _type: Option<i64>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type2: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lastfix: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipservice: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct User {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buyeruid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub yob: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geo: Option<Geo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
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
    pub fd: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pchain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
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
    pub private_auction: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deals: Option<Vec<Deal>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Deal {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloor: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bidfloorcur: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wseat: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wadomain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Video {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linearity: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skip: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipmin: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipafter: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playbackmethod: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companionad: Option<Vec<Banner>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Audio {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mimes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maxduration: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocols: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startdelay: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Native {
    // In practice this can be a JSON object or a string; use Value for flexibility.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battr: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<serde_json::Value>,
}
