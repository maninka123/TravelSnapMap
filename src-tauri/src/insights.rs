//! Smart Search and freshness/conflict warnings. Both run locally over the structured facts already
//! in the library: no AI call, no embeddings, instant and free.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;

use crate::db::{Database, FactRecord, PlaceFilter, PlaceProvenance, PlaceRecord};
use crate::models::{PersonalStatus, PlaceCategory, TravelFactType};
use crate::text::normalize;

// MARK: - Smart Search

/// A travel idea the query can ask about ("sunrise", "avoid crowds", "advance booking"…).
#[derive(Debug)]
struct Concept {
    label: &'static str,
    emoji: &'static str,
    /// Query words/phrases that mean this concept (matched at word starts).
    triggers: &'static [&'static str],
    /// Words that show it in a saved fact (matched at word starts).
    keywords: &'static [&'static str],
    /// Fact types that are this concept by definition.
    types: &'static [TravelFactType],
}

use TravelFactType as F;

const CONCEPTS: &[Concept] = &[
    Concept { label: "Sunrise", emoji: "🌅", triggers: &["sunrise", "dawn", "early morning", "daybreak"],
        keywords: &["sunrise", "dawn", "early morning", "first light", "daybreak"], types: &[] },
    Concept { label: "Sunset", emoji: "🌇", triggers: &["sunset", "golden hour", "dusk"],
        keywords: &["sunset", "golden hour", "dusk", "evening light"], types: &[] },
    Concept { label: "At night", emoji: "🌙", triggers: &["at night", "night view", "night time", "nighttime", "after dark", "illuminat", "lit up", "light up"],
        keywords: &["night", "illuminat", "lit up", "light up", "after dark"], types: &[] },
    Concept { label: "Avoiding crowds", emoji: "👥", triggers: &["crowd", "busy", "quiet", "queue", "tourist trap", "less touristy"],
        keywords: &["crowd", "overcrowd", "busy", "quiet", "queue", "line up", "less people", "fewer people", "non touristy", "less touristy", "peaceful", "avoid tourists"], types: &[] },
    Concept { label: "Advance booking", emoji: "🎟️", triggers: &["advance booking", "book in advance", "book ahead", "booking", "reservation", "reserve", "prebook", "pre book", "sold out", "advance"],
        keywords: &["book", "reserv", "in advance", "sold out", "prebook", "pre book"], types: &[F::Reservation] },
    Concept { label: "Tickets", emoji: "🎫", triggers: &["ticket", "entry fee", "entrance fee", "admission"],
        keywords: &["ticket", "entry fee", "entrance", "admission"], types: &[F::Ticket] },
    Concept { label: "Free", emoji: "🆓", triggers: &["free"], keywords: &["free"], types: &[] },
    Concept { label: "Budget", emoji: "💸", triggers: &["cheap", "budget", "affordable", "inexpensive"],
        keywords: &["cheap", "budget", "affordable", "inexpensive", "free"], types: &[] },
    Concept { label: "Photo spots", emoji: "📸", triggers: &["photo", "photograph", "instagrammable", "picture"],
        keywords: &["photo", "picture", "shot", "angle"], types: &[F::Photography] },
    Concept { label: "Views", emoji: "🏞️", triggers: &["view", "scenic", "panoram"],
        keywords: &["view", "panoram", "scenic", "overlook", "lookout"], types: &[] },
    Concept { label: "Hidden gems", emoji: "💎", triggers: &["hidden gem", "hidden", "secret", "off the beaten", "underrated", "locals"],
        keywords: &["hidden", "secret", "underrated", "off the beaten", "less known", "lesser known", "locals"], types: &[] },
    Concept { label: "Tips", emoji: "💡", triggers: &["tip", "advice"], keywords: &["tip"], types: &[F::GeneralTip] },
    Concept { label: "Opening hours", emoji: "🕘", triggers: &["opening hours", "opening times", "open late", "open early", "hours"],
        keywords: &["open", "close", "hours"], types: &[F::OpeningHours] },
    Concept { label: "Getting there", emoji: "🚆", triggers: &["how to get", "getting there", "transport", "parking", "by train", "by bus"],
        keywords: &["train", "bus", "station", "parking", "ferry", "taxi", "metro", "subway"], types: &[F::Transportation, F::Route] },
    Concept { label: "What to eat", emoji: "🍜", triggers: &["must try", "what to eat", "what to order", "dishes", "signature dish"],
        keywords: &["must try", "order", "dish", "menu", "signature"], types: &[F::Food] },
    Concept { label: "Warnings", emoji: "⚠️", triggers: &["warning", "scam", "dangerous", "careful", "safety"],
        keywords: &["warning", "scam", "careful", "danger", "beware"], types: &[F::Warning] },
    Concept { label: "Family friendly", emoji: "👨‍👩‍👧", triggers: &["kid", "family", "families", "children", "child"],
        keywords: &["kid", "family", "families", "children", "child"], types: &[] },
    Concept { label: "Rainy day", emoji: "☔", triggers: &["rain", "indoor"], keywords: &["rain", "indoor"], types: &[] },
    Concept { label: "Spring", emoji: "🌸", triggers: &["spring", "cherry blossom", "sakura", "blossom"],
        keywords: &["spring", "cherry blossom", "sakura", "blossom"], types: &[] },
    Concept { label: "Summer", emoji: "☀️", triggers: &["summer"], keywords: &["summer"], types: &[] },
    Concept { label: "Autumn", emoji: "🍁", triggers: &["autumn", "fall foliage", "fall colors", "fall colours", "foliage", "koyo"],
        keywords: &["autumn", "foliage", "koyo", "autumn leaves", "fall colo"], types: &[] },
    Concept { label: "Winter", emoji: "❄️", triggers: &["winter", "snow"], keywords: &["winter", "snow"], types: &[] },
    Concept { label: "Accessibility", emoji: "♿", triggers: &["wheelchair", "accessib", "stroller", "step free"],
        keywords: &["wheelchair", "accessib", "stroller", "step free"], types: &[F::Accessibility] },
    Concept { label: "How long to spend", emoji: "⏱️", triggers: &["how long", "duration", "half day", "full day"],
        keywords: &["hour", "half day", "full day", "minutes"], types: &[F::Duration] },
];

use PlaceCategory as C;

/// Whole query words (plurals accepted) that mean a kind of place.
const CATEGORY_WORDS: &[(&str, &[PlaceCategory])] = &[
    ("restaurant", &[C::Restaurant, C::Food]), ("eat", &[C::Restaurant, C::Food, C::Cafe]), ("eats", &[C::Restaurant, C::Food, C::Cafe]),
    ("dining", &[C::Restaurant, C::Food]), ("dinner", &[C::Restaurant, C::Food]), ("lunch", &[C::Restaurant, C::Food, C::Cafe]),
    ("food", &[C::Food, C::Restaurant, C::Cafe]), ("foodie", &[C::Food, C::Restaurant, C::Cafe]),
    ("cafe", &[C::Cafe]), ("coffee", &[C::Cafe]), ("brunch", &[C::Cafe, C::Restaurant]), ("bakery", &[C::Food, C::Cafe]),
    ("dessert", &[C::Cafe, C::Food]), ("bar", &[C::Nightlife]), ("pub", &[C::Nightlife]), ("nightlife", &[C::Nightlife]),
    ("club", &[C::Nightlife]), ("hotel", &[C::Hotel, C::Accommodation]), ("stay", &[C::Hotel, C::Accommodation]),
    ("accommodation", &[C::Hotel, C::Accommodation]), ("hostel", &[C::Accommodation]), ("ryokan", &[C::Hotel]),
    ("resort", &[C::Hotel]), ("beach", &[C::Beach]), ("temple", &[C::Temple, C::ReligiousSite]), ("shrine", &[C::Temple, C::ReligiousSite]),
    ("church", &[C::ReligiousSite]), ("cathedral", &[C::ReligiousSite]), ("mosque", &[C::ReligiousSite]),
    ("museum", &[C::Museum]), ("gallery", &[C::Museum]), ("castle", &[C::HistoricSite]), ("ruin", &[C::HistoricSite]),
    ("historic", &[C::HistoricSite]), ("viewpoint", &[C::Viewpoint]), ("lookout", &[C::Viewpoint]),
    ("hike", &[C::Hiking, C::Nature]), ("hiking", &[C::Hiking, C::Nature]), ("trail", &[C::Hiking, C::Nature]), ("trek", &[C::Hiking, C::Nature]),
    ("nature", &[C::Nature, C::Hiking, C::Beach]), ("park", &[C::Nature]), ("lake", &[C::Nature]), ("waterfall", &[C::Nature]),
    ("mountain", &[C::Nature, C::Hiking]), ("forest", &[C::Nature]), ("garden", &[C::Nature]),
    ("shop", &[C::Shopping]), ("shopping", &[C::Shopping]), ("market", &[C::Food, C::Shopping]), ("mall", &[C::Shopping]),
    ("activity", &[C::Activity]), ("tour", &[C::Activity]), ("attraction", &[C::Attraction]), ("sight", &[C::Attraction, C::Viewpoint, C::HistoricSite]),
    ("sightseeing", &[C::Attraction, C::Viewpoint, C::HistoricSite]), ("landmark", &[C::Attraction, C::HistoricSite]),
    ("airport", &[C::Airport]), ("station", &[C::Station]),
];

/// Words that carry no meaning for matching ("Places I saved from…").
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "of", "in", "at", "on", "to", "for", "from", "with", "by", "near", "around", "into", "about",
    "i", "me", "my", "we", "our", "you", "someone", "somebody", "people", "person", "anyone", "they",
    "place", "places", "spot", "spots", "thing", "things", "stuff", "location", "locations", "somewhere",
    "good", "great", "best", "nice", "top", "cool", "amazing", "beautiful", "recommended", "recommend", "recommends",
    "saved", "save", "saving", "mentioned", "mention", "mentions", "said", "says", "say", "talked", "suggested",
    "where", "which", "what", "that", "this", "these", "those", "there", "here", "is", "are", "was", "were", "be", "it", "its",
    "show", "find", "list", "all", "any", "some", "go", "going", "visit", "see", "do", "can", "should", "worth",
    "requiring", "require", "requires", "required", "need", "needs", "needed", "needing",
    "avoid", "avoiding", "avoided", "get", "have", "has", "had", "when", "time", "times",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Chip {
    /// country | city | category | source | creator | status | concept | term
    pub kind: &'static str,
    pub label: String,
    pub emoji: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub place: PlaceRecord,
    /// The saved facts that made this place match (with their sources).
    pub matches: Vec<FactRecord>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSearchResult {
    pub chips: Vec<Chip>,
    pub hits: Vec<SearchHit>,
}

#[derive(Debug, Default)]
struct Parsed {
    countries: Vec<String>,
    cities: Vec<String>,
    categories: Vec<PlaceCategory>,
    sources: Vec<&'static str>,
    creators: Vec<String>,
    statuses: Vec<PersonalStatus>,
    not_visited: bool,
    concepts: Vec<&'static Concept>,
    terms: Vec<String>,
    chips: Vec<Chip>,
}

fn padded(s: &str) -> String {
    format!(" {} ", s)
}

/// `phrase` appears in `hay` starting at a word boundary (so "crowd" matches "crowded", not "overcrowd"… and
/// never inside another word's middle).
fn has_word_prefix(hay_padded: &str, phrase: &str) -> bool {
    hay_padded.contains(&format!(" {phrase}"))
}

fn is_plural_of(token: &str, word: &str) -> bool {
    token == word
        || token.strip_suffix('s') == Some(word)
        || token.strip_suffix("es") == Some(word)
        || (word.ends_with('y') && token.strip_suffix("ies") == Some(&word[..word.len() - 1]))
}

fn parse(query: &str, known_cities: &[(String, String)]) -> Parsed {
    let mut p = Parsed::default();
    let q = normalize(query);
    let qp = padded(&q);
    let tokens: Vec<&str> = q.split(' ').filter(|t| !t.is_empty()).collect();
    let mut used: HashSet<usize> = HashSet::new();
    let consume = |phrase: &str, used: &mut HashSet<usize>| {
        let words: Vec<&str> = phrase.split(' ').collect();
        for (i, _) in tokens.iter().enumerate() {
            if words.iter().enumerate().all(|(k, w)| tokens.get(i + k).is_some_and(|t| t.starts_with(w))) {
                used.extend(i..i + words.len());
            }
        }
    };

    // Creators: "@handle" in the raw text.
    for raw in query.split_whitespace().filter(|t| t.starts_with('@') && t.len() > 1) {
        let handle = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.' && c != '@').to_lowercase();
        p.chips.push(Chip { kind: "creator", label: handle.clone(), emoji: Some("👤".into()), code: None });
        consume(&normalize(&handle), &mut used);
        p.creators.push(handle.trim_start_matches('@').to_string());
    }

    // Status (negations first).
    for phrase in ["not visited", "not been", "havent visited", "haven t visited", "havent been", "haven t been", "never been", "unvisited", "not yet visited"] {
        if qp.contains(&padded(phrase)) {
            p.not_visited = true;
            consume(phrase, &mut used);
        }
    }
    if p.not_visited {
        p.chips.push(Chip { kind: "status", label: "Not visited yet".into(), emoji: Some("🗺️".into()), code: None });
    } else {
        for (phrases, status, label, emoji) in [
            (&["visited", "been to", "went to", "i ve been"][..], PersonalStatus::Visited, "Visited", "✅"),
            (&["favourite", "favorite", "favs", "loved"][..], PersonalStatus::Favourite, "Favourites", "⭐"),
            (&["wishlist", "want to visit", "want to go", "bucket list"][..], PersonalStatus::WantToVisit, "Want to visit", "📌"),
        ] {
            if phrases.iter().any(|ph| has_word_prefix(&qp, ph)) {
                phrases.iter().for_each(|ph| consume(ph, &mut used));
                p.statuses.push(status);
                p.chips.push(Chip { kind: "status", label: label.into(), emoji: Some(emoji.into()), code: None });
            }
        }
    }

    // Sources.
    for (words, source, label, emoji) in [
        (&["instagram", "insta", "ig", "reel", "reels"][..], "instagram", "Instagram", "📷"),
        (&["tiktok", "tik tok"][..], "tiktok", "TikTok", "🎵"),
        (&["youtube", "yt"][..], "youtube", "YouTube", "▶️"),
        (&["google maps"][..], "googleMaps", "Google Maps", "🗺️"),
        (&["xiaohongshu", "rednote", "red note"][..], "xiaohongshu", "Xiaohongshu", "📕"),
        (&["reddit"][..], "reddit", "Reddit", "👽"),
        (&["tripadvisor"][..], "tripadvisor", "Tripadvisor", "🦉"),
    ] {
        if words.iter().any(|w| qp.contains(&padded(w))) {
            words.iter().for_each(|w| consume(w, &mut used));
            p.sources.push(source);
            p.chips.push(Chip { kind: "source", label: label.into(), emoji: Some(emoji.into()), code: None });
        }
    }

    // Countries (full names) and cities you have places in.
    for code in crate::text::countries_in(query) {
        if let Some(name) = crate::text::country_name(&code) {
            consume(&normalize(name), &mut used);
            p.chips.push(Chip { kind: "country", label: name.to_string(), emoji: None, code: Some(code.clone()) });
        }
        p.countries.push(code);
    }
    for (norm, display) in known_cities {
        if norm.len() > 2 && qp.contains(&padded(norm)) && !p.cities.contains(norm) {
            consume(norm, &mut used);
            p.cities.push(norm.clone());
            p.chips.push(Chip { kind: "city", label: display.clone(), emoji: Some("🏙️".into()), code: None });
        }
    }

    // Concepts (longest trigger wins inside each concept; several concepts combine with AND).
    for concept in CONCEPTS {
        let hits: Vec<&&str> = concept.triggers.iter().filter(|t| has_word_prefix(&qp, t)).collect();
        if hits.is_empty() {
            continue;
        }
        hits.iter().for_each(|t| consume(t, &mut used));
        p.concepts.push(concept);
        p.chips.push(Chip { kind: "concept", label: concept.label.into(), emoji: Some(concept.emoji.into()), code: None });
    }

    // Categories: whole words only ("bar" must not match "barcelona").
    for (i, token) in tokens.iter().enumerate() {
        if used.contains(&i) {
            continue;
        }
        if let Some((word, cats)) = CATEGORY_WORDS.iter().find(|(w, _)| is_plural_of(token, w)) {
            used.insert(i);
            let new: Vec<PlaceCategory> = cats.iter().copied().filter(|c| !p.categories.contains(c)).collect();
            if !new.is_empty() {
                p.categories.extend(new);
                let label = cats[0].as_str();
                p.chips.push(Chip { kind: "category", label: title_case(word), emoji: None, code: Some(label.to_string()) });
            }
        }
    }

    // Whatever is left must appear somewhere in the place's names, notes or saved facts.
    for (i, token) in tokens.iter().enumerate() {
        if used.contains(&i) || STOPWORDS.contains(token) || token.len() < 2 {
            continue;
        }
        let term = if token.len() > 4 { token.strip_suffix('s').unwrap_or(token) } else { token };
        if !p.terms.iter().any(|t| t == term) {
            p.terms.push(term.to_string());
            p.chips.push(Chip { kind: "term", label: format!("“{token}”"), emoji: None, code: None });
        }
    }
    p
}

fn title_case(w: &str) -> String {
    let mut c = w.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn fact_matches_concept(fact: &FactRecord, text_padded: &str, concept: &Concept) -> bool {
    concept.types.contains(&fact.fact_type) || concept.keywords.iter().any(|k| has_word_prefix(text_padded, k))
}

pub fn smart_search(db: &Database, query: &str, verified_only: bool) -> Result<SmartSearchResult> {
    let places = db.list_places(&PlaceFilter { verified_only, ..Default::default() })?;
    let mut cities: Vec<(String, String)> = Vec::new();
    for p in &places {
        for name in [&p.city, &p.region].into_iter().flatten() {
            let n = normalize(name);
            if !n.is_empty() && !cities.iter().any(|(c, _)| *c == n) {
                cities.push((n, name.clone()));
            }
        }
    }
    cities.sort_by_key(|(n, _)| std::cmp::Reverse(n.len())); // "New York City" before "York"
    let parsed = parse(query, &cities);

    let mut facts_by_place: HashMap<&str, Vec<&FactRecord>> = HashMap::new();
    let facts = db.all_facts()?;
    for f in &facts {
        facts_by_place.entry(f.place_id.as_str()).or_default().push(f);
    }
    let provenance = db.place_provenance()?;
    let empty = PlaceProvenance::default();

    let mut hits: Vec<SearchHit> = Vec::new();
    for place in places {
        // Hard filters.
        if !parsed.countries.is_empty() && !place.country_code.as_deref().is_some_and(|c| parsed.countries.iter().any(|q| q.eq_ignore_ascii_case(c))) {
            continue;
        }
        if !parsed.cities.is_empty() {
            let here = [&place.city, &place.region].into_iter().flatten().map(|c| normalize(c)).collect::<Vec<_>>();
            if !parsed.cities.iter().any(|c| here.contains(c)) {
                continue;
            }
        }
        if !parsed.categories.is_empty() && !parsed.categories.contains(&place.category) {
            continue;
        }
        if !parsed.statuses.is_empty() && !parsed.statuses.contains(&place.personal_status) {
            continue;
        }
        if parsed.not_visited && matches!(place.personal_status, PersonalStatus::Visited | PersonalStatus::NotInterested) {
            continue;
        }
        let prov = provenance.get(&place.id).unwrap_or(&empty);
        if !parsed.sources.is_empty() && !parsed.sources.iter().all(|s| prov.source_types.iter().any(|t| t == s)) {
            continue;
        }
        if !parsed.creators.is_empty()
            && !parsed.creators.iter().all(|c| prov.creators.iter().any(|pc| pc.to_lowercase().trim_start_matches('@').contains(c.as_str())))
        {
            continue;
        }

        let place_facts = facts_by_place.get(place.id.as_str()).map(Vec::as_slice).unwrap_or(&[]);
        let fact_texts: Vec<String> = place_facts.iter().map(|f| padded(&normalize(&format!("{} {}", f.text, f.source_quote.clone().unwrap_or_default())))).collect();
        let own_text = padded(&normalize(&format!(
            "{} {} {}",
            place.notes,
            place.visit_notes,
            place.summary_text.clone().unwrap_or_default()
        )));
        let name_text = padded(&normalize(&format!(
            "{} {} {} {} {} {}",
            place.all_names().join(" "),
            place.city.clone().unwrap_or_default(),
            place.region.clone().unwrap_or_default(),
            place.country.clone().unwrap_or_default(),
            place.address.clone().unwrap_or_default(),
            prov.creators.join(" ")
        )));

        let mut matched: Vec<usize> = Vec::new();
        let mut score = 0.0;
        let mut ok = true;
        for concept in &parsed.concepts {
            let found: Vec<usize> = (0..place_facts.len()).filter(|&i| fact_matches_concept(place_facts[i], &fact_texts[i], concept)).collect();
            let in_notes = concept.keywords.iter().any(|k| has_word_prefix(&own_text, k));
            if found.is_empty() && !in_notes {
                ok = false;
                break;
            }
            score += 2.0 * found.len().min(3) as f64 + if in_notes { 1.0 } else { 0.0 };
            matched.extend(found);
        }
        if !ok {
            continue;
        }
        for term in &parsed.terms {
            let in_name = has_word_prefix(&name_text, term);
            let found: Vec<usize> = (0..place_facts.len()).filter(|&i| has_word_prefix(&fact_texts[i], term)).collect();
            let in_notes = has_word_prefix(&own_text, term) || has_word_prefix(&padded(place.category.as_str()), term);
            if !in_name && found.is_empty() && !in_notes {
                ok = false;
                break;
            }
            score += if in_name { 3.0 } else { 0.0 } + found.len().min(3) as f64 + if in_notes { 1.0 } else { 0.0 };
            matched.extend(found);
        }
        if !ok {
            continue;
        }
        matched.sort_unstable();
        matched.dedup();
        score += 0.1 * place.source_count as f64;
        let matches = matched.into_iter().take(3).map(|i| place_facts[i].clone()).collect();
        hits.push(SearchHit { place, matches, score });
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.place.canonical_name.cmp(&b.place.canonical_name)));
    hits.truncate(300);
    Ok(SmartSearchResult { chips: parsed.chips, hits })
}

// MARK: - Freshness and conflict warnings

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceWarning {
    /// stale | conflict
    pub kind: &'static str,
    pub title: String,
    pub detail: String,
    pub fact_ids: Vec<String>,
}

/// Saved values older than this are flagged as possibly out of date.
const STALE_AFTER_MONTHS: i64 = 12;
/// Different values saved further apart than this are a change over time, not a disagreement.
const SAME_PERIOD_DAYS: i64 = 180;

fn fact_day(f: &FactRecord) -> Option<NaiveDate> {
    let s = f.valid_from.as_deref().unwrap_or(&f.created_at);
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn source_key(f: &FactRecord) -> &str {
    f.reel_id.as_deref().or(f.screenshot_id.as_deref()).unwrap_or(&f.id)
}

fn age_label(months: i64) -> String {
    match months {
        m if m < 24 => format!("{m} months ago"),
        m => format!("{} years ago", m / 12),
    }
}

fn count_word(n: usize) -> String {
    match n {
        2 => "Two".into(),
        3 => "Three".into(),
        4 => "Four".into(),
        n => n.to_string(),
    }
}

/// Times of day as minutes after midnight ("9am", "09:30", "5 pm", "17:00", "9-17").
fn times_in(text: &str) -> HashSet<u32> {
    let lower = text.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut out = HashSet::new();
    if lower.contains("24 hours") || lower.contains("24h") || lower.contains("24/7") {
        out.insert(24 * 60);
    }
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() || (i > 0 && (chars[i - 1].is_ascii_digit() || chars[i - 1] == '.' || chars[i - 1] == ':')) {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        let Ok(mut hour) = chars[start..i].iter().collect::<String>().parse::<u32>() else { continue };
        let mut minute = 0;
        let mut explicit = false;
        if i + 2 < chars.len() && (chars[i] == ':' || chars[i] == '.') && chars[i + 1].is_ascii_digit() && chars[i + 2].is_ascii_digit() {
            minute = chars[i + 1].to_digit(10).unwrap() * 10 + chars[i + 2].to_digit(10).unwrap();
            i += 3;
            explicit = true;
        }
        let rest: String = chars[i..chars.len().min(i + 4)].iter().collect::<String>().trim_start().to_string();
        if rest.starts_with("am") || rest.starts_with("a.m") {
            if hour == 12 { hour = 0; }
            explicit = true;
        } else if rest.starts_with("pm") || rest.starts_with("p.m") {
            if hour < 12 { hour += 12; }
            explicit = true;
        }
        // Bare numbers only count next to a range dash ("9-17"), so "7 days a week" isn't a time.
        let near_dash = chars.get(i).is_some_and(|c| matches!(c, '-' | '–' | '~')) || (start > 0 && matches!(chars[start - 1], '-' | '–' | '~'));
        if hour <= 24 && minute < 60 && (explicit || near_dash) {
            out.insert(hour * 60 + minute);
        }
    }
    out
}

/// Amounts in a price text (thousands separators removed; years ignored).
fn amounts_in(text: &str) -> HashSet<u64> {
    let mut out = HashSet::new();
    let mut current = String::new();
    let flush = |s: &mut String, out: &mut HashSet<u64>| {
        if let Ok(v) = s.replace(',', "").parse::<f64>() {
            let year_like = (1990.0..=2040.0).contains(&v) && v.fract() == 0.0;
            if v > 0.0 && !year_like {
                out.insert((v * 100.0).round() as u64);
            }
        }
        s.clear();
    };
    for c in text.chars() {
        if c.is_ascii_digit() || ((c == ',' || c == '.') && !current.is_empty()) {
            current.push(c);
        } else {
            let trimmed = current.trim_end_matches([',', '.']).to_string();
            current = trimmed;
            flush(&mut current, &mut out);
        }
    }
    let trimmed = current.trim_end_matches([',', '.']).to_string();
    current = trimmed;
    flush(&mut current, &mut out);
    out
}

/// Best-time buckets per dimension: time of day, season, week.
fn visit_buckets(text: &str) -> [HashSet<&'static str>; 3] {
    let t = padded(&normalize(text));
    let pick = |table: &[(&'static str, &[&str])]| -> HashSet<&'static str> {
        table.iter().filter(|(_, words)| words.iter().any(|w| has_word_prefix(&t, w))).map(|(b, _)| *b).collect()
    };
    [
        pick(&[
            ("morning", &["sunrise", "dawn", "early morning", "morning", "before 8", "first thing"]),
            ("midday", &["noon", "midday", "afternoon", "lunchtime"]),
            ("evening", &["sunset", "dusk", "golden hour", "evening"]),
            ("night", &["night", "after dark", "illuminat"]),
        ]),
        pick(&[
            ("spring", &["spring", "march", "april", "may ", "cherry blossom", "sakura"]),
            ("summer", &["summer", "june", "july", "august"]),
            ("autumn", &["autumn", "fall ", "september", "october", "november", "foliage"]),
            ("winter", &["winter", "december", "january", "february", "snow"]),
        ]),
        pick(&[("weekday", &["weekday"]), ("weekend", &["weekend", "saturday", "sunday"])]),
    ]
}

/// Warnings for one place's facts, as of `today`. Whole cities/regions collect unrelated prices (flight
/// deals, tours), so price checks only apply to actual places.
pub fn warnings_for(facts: &[FactRecord], category: PlaceCategory, today: NaiveDate) -> Vec<PlaceWarning> {
    let mut out = Vec::new();
    let is_area = matches!(category, PlaceCategory::City | PlaceCategory::Region);

    // 1. Freshness: the newest saved value of things that change.
    for (types, label) in [
        (&[F::Ticket][..], "Ticket price"),
        (&[F::Price][..], "Price"),
        (&[F::OpeningHours][..], "Opening hours"),
        (&[F::Reservation][..], "Booking info"),
    ] {
        if is_area && types != [F::OpeningHours] {
            continue;
        }
        let newest = facts.iter().filter(|f| types.contains(&f.fact_type)).filter_map(|f| Some((fact_day(f)?, f))).max_by_key(|(d, _)| *d);
        if let Some((day, fact)) = newest {
            use chrono::Datelike;
            let months = (today.year() - day.year()) as i64 * 12 + today.month() as i64 - day.month() as i64
                - i64::from(today.day() < day.day());
            if months >= STALE_AFTER_MONTHS {
                out.push(PlaceWarning {
                    kind: "stale",
                    title: format!("{label} saved {}", age_label(months)),
                    detail: format!("“{}” — saved {}. It may have changed; check before you go.", fact.text, day.format("%b %Y")),
                    fact_ids: vec![fact.id.clone()],
                });
            }
        }
    }

    // 2. Disagreements between sources saved around the same time.
    let conflict = |types: &[TravelFactType], values: &dyn Fn(&str) -> HashSet<u64>, title: &str| -> Option<PlaceWarning> {
        let items: Vec<(&FactRecord, HashSet<u64>, Option<NaiveDate>)> = facts
            .iter()
            .filter(|f| types.contains(&f.fact_type))
            .map(|f| (f, values(&f.text), fact_day(f)))
            .filter(|(_, v, _)| !v.is_empty())
            .collect();
        let mut involved: Vec<&FactRecord> = Vec::new();
        for (i, (a, va, da)) in items.iter().enumerate() {
            for (b, vb, db) in &items[i + 1..] {
                let same_period = match (da, db) {
                    (Some(x), Some(y)) => (*x - *y).num_days().abs() <= SAME_PERIOD_DAYS,
                    _ => true,
                };
                if source_key(a) != source_key(b) && same_period && va.is_disjoint(vb) {
                    for f in [*a, *b] {
                        if !involved.iter().any(|x| x.id == f.id) {
                            involved.push(f);
                        }
                    }
                }
            }
        }
        let sources: HashSet<&str> = involved.iter().map(|f| source_key(f)).collect();
        (sources.len() >= 2).then(|| PlaceWarning {
            kind: "conflict",
            title: format!("{} sources give different {title}", count_word(sources.len())),
            detail: involved.iter().map(|f| format!("“{}”", f.text)).collect::<Vec<_>>().join(" vs "),
            fact_ids: involved.iter().map(|f| f.id.clone()).collect(),
        })
    };
    let times = |t: &str| times_in(t).into_iter().map(u64::from).collect::<HashSet<u64>>();
    out.extend(conflict(&[F::OpeningHours], &times, "opening times"));
    // Only entry tickets: "Price" facts often describe different things (a set menu vs a drink).
    if !is_area {
        out.extend(conflict(&[F::Ticket], &amounts_in, "ticket prices"));
    }

    // 3. Different advice on when to go (e.g. sunrise vs sunset).
    let timing: Vec<(&FactRecord, [HashSet<&'static str>; 3])> = facts
        .iter()
        .filter(|f| matches!(f.fact_type, F::RecommendedTime | F::BestSeason))
        .map(|f| (f, visit_buckets(&f.text)))
        .collect();
    let mut pairs: Vec<(&FactRecord, &FactRecord)> = Vec::new();
    for (i, (a, ba)) in timing.iter().enumerate() {
        for (b, bb) in &timing[i + 1..] {
            let differs = (0..3).any(|d| !ba[d].is_empty() && !bb[d].is_empty() && ba[d].is_disjoint(&bb[d]));
            if source_key(a) != source_key(b) && differs {
                pairs.push((a, b));
            }
        }
    }
    if let Some((a, b)) = pairs.first() {
        let mut ids: Vec<String> = pairs.iter().flat_map(|(x, y)| [x.id.clone(), y.id.clone()]).collect();
        ids.dedup();
        out.push(PlaceWarning {
            kind: "conflict",
            title: "Different recommendations for the best time to visit".into(),
            detail: format!("“{}” vs “{}”", a.text, b.text),
            fact_ids: ids,
        });
    }
    out
}

/// Warnings for every place that has any (place id → warnings).
pub fn all_warnings(db: &Database) -> Result<HashMap<String, Vec<PlaceWarning>>> {
    let today = chrono::Local::now().date_naive();
    let categories: HashMap<String, PlaceCategory> =
        db.list_places(&PlaceFilter::default())?.into_iter().map(|p| (p.id, p.category)).collect();
    let mut by_place: HashMap<String, Vec<FactRecord>> = HashMap::new();
    for f in db.all_facts()? {
        by_place.entry(f.place_id.clone()).or_default().push(f);
    }
    Ok(by_place
        .into_iter()
        .map(|(id, facts)| {
            let category = categories.get(&id).copied().unwrap_or_default();
            (id, warnings_for(&facts, category, today))
        })
        .filter(|(_, w)| !w.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DataOrigin, SourceType};

    fn fact(id: &str, source: &str, kind: TravelFactType, text: &str, day: &str) -> FactRecord {
        FactRecord {
            id: id.into(), place_id: "p".into(), screenshot_id: Some(source.into()), reel_id: None, source_kind: "screenshot".into(),
            source_time_sec: None, fact_type: kind, text: text.into(), source_quote: None, confidence: 0.9, source_block_ids: vec![],
            valid_from: Some(day.into()), origin: DataOrigin::Ai, created_at: day.into(), source_type: Some(SourceType::Instagram), creator: None,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 9, 26).unwrap()
    }

    #[test]
    fn parses_the_example_queries() {
        let cities = vec![("tokyo".to_string(), "Tokyo".to_string())];
        let p = parse("Places in Japan good for sunrise", &cities);
        assert_eq!(p.countries, vec!["JP"]);
        assert_eq!(p.concepts.iter().map(|c| c.label).collect::<Vec<_>>(), vec!["Sunrise"]);
        assert!(p.terms.is_empty(), "{:?}", p.terms);

        let p = parse("Restaurants I saved from Instagram in Tokyo", &cities);
        assert!(p.categories.contains(&PlaceCategory::Restaurant));
        assert_eq!(p.sources, vec!["instagram"]);
        assert_eq!(p.cities, vec!["tokyo"]);
        assert!(p.terms.is_empty(), "{:?}", p.terms);

        let p = parse("Places where someone mentioned avoiding crowds", &cities);
        assert_eq!(p.concepts.iter().map(|c| c.label).collect::<Vec<_>>(), vec!["Avoiding crowds"]);
        assert!(p.terms.is_empty(), "{:?}", p.terms);

        let p = parse("Things requiring advance booking", &cities);
        assert_eq!(p.concepts.iter().map(|c| c.label).collect::<Vec<_>>(), vec!["Advance booking"]);
        assert!(p.terms.is_empty(), "{:?}", p.terms);
    }

    #[test]
    fn category_words_do_not_match_inside_names() {
        let p = parse("barcelona tapas", &[]);
        assert!(p.categories.is_empty());
        assert_eq!(p.terms, vec!["barcelona", "tapa"]); // plural trimmed; still prefix-matches "tapas"
        let p = parse("cafes not visited", &[]);
        assert_eq!(p.categories, vec![PlaceCategory::Cafe]);
        assert!(p.not_visited);
    }

    #[test]
    fn old_prices_are_flagged_as_stale() {
        let w = warnings_for(&[fact("a", "s1", F::Ticket, "¥1,000 entry", "2025-03-01")], PlaceCategory::Attraction, today());
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].title, "Ticket price saved 18 months ago");
        assert!(warnings_for(&[fact("a", "s1", F::Ticket, "¥1,000", "2026-06-01")], PlaceCategory::Attraction, today()).is_empty());
    }

    #[test]
    fn different_opening_times_conflict_but_same_times_do_not() {
        let hours = [
            fact("a", "s1", F::OpeningHours, "Open 9am–5pm", "2026-05-01"),
            fact("b", "s2", F::OpeningHours, "Opens 10:00 until 18:00", "2026-06-01"),
        ];
        let w = warnings_for(&hours, PlaceCategory::Attraction, today());
        assert!(w.iter().any(|w| w.title == "Two sources give different opening times"), "{w:?}");
        let same = [
            fact("a", "s1", F::OpeningHours, "Open 9am–5pm", "2026-05-01"),
            fact("b", "s2", F::OpeningHours, "09:00-17:00", "2026-06-01"),
        ];
        assert!(warnings_for(&same, PlaceCategory::Attraction, today()).is_empty());
    }

    #[test]
    fn price_changes_over_years_are_not_conflicts() {
        let prices = [
            fact("a", "s1", F::Price, "¥2,000", "2023-01-01"),
            fact("b", "s2", F::Price, "¥2,500", "2026-08-01"),
        ];
        assert!(!warnings_for(&prices, PlaceCategory::Attraction, today()).iter().any(|w| w.kind == "conflict"));
    }

    #[test]
    fn sunrise_versus_sunset_advice_is_flagged() {
        let advice = [
            fact("a", "s1", F::RecommendedTime, "Go at sunrise to beat the crowds", "2026-01-01"),
            fact("b", "s2", F::RecommendedTime, "Best at sunset", "2026-02-01"),
        ];
        let w = warnings_for(&advice, PlaceCategory::Attraction, today());
        assert!(w.iter().any(|w| w.title.starts_with("Different recommendations")), "{w:?}");
    }

    #[test]
    fn time_and_amount_parsing() {
        assert_eq!(times_in("Open 9am-5pm daily"), HashSet::from([540, 1020]));
        assert!(times_in("Open 7 days a week").is_empty());
        assert_eq!(amounts_in("Adults ¥1,500 (2024)"), HashSet::from([150000]));
    }
}

