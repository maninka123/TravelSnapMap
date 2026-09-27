//! Domain enums and records shared by the database, pipeline and UI.
//! Enums are stored as strings, so new cases never need a migration and unknown
//! values (e.g. from a newer app version) degrade to a default.

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

macro_rules! string_enum {
    ($name:ident, default = $def:ident, { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            pub fn as_str(&self) -> &'static str { match self { $($name::$variant => $s),+ } }
            pub fn parse(s: &str) -> Self { match s { $($s => $name::$variant,)+ _ => $name::$def } }
            pub fn try_parse(s: &str) -> Option<Self> { match s { $($s => Some($name::$variant),)+ _ => None } }
        }
        impl Default for $name { fn default() -> Self { $name::$def } }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.as_str()) }
        }
        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { s.serialize_str(self.as_str()) }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Ok($name::parse(&String::deserialize(d)?))
            }
        }
        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> { Ok(ToSqlOutput::from(self.as_str())) }
        }
        impl FromSql for $name {
            fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> { v.as_str().map($name::parse) }
        }
    };
}

string_enum!(PlaceCategory, default = Other, {
    Attraction => "attraction", Restaurant => "restaurant", Cafe => "cafe", Hotel => "hotel",
    Accommodation => "accommodation", Viewpoint => "viewpoint", Nature => "nature", Beach => "beach",
    Hiking => "hiking", Temple => "temple", ReligiousSite => "religiousSite", Museum => "museum",
    HistoricSite => "historicSite", Shopping => "shopping", Activity => "activity", Nightlife => "nightlife",
    Transport => "transport", Airport => "airport", Station => "station", Food => "food",
    City => "city", Region => "region", Other => "other",
});

impl PlaceCategory {
    /// Lenient mapping from AI output or MapKit POI categories. Unknown text → None.
    pub fn fuzzy(text: Option<&str>) -> Option<Self> {
        let raw = text?.trim();
        if raw.is_empty() {
            return None;
        }
        let compact = raw.replace(['_', ' ', '-'], "").to_lowercase();
        if let Some(exact) = Self::ALL.iter().find(|c| c.as_str().to_lowercase() == compact) {
            return Some(*exact);
        }
        let t = raw.to_lowercase();
        const TABLE: &[(&str, PlaceCategory)] = &[
            ("restaurant", PlaceCategory::Restaurant), ("cafe", PlaceCategory::Cafe), ("café", PlaceCategory::Cafe),
            ("coffee", PlaceCategory::Cafe), ("bakery", PlaceCategory::Food), ("nightlife", PlaceCategory::Nightlife),
            ("brewery", PlaceCategory::Nightlife), ("winery", PlaceCategory::Nightlife), ("bar", PlaceCategory::Nightlife),
            ("food", PlaceCategory::Food), ("market", PlaceCategory::Food), ("hotel", PlaceCategory::Hotel),
            ("hostel", PlaceCategory::Accommodation), ("ryokan", PlaceCategory::Hotel), ("resort", PlaceCategory::Hotel),
            ("campground", PlaceCategory::Accommodation), ("museum", PlaceCategory::Museum), ("gallery", PlaceCategory::Museum),
            ("temple", PlaceCategory::Temple), ("shrine", PlaceCategory::Temple), ("church", PlaceCategory::ReligiousSite),
            ("cathedral", PlaceCategory::ReligiousSite), ("mosque", PlaceCategory::ReligiousSite),
            ("castle", PlaceCategory::HistoricSite), ("historic", PlaceCategory::HistoricSite),
            ("landmark", PlaceCategory::Attraction), ("beach", PlaceCategory::Beach), ("hik", PlaceCategory::Hiking),
            ("trail", PlaceCategory::Hiking), ("park", PlaceCategory::Nature), ("lake", PlaceCategory::Nature),
            ("mountain", PlaceCategory::Nature), ("nature", PlaceCategory::Nature), ("forest", PlaceCategory::Nature),
            ("waterfall", PlaceCategory::Nature), ("garden", PlaceCategory::Nature), ("viewpoint", PlaceCategory::Viewpoint),
            ("observation", PlaceCategory::Viewpoint), ("lookout", PlaceCategory::Viewpoint), ("shop", PlaceCategory::Shopping),
            ("store", PlaceCategory::Shopping), ("mall", PlaceCategory::Shopping), ("airport", PlaceCategory::Airport),
            ("station", PlaceCategory::Station), ("transit", PlaceCategory::Transport), ("ferry", PlaceCategory::Transport),
            ("city", PlaceCategory::City), ("town", PlaceCategory::City), ("village", PlaceCategory::City),
            ("region", PlaceCategory::Region), ("island", PlaceCategory::Region), ("activity", PlaceCategory::Activity),
            ("tour", PlaceCategory::Activity), ("amusement", PlaceCategory::Activity), ("zoo", PlaceCategory::Activity),
            ("aquarium", PlaceCategory::Activity), ("stadium", PlaceCategory::Activity), ("theater", PlaceCategory::Activity),
            ("attraction", PlaceCategory::Attraction),
        ];
        TABLE.iter().find(|(k, _)| t.contains(k)).map(|(_, c)| *c)
    }

    /// Broad group for category-compatibility scoring during place resolution.
    pub fn group(&self) -> &'static str {
        use PlaceCategory::*;
        match self {
            Restaurant | Cafe | Food | Nightlife => "food",
            Hotel | Accommodation => "stay",
            Transport | Airport | Station => "transport",
            City | Region => "area",
            Other => "any",
            _ => "sight",
        }
    }
}

string_enum!(PersonalStatus, default = WantToVisit, {
    WantToVisit => "wantToVisit", Maybe => "maybe", Visited => "visited",
    Favourite => "favourite", NotInterested => "notInterested",
});

string_enum!(TravelFactType, default = Other, {
    GeneralTip => "generalTip", RecommendedTime => "recommendedTime", BestSeason => "bestSeason",
    OpeningHours => "openingHours", Price => "price", Reservation => "reservation",
    Transportation => "transportation", Duration => "duration", Photography => "photography",
    Food => "food", Warning => "warning", Accessibility => "accessibility",
    Accommodation => "accommodation", Activity => "activity", Route => "route", Ticket => "ticket",
    Nearby => "nearby", Itinerary => "itinerary", Other => "other",
});

impl TravelFactType {
    /// Accepts camelCase, snake_case and common synonyms; unknown → Other.
    pub fn fuzzy(text: Option<&str>) -> Self {
        let compact = text.unwrap_or("").replace(['_', ' ', '-'], "").to_lowercase();
        if let Some(t) = Self::ALL.iter().find(|t| t.as_str().to_lowercase() == compact) {
            return *t;
        }
        match compact.as_str() {
            "tip" | "tips" => Self::GeneralTip,
            "besttime" | "timing" | "time" => Self::RecommendedTime,
            "season" => Self::BestSeason,
            "hours" => Self::OpeningHours,
            "transport" | "gettingthere" => Self::Transportation,
            "booking" => Self::Reservation,
            "photo" | "photospot" => Self::Photography,
            "cost" | "fee" | "entryfee" => Self::Price,
            "nearbyplaces" | "nearbyplace" => Self::Nearby,
            "crowds" | "crowdadvice" => Self::RecommendedTime,
            "localtip" | "localtips" => Self::GeneralTip,
            _ => Self::Other,
        }
    }

    /// Values that change over time: the UI shows latest vs earlier saved values.
    pub fn is_time_sensitive(&self) -> bool {
        matches!(self, Self::Price | Self::OpeningHours | Self::Ticket | Self::Reservation)
    }
}

string_enum!(SourceType, default = Unknown, {
    Instagram => "instagram", Tiktok => "tiktok", Youtube => "youtube", GoogleMaps => "googleMaps",
    AppleMaps => "appleMaps", Booking => "booking", Tripadvisor => "tripadvisor", Airbnb => "airbnb",
    Reddit => "reddit", Safari => "safari", Facebook => "facebook", Pinterest => "pinterest",
    Xiaohongshu => "xiaohongshu", Other => "other", Unknown => "unknown",
});

impl SourceType {
    pub fn fuzzy(text: Option<&str>) -> Self {
        let compact = text.unwrap_or("").replace(['_', ' ', '-', '.'], "").to_lowercase();
        if let Some(s) = Self::ALL.iter().find(|s| s.as_str().to_lowercase() == compact) {
            return *s;
        }
        match compact.as_str() {
            "web" | "blog" | "browser" | "website" => Self::Safari,
            "maps" => Self::GoogleMaps,
            "bookingcom" => Self::Booking,
            "rednote" => Self::Xiaohongshu,
            _ => Self::Unknown,
        }
    }

    pub fn is_map_app(&self) -> bool {
        matches!(self, Self::GoogleMaps | Self::AppleMaps)
    }
}

string_enum!(ProcessingStatus, default = Discovered, {
    Discovered => "discovered", Loading => "loading", OcrProcessing => "ocrProcessing",
    OcrComplete => "ocrComplete", Classifying => "classifying", Extracting => "extracting",
    ResolvingPlaces => "resolvingPlaces", ExtractingImages => "extractingImages", Complete => "complete",
    NotTravel => "notTravel", NeedsReview => "needsReview", WaitingForNetwork => "waitingForNetwork", NeedsMedia => "needsMedia",
    Ignored => "ignored", Failed => "failed",
});

impl ProcessingStatus {
    /// No further automatic work is needed.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Complete | Self::NotTravel | Self::NeedsReview | Self::Ignored | Self::NeedsMedia)
    }

    /// Picked up by the queue: new, interrupted mid-way (app quit), offline, or failed.
    pub fn needs_processing(&self) -> bool {
        !self.is_terminal()
    }

    /// OCR output is persisted, so anything interrupted after OCR resumes without re-running it.
    pub fn can_reuse_ocr(&self) -> bool {
        matches!(
            self,
            Self::OcrComplete | Self::Classifying | Self::Extracting | Self::ResolvingPlaces
                | Self::ExtractingImages | Self::WaitingForNetwork
        )
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Loading | Self::OcrProcessing | Self::Classifying | Self::Extracting
                | Self::ResolvingPlaces | Self::ExtractingImages
        )
    }
}

string_enum!(Classification, default = Unknown, {
    Unknown => "unknown", Travel => "travel", NotTravel => "notTravel", Uncertain => "uncertain",
});

string_enum!(RegionType, default = Unknown, {
    Photograph => "photograph", Map => "map", Interface => "interface", Text => "text",
    Illustration => "illustration", Unknown => "unknown",
});

string_enum!(DataOrigin, default = Ai, {
    Ai => "ai", MapKit => "mapKit", Local => "local", User => "user",
    // Added by hand with "+ Add Place": has no screenshots or Reels on purpose, so it's never auto-removed.
    Manual => "manual",
});

string_enum!(Verification, default = Verified, {
    Verified => "verified", NeedsReview => "needsReview", UserVerified => "userVerified",
});

string_enum!(ReviewKind, default = PlaceResolution, {
    TravelClassification => "travelClassification", PlaceResolution => "placeResolution",
    DuplicatePlace => "duplicatePlace", PhotoCrop => "photoCrop", ProcessingFailure => "processingFailure",
});

/// Normalised rectangle, top-left origin (0–1).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }
    pub fn area(&self) -> f64 {
        self.width.max(0.0) * self.height.max(0.0)
    }
    pub fn max_x(&self) -> f64 {
        self.x + self.width
    }
    pub fn max_y(&self) -> f64 {
        self.y + self.height
    }
    pub fn intersection(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let w = self.max_x().min(o.max_x()) - x;
        let h = self.max_y().min(o.max_y()) - y;
        if w <= 0.0 || h <= 0.0 { Rect::default() } else { Rect::new(x, y, w, h) }
    }
    pub fn clamped(&self) -> Rect {
        let x = self.x.clamp(0.0, 1.0);
        let y = self.y.clamp(0.0, 1.0);
        Rect::new(x, y, self.width.min(1.0 - x).max(0.0), self.height.min(1.0 - y).max(0.0))
    }
}

/// A verified location from the place search provider (never from the AI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlaceCandidate {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub country_code: Option<String>,
    #[serde(default)]
    pub map_identifier: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    /// Distance from the centre of the searched area (city/country hint), when one was given.
    #[serde(default)]
    pub near_distance_km: Option<f64>,
    #[serde(default)]
    pub score: f64,
}

impl PlaceCandidate {
    pub fn category_hint(&self) -> Option<PlaceCategory> {
        PlaceCategory::fuzzy(self.category.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_round_trip_and_default() {
        assert_eq!(PlaceCategory::parse("religiousSite"), PlaceCategory::ReligiousSite);
        assert_eq!(PlaceCategory::parse("spaceport"), PlaceCategory::Other);
        assert_eq!(serde_json::to_string(&TravelFactType::RecommendedTime).unwrap(), "\"recommendedTime\"");
    }

    #[test]
    fn fuzzy_parsing() {
        assert_eq!(PlaceCategory::fuzzy(Some("religious_site")), Some(PlaceCategory::ReligiousSite));
        assert_eq!(PlaceCategory::fuzzy(Some("Park")), Some(PlaceCategory::Nature));
        assert_eq!(PlaceCategory::fuzzy(Some("")), None);
        assert_eq!(TravelFactType::fuzzy(Some("recommended_time")), TravelFactType::RecommendedTime);
        assert_eq!(TravelFactType::fuzzy(Some("fee")), TravelFactType::Price);
        assert_eq!(TravelFactType::fuzzy(None), TravelFactType::Other);
        assert_eq!(SourceType::fuzzy(Some("google_maps")), SourceType::GoogleMaps);
        assert_eq!(SourceType::fuzzy(Some("Booking.com")), SourceType::Booking);
    }

    #[test]
    fn processing_states() {
        use ProcessingStatus::*;
        for s in ProcessingStatus::ALL {
            // Every state is either finished or will be picked up again — nothing gets stuck.
            assert_ne!(s.is_terminal(), s.needs_processing());
        }
        assert!(Failed.needs_processing());
        assert!(WaitingForNetwork.needs_processing() && WaitingForNetwork.can_reuse_ocr());
        assert!(!Discovered.can_reuse_ocr());
        assert!(Ignored.is_terminal() && !Ignored.needs_processing());
        assert!(Extracting.is_transient());
    }

    #[test]
    fn rect_math() {
        let a = Rect::new(0.0, 0.0, 0.5, 0.5);
        let b = Rect::new(0.25, 0.25, 0.5, 0.5);
        assert!((a.intersection(&b).area() - 0.0625).abs() < 1e-9);
        assert!((Rect::new(0.9, 0.9, 0.5, 0.5).clamped().width - 0.1).abs() < 1e-9);
    }
}
