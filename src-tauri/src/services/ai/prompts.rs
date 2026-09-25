//! Compact, stable system prompts. Keeping them byte-identical across requests also lets
//! DeepSeek's prompt cache discount the repeated prefix. Bump `AI_PROMPT_VERSION` on change.

use crate::models::{PlaceCategory, SourceType, TravelFactType};

use super::types::ScreenshotAiInput;

fn joined<T>(all: &[T], f: fn(&T) -> &'static str) -> String {
    all.iter().map(f).collect::<Vec<_>>().join("|")
}

pub fn extraction_system() -> String {
    format!(
        r#"You extract travel information from phone screenshots and Instagram Reels for a personal travel map.
Input: numbered lines — a screenshot's OCR text (may include app UI noise), or a Reel's timestamped audio transcript, caption and on-screen text — plus metadata, sometimes an image.
Travel-related = useful for planning or remembering travel: places to visit/eat/stay, activities, transport, itineraries, travel advice.
Reply with ONLY this JSON:
{{"is_travel_related":bool,"travel_confidence":0-1 probability it is travel-related,"reason":str,"source_type":"{sources}","creator":str|null,
"places":[{{"display_name":str,"alternative_names":[str],"city":str|null,"region":str|null,"country":str|null,
"category":"{categories}","place_confidence":0-1,"ambiguous":bool,"search_query":str,"has_useful_photo":bool,
"facts":[{{"type":"{facts}","text":str,"confidence":0-1,"source_quote":str|null,"source_lines":[int]}}]}}]}}
Rules: use only what the screenshot supports; never invent places, prices, hours, addresses or coordinates; null when unknown; don't guess a country without evidence; list every distinct place the source recommends or is about (0..n), not whole countries; places mentioned only as context (a departure station, a mountain seen from the main place) go into the main place's facts, not the places list; keep contradictory advice as separate facts; keep meaning exact; creator only if visible (e.g. @handle); ambiguous=true if the name could match several real places; search_query = "name, city, country" using only known parts. No text outside the JSON."#,
        sources = joined(SourceType::ALL, SourceType::as_str),
        categories = joined(PlaceCategory::ALL, PlaceCategory::as_str),
        facts = joined(TravelFactType::ALL, TravelFactType::as_str),
    )
}

pub const CLASSIFICATION_SYSTEM: &str = r#"Decide if a phone screenshot is useful for planning or remembering travel (places, stays, food spots, activities, transport, itineraries, travel tips). Input: numbered OCR lines and metadata.
Reply with ONLY: {"is_travel_related":bool,"confidence":0-1,"reason":str}"#;

pub const REGION_SYSTEM: &str = r#"Classify this image cropped from a phone screenshot.
Reply with ONLY: {"region_type":"photograph|map|interface|text|illustration|unknown","confidence":0-1,"is_useful_travel_photo":bool}
A useful travel photo shows a real place (landscape, building, food, room) with no app UI or map."#;

pub const SUMMARY_SYSTEM: &str = r#"Using ONLY the numbered saved facts provided (no outside knowledge), write one or two short sentences explaining why the user saved this place, starting "You saved this place". Do not add details that are not in the facts.
Reply with ONLY: {"summary":str,"used_fact_ids":[str]}"#;

pub fn screenshot_payload(input: &ScreenshotAiInput) -> String {
    let mut out: Vec<String> = Vec::new();
    if let Some(date) = &input.creation_date {
        out.push(format!("date: {}", date.chars().take(10).collect::<String>()));
    }
    if let Some(app) = &input.source_hint {
        out.push(format!("app_hint: {app}"));
    }
    if let Some(creator) = &input.creator_hint {
        out.push(format!("creator_hint: {creator}"));
    }
    if input.image_jpeg.is_some() {
        out.push("image: attached".into());
    }
    out.push("ocr:".into());
    if input.lines.is_empty() {
        out.push("(no text)".into());
    }
    out.extend(input.lines.iter().enumerate().map(|(i, l)| format!("[{i}] {l}")));
    out.join("\n")
}

pub fn summary_payload(name: &str, facts: &[(String, String)]) -> String {
    let mut out = vec![format!("place: {name}"), "facts:".to_string()];
    out.extend(facts.iter().map(|(id, text)| format!("[{id}] {text}")));
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_is_compact_and_numbered() {
        let input = ScreenshotAiInput {
            lines: vec!["Lake Kawaguchi".into(), "Best view at sunrise".into()],
            creation_date: Some("2025-04-02T10:00:00Z".into()),
            source_hint: Some("instagram".into()),
            ..Default::default()
        };
        let p = screenshot_payload(&input);
        assert_eq!(p, "date: 2025-04-02\napp_hint: instagram\nocr:\n[0] Lake Kawaguchi\n[1] Best view at sunrise");
    }

    #[test]
    fn system_prompt_lists_every_category() {
        let s = extraction_system();
        assert!(s.contains("religiousSite") && s.contains("recommendedTime") && s.contains("googleMaps"));
        assert!(s.len() < 2400, "keep the system prompt compact");
    }
}
