//! Level 1: cheap, local, deterministic filtering. Clearly unrelated screenshots (code, chats,
//! banking, spreadsheets…) stop here and never generate an AI request.

use crate::config::AppConfig;
use crate::models::SourceType;
use crate::text;

#[derive(Debug, Clone, PartialEq)]
pub struct LocalReport {
    /// 0–1 likelihood of being travel-related from local signals.
    pub score: f64,
    /// 0–1 confidence that it is definitely NOT travel.
    pub not_travel_confidence: f64,
    pub source: SourceType,
    pub creator: Option<String>,
    pub countries: Vec<String>,
    pub text_length: usize,
    pub signals: Vec<String>,
}

/// What to do next with a screenshot after local analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Level 1: stop, not travel.
    SkipNotTravel,
    /// Level 2 with OCR text only (the normal case).
    AiText,
    /// Level 2 with a compressed image: little/no text but a large photo region.
    AiWithImage,
}

const STRONG: &[&str] = &[
    "things to do", "must visit", "must-visit", "bucket list", "itinerary", "travel guide", "hidden gem",
    "save this for", "places to visit", "where to stay", "check-in", "check-out", "per night", "book now",
    "opening hours", "open now", "admission", "entry fee", "tickets", "directions", "viewpoint",
    "national park", "hotel", "hostel", "ryokan", "resort", "airbnb", "tripadvisor", "booking.com", "flight",
    "boarding", "departure", "arrival", "where to eat", "what to eat", "where to go", "how to get there", "worth visiting",
    "day trip", "walking tour", "coastal walk", "city walls",
];
const KEYWORDS: &[&str] = &[
    "travel", "trip", "visit", "vacation", "holiday", "tour", "restaurant", "cafe", "café", "bar", "brunch",
    "temple", "shrine", "church", "cathedral", "museum", "gallery", "castle", "palace", "beach", "island",
    "lake", "mountain", "waterfall", "hike", "hiking", "trail", "park", "garden", "market", "street food",
    "station", "airport", "train", "ferry", "cable car", "ropeway", "gondola", "sunrise", "sunset", "view",
    "km", "min walk", "closed", "open", "free entry", "reservation", "guesthouse", "old town", "neighbourhood",
    "neighborhood", "district", "rooftop", "skyline", "onsen", "hot spring", "cruise", "reviews", "¥", "€", "$",
    "valley", "canyon", "gorge", "cave", "falls", "peak", "glacier", "river", "bay", "harbour", "harbor", "pagoda",
    "village", "province", "scenic", "sightseeing", "attraction", "landmark", "tower", "bridge", "temple", "fort",
    "foodie", "foodies", "cuisine", "dessert", "bakery", "buffet", "villa", "camping", "road trip",
    "walk", "coastal", "entry", "ruins", "bus", "metro", "subway", "night market", "old city", "lookout", "swim", "snorkel",
];
/// Matched anywhere (also inside handles like @asiaodysseytravel) and in other scripts.
const SUBSTRING_KEYWORDS: &[&str] = &[
    "travel", "wanderlust", "itinerar", "backpack", "tourism",
    "旅游", "旅行", "景区", "景点", "攻略", "酒店", "餐厅", "美食", "打卡", "夜景", "地铁", "门票", "古镇", "公园", "観光", "ホテル", "温泉", "여행", "관광", "맛집", "호텔",
];
const NEGATIVE: &[&str] = &[
    "func ", "import ", "error:", "warning:", "xcode", "terminal", "npm ", "git ", "stack trace", "exception",
    "localhost", "pull request", "delivered", "imessage", "read receipt", "whatsapp", "available balance",
    "bsb", "account number", "payment received", "invoice", "assignment", "lecture", "due date", "moodle",
    "quiz", "spreadsheet", "console.log", "def ", "=>", "http://localhost",
];
const SOURCES: &[(SourceType, &[&str])] = &[
    (SourceType::Booking, &["booking.com", "genius", "per night", "free cancellation"]),
    (SourceType::Airbnb, &["airbnb", "superhost", "guest favourite", "guest favorite"]),
    (SourceType::Tripadvisor, &["tripadvisor", "travelers' choice", "travellers' choice"]),
    (SourceType::GoogleMaps, &["directions", "google maps", "open ⋅", "closed ⋅", "closes", "opens"]),
    (SourceType::AppleMaps, &["look around", "apple maps"]),
    (SourceType::Instagram, &["instagram", "liked by", "reels", "view all comments", "original audio"]),
    (SourceType::Tiktok, &["tiktok", "for you", "add comment", "original sound"]),
    (SourceType::Youtube, &["youtube", "subscribe", "shorts"]),
    (SourceType::Xiaohongshu, &["小红书", "rednote", "xiaohongshu"]),
    (SourceType::Reddit, &["reddit", "r/", "upvote"]),
    (SourceType::Pinterest, &["pinterest"]),
    (SourceType::Facebook, &["facebook"]),
    (SourceType::Safari, &["http", "www."]),
];

pub fn analyze(ocr_text: &str) -> LocalReport {
    let lower = format!(" {} ", ocr_text.to_lowercase().replace('\n', " "));
    let strong: Vec<&str> = STRONG.iter().copied().filter(|k| lower.contains(k)).collect();
    let mut hits: Vec<&str> = KEYWORDS.iter().copied().filter(|k| lower.contains(&format!(" {k}")) || (k.chars().count() == 1 && lower.contains(k))).collect();
    hits.extend(SUBSTRING_KEYWORDS.iter().copied().filter(|k| lower.contains(k)));
    let has_address = has_street_address(&lower);
    let negative: Vec<&str> = NEGATIVE.iter().copied().filter(|k| lower.contains(k)).collect();
    let countries = text::countries_in(ocr_text);
    let source = detect_source(&lower);
    let text_length = ocr_text.chars().filter(|c| !c.is_whitespace()).count();

    let mut points = strong.len() as f64 * 2.0 + (hits.len().min(6)) as f64;
    if !countries.is_empty() {
        points += 2.0;
    }
    if ocr_text.contains('📍') {
        points += 3.0;
    }
    if has_address {
        points += 2.0;
    }
    points += match source {
        SourceType::Booking | SourceType::Airbnb | SourceType::Tripadvisor => 4.0,
        SourceType::GoogleMaps | SourceType::AppleMaps => 2.0,
        _ => 0.0,
    };
    points -= negative.len() as f64 * 2.0;

    let mut score = 1.0 - (-points.max(0.0) / 5.0).exp();
    // Very short text is too little to judge — unless it already carries travel signals (CJK text is dense).
    if text_length < 12 && strong.is_empty() && hits.is_empty() {
        score = score.min(0.15);
    }

    let positive = strong.len() + hits.len() + countries.len() + usize::from(has_address);
    let not_travel_confidence = if negative.len() >= 2 && positive <= 1 {
        0.97
    } else if !negative.is_empty() && positive == 0 {
        0.9
    } else if positive == 0 && text_length >= 40 {
        0.8
    } else {
        (1.0 - score) * 0.7
    };

    let mut signals: Vec<String> = strong.iter().chain(hits.iter()).map(|s| s.to_string()).collect();
    signals.extend(countries.iter().map(|c| format!("country:{c}")));
    signals.extend(negative.iter().map(|s| format!("-{}", s.trim())));

    LocalReport { score, not_travel_confidence, source, creator: detect_creator(ocr_text), countries, text_length, signals }
}

/// Level routing. Deterministic and cheap; the AI is only used when it adds value.
pub fn route(report: &LocalReport, has_large_photo: bool, config: &AppConfig) -> Route {
    if report.not_travel_confidence >= config.minimum_local_confidence_for_skipping_ai {
        return Route::SkipNotTravel;
    }
    // Photo-only screenshots (a place shown visually, little text) need vision to be understood.
    if report.text_length < 40 && has_large_photo && config.allow_vision_requests {
        return Route::AiWithImage;
    }
    if report.score < config.local_skip_score {
        return Route::SkipNotTravel;
    }
    Route::AiText
}

/// "498, Negombo Rd", "12 Smith Street", "775B Ridgewood Avenue" — a number followed by a street word.
fn has_street_address(lower: &str) -> bool {
    const STREET: &[&str] = &["rd", "road", "st", "street", "ave", "avenue", "lane", "ln", "blvd", "boulevard", "dr", "drive", "hwy", "highway", "mawatha", "jalan", "soi", "straße", "strasse", "rue", "calle", "via"];
    let words: Vec<&str> = lower.split(|c: char| c.is_whitespace() || c == ',').filter(|w| !w.is_empty()).collect();
    words.windows(3).any(|w| {
        let has_number = w[0].chars().next().is_some_and(|c| c.is_ascii_digit()) && w[0].chars().filter(|c| c.is_ascii_digit()).count() <= 5;
        has_number && (STREET.contains(&w[2].trim_end_matches('.')) || STREET.contains(&w[1].trim_end_matches('.')))
    })
}

fn detect_source(lower: &str) -> SourceType {
    SOURCES
        .iter()
        .map(|(s, signs)| (*s, signs.iter().filter(|k| lower.contains(*k)).count()))
        .filter(|(_, n)| *n > 0)
        .max_by_key(|(_, n)| *n)
        .map(|(s, _)| s)
        .unwrap_or(SourceType::Unknown)
}

/// Visible @handles only — identities are never inferred.
fn detect_creator(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == '·' || c == '•')
        .find(|w| {
            w.starts_with('@')
                && (4..=31).contains(&w.chars().count())
                && w[1..].chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
        })
        .map(|w| w.trim_end_matches('.').to_string())
}

/// Social-media chrome that carries no information.
const CHROME: &[&str] = &[
    "follow", "following", "like", "reply", "share", "send", "save", "more", "see translation", "sponsored",
    "view all comments", "add a comment...", "add comment...", "search", "home", "reels", "profile", "for you",
];

/// Drops UI noise (buttons, counters, clock) so only content lines are sent to the AI.
/// Returns the kept line indices so AI `source_lines` can be mapped back to OCR blocks.
pub fn content_lines(lines: &[String]) -> Vec<usize> {
    let mut kept = Vec::new();
    let mut budget = 3500usize;
    for (i, line) in lines.iter().enumerate() {
        let l = line.trim().to_lowercase();
        if l.chars().count() <= 2 || CHROME.contains(&l.as_str()) {
            continue;
        }
        if l.chars().all(|c| c.is_ascii_digit() || " .,:%kmb".contains(c)) {
            continue; // counters like "12.3K", "1,204", "9:41"
        }
        let len = l.chars().count() + 6;
        if len > budget {
            break;
        }
        budget -= len;
        kept.push(i);
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AppConfig {
        AppConfig::default()
    }

    #[test]
    fn travel_post_goes_to_ai() {
        let r = analyze("YOU HAVE TO VISIT THIS PLACE 😍\nSave this for your Japan trip!\nLake Kawaguchi\nBest view of Fuji around sunrise.\nTake the train from Shinjuku.");
        assert!(r.score > 0.5, "score {}", r.score);
        assert_eq!(r.countries, vec!["JP"]);
        assert_eq!(route(&r, false, &cfg()), Route::AiText);
    }

    #[test]
    fn code_screenshot_is_skipped_locally() {
        let r = analyze("import SwiftUI\nfunc body() -> some View {\nerror: cannot find 'x' in scope\nXcode build failed");
        assert_eq!(route(&r, false, &cfg()), Route::SkipNotTravel);
    }

    #[test]
    fn chat_without_travel_signals_is_skipped() {
        let r = analyze("Mum\nDid you pick up the milk? Delivered\niMessage\nRead receipt 9:41");
        assert_eq!(route(&r, false, &cfg()), Route::SkipNotTravel);
    }

    #[test]
    fn photo_only_screenshot_uses_vision_when_allowed() {
        let r = analyze("9:41");
        assert_eq!(route(&r, true, &cfg()), Route::AiWithImage);
        let mut no_vision = cfg();
        no_vision.allow_vision_requests = false;
        assert_eq!(route(&r, true, &no_vision), Route::SkipNotTravel);
        assert_eq!(route(&r, false, &cfg()), Route::SkipNotTravel);
    }

    #[test]
    fn detects_source_and_creator() {
        let r = analyze("travelwithxyz @travel.with_xyz · Follow\nLiked by friend and 2,003 others\nView all comments\nKyoto hidden gem");
        assert_eq!(r.source, SourceType::Instagram);
        assert_eq!(r.creator.as_deref(), Some("@travel.with_xyz"));
        let b = analyze("Booking.com\nHotel Granvia Kyoto\nFree cancellation\n¥32,000 per night");
        assert_eq!(b.source, SourceType::Booking);
    }

    #[test]
    fn content_lines_drop_chrome() {
        let lines: Vec<String> = ["9:41", "Follow", "Fushimi Inari", "12.3K", "Go before 8am", "Reply"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(content_lines(&lines), vec![2, 4]);
    }
}

#[cfg(test)]
mod recall_tests {
    use super::*;

    /// Cases taken from a real validation run that were wrongly skipped.
    #[test]
    fn real_false_negatives_now_reach_the_ai() {
        let cfg = AppConfig::default();
        let wangxian = analyze("1:31 / Asia Odyssey Trave / Wangxian Valley / asiaodysseytravel / Follow / Jiangxi really has an im");
        assert_eq!(route(&wangxian, false, &cfg), Route::AiText, "score {}", wangxian.score);
        let restaurant = analyze("12:05 / Reels • / Foodies Welcome / 15.4k / MAD MUNCH / 498, Negombo Rd, Walisara.");
        assert_eq!(route(&restaurant, false, &cfg), Route::AiText, "score {}", restaurant.score);
        let chinese = analyze("九寨沟景区 攻略 必去");
        assert_eq!(route(&chinese, false, &cfg), Route::AiText);
    }

    #[test]
    fn street_addresses() {
        assert!(has_street_address("mad munch 498, negombo rd, walisara"));
        assert!(has_street_address("12 smith street"));
        assert!(!has_street_address("2,079 likes 147 comments"));
    }
}
