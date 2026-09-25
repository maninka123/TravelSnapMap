//! Deterministic text utilities: normalisation, name similarity, hashing, country lookup.
//! None of this ever calls the AI.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use sha2::{Digest, Sha256};
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::countries::COUNTRIES;

/// Lowercased, accent/width-folded, punctuation-free form used for fuzzy comparison.
pub fn normalize(s: &str) -> String {
    let folded: String = s
        .nfkd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    folded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Stable SHA-256 of normalised text; identical OCR input ⇒ identical hash ⇒ AI cache hit.
pub fn stable_hash(s: &str) -> String {
    hex::encode(Sha256::digest(normalize(s).as_bytes()))
}

const STOP_WORDS: &[&str] = &["the", "of", "a", "an", "and", "de", "la", "le", "el", "at"];

fn tokens(s: &str) -> HashSet<String> {
    normalize(s)
        .split(' ')
        .filter(|t| !t.is_empty() && !STOP_WORDS.contains(t))
        .map(str::to_string)
        .collect()
}

/// 0…1 similarity between two names. Tolerates spacing ("Sky Tree"/"Skytree") and extra words
/// ("Arashiyama"/"Arashiyama Bamboo Grove").
pub fn name_similarity(a: &str, b: &str) -> f64 {
    let (na, nb) = (normalize(a), normalize(b));
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    let (ca, cb) = (na.replace(' ', ""), nb.replace(' ', ""));
    if ca == cb {
        return 0.95;
    }
    if ca.contains(&cb) || cb.contains(&ca) {
        let (la, lb) = (ca.chars().count() as f64, cb.chars().count() as f64);
        return 0.6 + 0.3 * la.min(lb) / la.max(lb);
    }
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    inter / union * 0.85
}

pub fn best_similarity(name: &str, others: &[String]) -> f64 {
    others.iter().map(|o| name_similarity(name, o)).fold(0.0, f64::max)
}

fn country_index() -> &'static HashMap<String, &'static str> {
    static INDEX: OnceLock<HashMap<String, &'static str>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut map: HashMap<String, &'static str> = COUNTRIES.iter().map(|(code, name)| (normalize(name), *code)).collect();
        for (alias, code) in [
            ("usa", "US"), ("united states of america", "US"), ("america", "US"), ("uk", "GB"),
            ("england", "GB"), ("scotland", "GB"), ("wales", "GB"), ("great britain", "GB"),
            ("korea", "KR"), ("china", "CN"), ("czech republic", "CZ"), ("holland", "NL"),
            ("uae", "AE"), ("russia", "RU"), ("vietnam", "VN"), ("turkey", "TR"), ("macau", "MO"),
        ] {
            map.insert(alias.to_string(), code);
        }
        map
    })
}

/// ISO code for an English country name (or a 2-letter code).
pub fn country_code(name: Option<&str>) -> Option<String> {
    let n = normalize(name?);
    if n.is_empty() {
        return None;
    }
    if n.len() == 2 {
        let upper = n.to_uppercase();
        if COUNTRIES.iter().any(|(c, _)| *c == upper) {
            return Some(upper);
        }
    }
    country_index().get(&n).map(|c| c.to_string())
}

/// Country codes whose names appear as whole words in the text.
pub fn countries_in(text: &str) -> Vec<String> {
    let haystack = format!(" {} ", normalize(text));
    let mut found: Vec<String> = country_index()
        .iter()
        .filter(|(name, _)| name.len() > 2 && haystack.contains(&format!(" {name} ")))
        .map(|(_, code)| code.to_string())
        .collect();
    found.sort();
    found.dedup();
    found
}

/// Great-circle distance in metres.
pub fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6_371_000.0;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_accents_case_and_punctuation() {
        assert_eq!(normalize("Kiyomizu-dera!"), "kiyomizu dera");
        assert_eq!(normalize("Café  Rüdesheim"), "cafe rudesheim");
        assert_eq!(normalize("ＴＯＫＹＯ"), "tokyo");
    }

    #[test]
    fn similar_names_match() {
        assert!(name_similarity("Tokyo Skytree", "Tokyo Sky Tree") >= 0.95);
        assert!(name_similarity("Arashiyama", "Arashiyama Bamboo Grove") >= 0.6);
        assert!(name_similarity("Blue Lagoon", "Eiffel Tower") < 0.1);
        assert_eq!(name_similarity("", "x"), 0.0);
    }

    #[test]
    fn hash_is_stable_across_formatting() {
        assert_eq!(stable_hash("Lake  Bled, Slovenia"), stable_hash("lake bled slovenia"));
        assert_ne!(stable_hash("Lake Bled"), stable_hash("Lake Como"));
    }

    #[test]
    fn countries() {
        assert_eq!(country_code(Some("Japan")).as_deref(), Some("JP"));
        assert_eq!(country_code(Some("USA")).as_deref(), Some("US"));
        assert_eq!(country_code(Some("jp")).as_deref(), Some("JP"));
        assert_eq!(country_code(Some("Atlantis")), None);
        assert_eq!(countries_in("Save this for your Japan trip!"), vec!["JP".to_string()]);
    }

    #[test]
    fn distance() {
        // Kyoto Station → Fushimi Inari ≈ 2.7 km
        let d = distance_m(34.9858, 135.7588, 34.9671, 135.7727);
        assert!((2_000.0..3_500.0).contains(&d));
    }
}
