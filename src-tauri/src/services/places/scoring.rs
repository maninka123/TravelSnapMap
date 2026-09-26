//! Candidate scoring for place resolution. Pure functions — fully unit-testable.

use serde::Serialize;

use crate::config::AppConfig;
use crate::models::PlaceCandidate;
use crate::services::ai::types::ExtractedPlace;
use crate::text::{best_similarity, country_code, distance_m, normalize};

/// Other confidently resolved places from the same screenshot ("7 places in Kyoto").
#[derive(Debug, Clone, Default)]
pub struct ResolutionContext {
    pub anchors: Vec<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    /// High confidence: create/attach a verified pin automatically.
    AutoAccept,
    /// Plausible: store a provisional place and ask the user to confirm.
    Review,
    /// Low confidence: no pin; show candidates in the Review Inbox.
    Reject,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceResolutionResult {
    pub candidate: Option<PlaceCandidate>,
    pub confidence: f64,
    pub alternatives: Vec<PlaceCandidate>,
    pub reason: String,
    pub decision: Decision,
}

/// Scores and sorts candidates for an extracted place (never trusts the first result blindly).
pub fn rank(candidates: &[PlaceCandidate], place: &ExtractedPlace, ctx: &ResolutionContext) -> Vec<PlaceCandidate> {
    let names = place.names();
    let wanted_country = country_code(place.country.as_deref());
    let wanted_group = place.place_category().group();

    let mut ranked: Vec<PlaceCandidate> = candidates
        .iter()
        .enumerate()
        .map(|(index, c)| {
            let name_sim = best_similarity(&c.name, &names);
            let mut score = 0.6 * name_sim;

            let near = c.near_distance_km;
            if place.country.as_deref().map(str::trim).is_some_and(|s| !s.is_empty()) {
                let known = c.country_code.as_deref().filter(|s| !s.is_empty()).is_some() || c.country.as_deref().is_some_and(|s| !s.trim().is_empty());
                if known {
                    let matches = match (&wanted_country, &c.country_code) {
                        (Some(w), Some(have)) => w.eq_ignore_ascii_case(have),
                        _ => normalize(place.country.as_deref().unwrap_or("")) == normalize(c.country.as_deref().unwrap_or("")),
                    };
                    score += if matches { 0.25 } else { -0.3 };
                } else if near.is_some_and(|d| d < 1500.0) {
                    // Maps omitted the country, but the result is inside the area we searched.
                    score += 0.2;
                }
            } else {
                score += 0.1;
            }

            if let Some(city) = place.city.as_deref().map(normalize).filter(|c| !c.is_empty()) {
                let haystack = normalize(&[c.city.clone(), c.region.clone(), c.address.clone()].into_iter().flatten().collect::<Vec<_>>().join(" "));
                if haystack.contains(&city) {
                    score += 0.15;
                } else if let Some(d) = near {
                    // Distance from the city we searched around: close is good, far is a different place.
                    // (Real case: "Kameyama" in a Kyoto itinerary matched Kameyama city ~70 km away.)
                    if d < 25.0 { score += 0.1 } else if d > 40.0 { score -= 0.25 }
                } else if c.city.as_deref().is_some_and(|x| !x.trim().is_empty()) {
                    score -= 0.1;
                }
            }

            if let Some(hint) = c.category_hint() {
                let g = hint.group();
                if g == wanted_group {
                    score += 0.05;
                } else if name_sim < 0.95 && !matches!(g, "any" | "area") && !matches!(wanted_group, "any" | "area") {
                    score -= 0.1;
                }
            }

            if let Some(nearest) = ctx.anchors.iter().map(|(lat, lon)| distance_m(*lat, *lon, c.latitude, c.longitude)).reduce(f64::min) {
                if nearest < 50_000.0 {
                    score += 0.05;
                } else if nearest > 1_000_000.0 {
                    score -= 0.15;
                }
            }

            if index == 0 {
                score += 0.05;
            }
            // Matching context can't rescue a result whose name is unrelated.
            if name_sim < 0.3 {
                score = score.min(0.3);
            }
            PlaceCandidate { score: score.clamp(0.0, 1.0), ..c.clone() }
        })
        .collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
    ranked
}

/// Applies the configured thresholds to ranked candidates.
pub fn decide(ranked: &[PlaceCandidate], place: &ExtractedPlace, config: &AppConfig) -> PlaceResolutionResult {
    let Some(best) = ranked.first() else {
        return PlaceResolutionResult {
            candidate: None, confidence: 0.0, alternatives: vec![], decision: Decision::Reject,
            reason: format!("No map results for \"{}\"", place.map_query()),
        };
    };
    let runner_up = ranked.get(1);
    let gap = runner_up.map(|r| best.score - r.score).unwrap_or(1.0);
    let same_spot = runner_up.is_some_and(|r| distance_m(best.latitude, best.longitude, r.latitude, r.longitude) < 300.0);
    let clear_winner = gap >= 0.15 || same_spot;
    let ai_sure = place.place_confidence.unwrap_or(0.8) >= 0.5 && !(place.ambiguous == Some(true) && gap < 0.3);
    let alternatives: Vec<PlaceCandidate> = ranked.iter().take(5).cloned().collect();

    let (decision, reason) = if best.score >= config.place_auto_accept_score && clear_winner && ai_sure {
        (Decision::AutoAccept, format!("Strong match ({:.0}%)", best.score * 100.0))
    } else if best.score >= config.place_review_score {
        let why = if !clear_winner {
            "several places match"
        } else if !ai_sure {
            "the screenshot is ambiguous"
        } else {
            "match is not certain"
        };
        (Decision::Review, format!("Possible match ({:.0}%), {why}", best.score * 100.0))
    } else {
        (Decision::Reject, format!("Weak matches only ({:.0}%)", best.score * 100.0))
    };

    PlaceResolutionResult {
        candidate: (decision != Decision::Reject).then(|| best.clone()),
        confidence: best.score,
        alternatives,
        reason,
        decision,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(name: &str, lat: f64, lon: f64, country: &str, code: &str, city: &str) -> PlaceCandidate {
        PlaceCandidate {
            name: name.into(), latitude: lat, longitude: lon, country: Some(country.into()),
            country_code: Some(code.into()), city: Some(city.into()), ..Default::default()
        }
    }

    fn extracted(name: &str, city: Option<&str>, country: Option<&str>) -> ExtractedPlace {
        ExtractedPlace { city: city.map(Into::into), country: country.map(Into::into), ..ExtractedPlace::named(name) }
    }

    #[test]
    fn country_context_picks_the_right_blue_lagoon() {
        let cands = vec![
            cand("Blue Lagoon", 35.95, 14.33, "Malta", "MT", "Comino"),
            cand("Blue Lagoon", 63.88, -22.45, "Iceland", "IS", "Grindavík"),
        ];
        let ranked = rank(&cands, &extracted("Blue Lagoon", None, Some("Iceland")), &ResolutionContext::default());
        assert_eq!(ranked[0].country_code.as_deref(), Some("IS"));
        let result = decide(&ranked, &extracted("Blue Lagoon", None, Some("Iceland")), &AppConfig::default());
        assert_eq!(result.decision, Decision::AutoAccept);
    }

    #[test]
    fn same_name_without_context_goes_to_review() {
        let cands = vec![
            cand("Blue Lagoon", 35.95, 14.33, "Malta", "MT", "Comino"),
            cand("Blue Lagoon", 63.88, -22.45, "Iceland", "IS", "Grindavík"),
            cand("Blue Lagoon", 18.17, -76.39, "Jamaica", "JM", "Port Antonio"),
        ];
        let place = extracted("Blue Lagoon", None, None);
        let result = decide(&rank(&cands, &place, &ResolutionContext::default()), &place, &AppConfig::default());
        assert_eq!(result.decision, Decision::Review);
        assert_eq!(result.alternatives.len(), 3);
    }

    #[test]
    fn unrelated_results_are_rejected() {
        let place = extracted("Secret Noodle Bar", Some("Kyoto"), Some("Japan"));
        let cands = vec![cand("Kyoto Station", 34.98, 135.75, "Japan", "JP", "Kyoto")];
        let result = decide(&rank(&cands, &place, &ResolutionContext::default()), &place, &AppConfig::default());
        assert_eq!(result.decision, Decision::Reject);
        assert!(result.candidate.is_none());
        assert!(decide(&[], &place, &AppConfig::default()).alternatives.is_empty());
    }

    #[test]
    fn proximity_to_other_places_in_the_screenshot_breaks_ties() {
        let cands = vec![
            cand("Nishiki Market", 43.0, -79.0, "Canada", "CA", "Toronto"),
            cand("Nishiki Market", 35.005, 135.765, "Japan", "JP", "Kyoto"),
        ];
        let ctx = ResolutionContext { anchors: vec![(34.967, 135.772)] }; // Fushimi Inari
        let ranked = rank(&cands, &extracted("Nishiki Market", None, None), &ctx);
        assert_eq!(ranked[0].country_code.as_deref(), Some("JP"));
    }

    #[test]
    fn ambiguous_flag_from_ai_prevents_auto_accept() {
        let mut place = extracted("Shibuya Sky", Some("Tokyo"), Some("Japan"));
        place.ambiguous = Some(true);
        let cands = vec![
            cand("Shibuya Sky", 35.658, 139.702, "Japan", "JP", "Tokyo"),
            cand("Shibuya Sky Deck", 35.66, 139.70, "Japan", "JP", "Tokyo"),
        ];
        let mut cands2 = cands.clone();
        cands2[1].latitude = 35.70; // far enough not to be the same spot
        let result = decide(&rank(&cands2, &place, &ResolutionContext::default()), &place, &AppConfig::default());
        assert_ne!(result.decision, Decision::AutoAccept);
    }
}

#[cfg(test)]
mod real_cases {
    use super::*;

    /// Real case: Maps returned the exact place without a country, categorised as a restaurant.
    #[test]
    fn exact_name_near_the_hint_without_country_is_accepted() {
        let mut place = ExtractedPlace::named("Matterhorn Glacier Paradise");
        place.city = Some("Zermatt".into());
        place.country = Some("Switzerland".into());
        place.category = Some("viewpoint".into());
        let cand = PlaceCandidate {
            name: "Matterhorn Glacier Paradise".into(), latitude: 46.0148, longitude: 7.7429,
            category: Some("Restaurant".into()), near_distance_km: Some(6.0), ..Default::default()
        };
        let result = decide(&rank(&[cand], &place, &ResolutionContext::default()), &place, &AppConfig::default());
        assert_eq!(result.decision, Decision::AutoAccept, "confidence {}", result.confidence);
    }

    /// Real case: the official name is longer than the one in the screenshot.
    #[test]
    fn short_name_inside_official_name() {
        let mut place = ExtractedPlace::named("Wangxian Valley");
        place.country = Some("China".into());
        let cand = PlaceCandidate {
            name: "Wangxian Valley Qingchuanxingguan - Wangxian Valley Scenic Area".into(), latitude: 28.8, longitude: 117.8,
            country: Some("China".into()), country_code: Some("CN".into()), ..Default::default()
        };
        let result = decide(&rank(&[cand], &place, &ResolutionContext::default()), &place, &AppConfig::default());
        assert_eq!(result.decision, Decision::AutoAccept, "confidence {}", result.confidence);
    }
}

#[cfg(test)]
mod real_reel_cases {
    use super::*;

    /// Real case: "Arashiyu Gion Foot Spa" (Kyoto) was matched to Gion in Hiroshima.
    #[test]
    fn same_name_in_another_city_is_not_accepted() {
        let mut place = ExtractedPlace::named("Arashiyu Gion Foot Spa");
        place.city = Some("Kyoto".into());
        place.country = Some("Japan".into());
        let cand = PlaceCandidate {
            name: "Gion".into(), latitude: 34.4, longitude: 132.46, city: Some("Hiroshima".into()),
            country: Some("Japan".into()), country_code: Some("JP".into()), near_distance_km: Some(300.0), ..Default::default()
        };
        let result = decide(&rank(&[cand], &place, &ResolutionContext::default()), &place, &AppConfig::default());
        assert_eq!(result.decision, Decision::Reject, "confidence {}", result.confidence);
    }
}

#[cfg(test)]
mod real_itinerary_cases {
    use super::*;

    /// Real case: a Kyoto itinerary's "Kameyama" (a park in Arashiyama) matched Kameyama city in Mie (~70 km).
    #[test]
    fn same_named_town_outside_the_city_is_not_auto_accepted() {
        let mut place = ExtractedPlace::named("Kameyama");
        place.city = Some("Kyoto".into());
        place.country = Some("Japan".into());
        let cand = PlaceCandidate {
            name: "Kameyama".into(), latitude: 34.856, longitude: 136.45, city: Some("Kameyama".into()),
            country: Some("Japan".into()), country_code: Some("JP".into()), near_distance_km: Some(70.0), ..Default::default()
        };
        let result = decide(&rank(&[cand], &place, &ResolutionContext::default()), &place, &AppConfig::default());
        assert_ne!(result.decision, Decision::AutoAccept, "confidence {}", result.confidence);
    }
}
