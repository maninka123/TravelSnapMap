//! Replaceable service interfaces:
//! - `native`: PhotoLibraryService, OcrService, VisionService (Swift bridge: PhotoKit + Vision)
//! - `ai`: TravelAIService (DeepSeek) + local filter
//! - `places`: PlaceService over a PlaceSearchProvider (MapKit via the bridge)

pub mod ai;
pub mod folder;
pub mod native;
pub mod places;
pub mod reels;
