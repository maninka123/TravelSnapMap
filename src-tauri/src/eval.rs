//! Accuracy evaluation on the hand-labelled datasets in `src-tauri/eval/`. Measures the deterministic parts of the
//! pipeline (on-device travel filter, place-match scoring and decision, duplicate detection) and enforces quality
//! gates. The AI extraction itself is not covered here (it needs DeepSeek; see the opt-in live tests).
//! Run with `cargo test --lib eval -- --nocapture` to print the report.

use serde::Deserialize;
use serde_json::Value;

use crate::config::AppConfig;
use crate::db::Database;
use crate::models::{DataOrigin, PlaceCandidate, PlaceCategory, Verification};
use crate::pipeline::merge::{find_match, PlaceMatch};
use crate::services::ai::local_filter::{analyze, route, Route};
use crate::services::ai::types::ExtractedPlace;
use crate::services::places::scoring::{decide, rank, Decision, ResolutionContext};

fn load(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("eval").join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 1.0 } else { n as f64 / d as f64 }
}

#[test]
fn eval_travel_filter() {
    let data = load("travel_detection.json");
    let config = AppConfig::default();
    let (mut travel, mut travel_sent, mut other, mut other_skipped) = (0, 0, 0, 0);
    let mut missed = vec![];
    for item in data["items"].as_array().unwrap() {
        let text = item["text"].as_str().unwrap();
        let skipped = route(&analyze(text), false, &config) == Route::SkipNotTravel;
        if item["travel"].as_bool().unwrap() {
            travel += 1;
            if skipped { missed.push(item["id"].as_str().unwrap().to_string()); } else { travel_sent += 1; }
        } else {
            other += 1;
            if skipped { other_skipped += 1; }
        }
    }
    let recall = pct(travel_sent, travel);
    let skip_precision = pct(other_skipped, other_skipped + (travel - travel_sent));
    println!("EVAL travel filter: travel posts kept for AI {travel_sent}/{travel} ({:.0}%), non-travel skipped on device {other_skipped}/{other} ({:.0}%), \
              skip precision {:.0}%, missed {missed:?}", recall * 100.0, pct(other_skipped, other) * 100.0, skip_precision * 100.0);
    // Gate: a travel post skipped on the device is lost for good, so recall matters most.
    assert!(recall >= 0.95, "travel recall {recall:.2} below gate 0.95 (missed {missed:?})");
    assert!(skip_precision >= 0.95, "skip precision {skip_precision:.2} below gate 0.95");
}

#[derive(Deserialize)]
struct EvalCandidate {
    name: String, lat: f64, lon: f64,
    #[serde(default)] city: Option<String>, #[serde(default)] country: Option<String>, #[serde(default)] cc: Option<String>,
    #[serde(default)] near: Option<f64>, #[serde(default)] category: Option<String>, #[serde(default)] address: Option<String>,
}

#[test]
fn eval_place_matching() {
    let data = load("place_matching.json");
    let config = AppConfig::default();
    let (mut cases, mut answerable, mut top1, mut auto, mut auto_right, mut wrong_confirmed, mut review, mut no_answer_safe, mut no_answer) =
        (0, 0, 0, 0, 0, 0, 0, 0, 0);
    let mut wrong = vec![];
    for item in data["items"].as_array().unwrap() {
        cases += 1;
        let extracted: ExtractedPlace = serde_json::from_value(item["extracted"].clone()).unwrap();
        let cands: Vec<PlaceCandidate> = serde_json::from_value::<Vec<EvalCandidate>>(item["candidates"].clone()).unwrap().into_iter()
            .map(|c| PlaceCandidate {
                name: c.name, latitude: c.lat, longitude: c.lon, city: c.city, country: c.country, country_code: c.cc,
                near_distance_km: c.near, category: c.category, address: c.address, ..Default::default()
            }).collect();
        let expected = item["expected"].as_u64().map(|i| cands[i as usize].name.clone());
        let ranked = rank(&cands, &extracted, &ResolutionContext::default());
        let result = decide(&ranked, &extracted, &config);
        let chosen = result.candidate.as_ref().map(|c| c.name.clone());
        match &expected {
            Some(name) => {
                answerable += 1;
                if ranked.first().map(|c| &c.name) == Some(name) { top1 += 1; }
                if result.decision == Decision::AutoAccept {
                    auto += 1;
                    if chosen.as_ref() == Some(name) { auto_right += 1; } else { wrong_confirmed += 1; wrong.push(item["id"].clone()); }
                } else if result.decision == Decision::Review { review += 1; }
            }
            None => {
                no_answer += 1;
                if result.decision == Decision::AutoAccept { wrong_confirmed += 1; wrong.push(item["id"].clone()); } else { no_answer_safe += 1; }
            }
        }
    }
    println!("EVAL place matching ({cases} cases): top-1 correct {top1}/{answerable} ({:.0}%), confirmed automatically {auto}/{answerable} ({:.0}%) \
              of which right {auto_right}/{auto}, sent to review {review}, no-right-answer cases not auto-confirmed {no_answer_safe}/{no_answer}, \
              wrong places confirmed automatically {wrong_confirmed}/{cases} ({:.1}%) {wrong:?}",
             pct(top1, answerable) * 100.0, pct(auto, answerable) * 100.0, pct(wrong_confirmed, cases) * 100.0);
    // Gates: a wrong pin confirmed without asking is the costliest mistake; ranking must usually be right.
    assert!(pct(wrong_confirmed, cases) <= 0.05, "wrong auto-confirmations {wrong_confirmed}/{cases} above gate 5%: {wrong:?}");
    assert!(pct(top1, answerable) >= 0.85, "top-1 accuracy {top1}/{answerable} below gate 85%");
}

#[test]
fn eval_duplicates() {
    let data = load("duplicates.json");
    let (mut tp, mut fp, mut fn_, mut tn) = (0, 0, 0, 0);
    let mut errors = vec![];
    for item in data["items"].as_array().unwrap() {
        let db = Database::open_in_memory().unwrap();
        let e = &item["existing"];
        let existing = PlaceCandidate {
            name: e["name"].as_str().unwrap().into(), latitude: e["lat"].as_f64().unwrap(), longitude: e["lon"].as_f64().unwrap(),
            map_identifier: e["map_id"].as_str().map(Into::into), ..Default::default()
        };
        let alt: Vec<String> = e["alt"].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect()).unwrap_or_default();
        db.insert_place(&existing, PlaceCategory::Other, Verification::Verified, DataOrigin::MapKit, &alt).unwrap();
        let c = &item["candidate"];
        let cand = PlaceCandidate {
            name: c["name"].as_str().unwrap().into(), latitude: c["lat"].as_f64().unwrap(), longitude: c["lon"].as_f64().unwrap(),
            map_identifier: c["map_id"].as_str().map(Into::into), ..Default::default()
        };
        let merged = matches!(find_match(&db, &cand, &[]).unwrap(), PlaceMatch::Same(_));
        match (item["same"].as_bool().unwrap(), merged) {
            (true, true) => tp += 1,
            (false, false) => tn += 1,
            (false, true) => { fp += 1; errors.push(item["id"].clone()); }
            (true, false) => { fn_ += 1; errors.push(item["id"].clone()); }
        }
    }
    let precision = pct(tp, tp + fp);
    let recall = pct(tp, tp + fn_);
    println!("EVAL duplicates: merge precision {:.0}% ({tp}/{}), recall {:.0}% ({tp}/{}), kept apart correctly {tn}/{}, errors {errors:?}",
             precision * 100.0, tp + fp, recall * 100.0, tp + fn_, tn + fp);
    // Gate: a wrong merge mixes two places' tips and is hard to notice; missing a merge only leaves a duplicate.
    assert!(precision >= 0.95, "merge precision {precision:.2} below gate 0.95: {errors:?}");
    assert!(recall >= 0.6, "merge recall {recall:.2} below gate 0.6: {errors:?}");
}
