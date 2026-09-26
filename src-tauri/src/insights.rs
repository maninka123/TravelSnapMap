//! Freshness and conflict warnings, computed locally from the structured facts already in the library
//! (no AI call).

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;

use crate::db::{Database, FactRecord, PlaceFilter};
use crate::models::{PlaceCategory, TravelFactType};
use crate::text::normalize;

use TravelFactType as F;

fn padded(s: &str) -> String {
    format!(" {} ", s)
}

/// `phrase` appears in `hay` at the start of a word ("dusk" matches "dusky", never the middle of a word).
fn has_word_prefix(hay_padded: &str, phrase: &str) -> bool {
    hay_padded.contains(&format!(" {phrase}"))
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

