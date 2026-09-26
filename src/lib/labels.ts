import type { PersonalStatus, PlaceCategory, ProcessingStatus, SourceType, TravelFactType } from "../api/types";

export const CATEGORY: Record<PlaceCategory, { label: string; emoji: string; color: string }> = {
  attraction: { label: "Attraction", emoji: "⭐", color: "#f08c00" },
  restaurant: { label: "Restaurant", emoji: "🍽️", color: "#e03131" },
  cafe: { label: "Café", emoji: "☕", color: "#c2255c" },
  hotel: { label: "Hotel", emoji: "🛏️", color: "#1c7ed6" },
  accommodation: { label: "Accommodation", emoji: "🏠", color: "#1971c2" },
  viewpoint: { label: "Viewpoint", emoji: "🔭", color: "#e8590c" },
  nature: { label: "Nature", emoji: "🌿", color: "#2f9e44" },
  beach: { label: "Beach", emoji: "🏖️", color: "#15aabf" },
  hiking: { label: "Hiking", emoji: "🥾", color: "#5c940d" },
  temple: { label: "Temple", emoji: "⛩️", color: "#9c36b5" },
  religiousSite: { label: "Religious site", emoji: "🕌", color: "#862e9c" },
  museum: { label: "Museum", emoji: "🏛️", color: "#6741d9" },
  historicSite: { label: "Historic site", emoji: "🏰", color: "#a8662b" },
  shopping: { label: "Shopping", emoji: "🛍️", color: "#d6336c" },
  activity: { label: "Activity", emoji: "🎟️", color: "#0ca678" },
  nightlife: { label: "Nightlife", emoji: "🍸", color: "#ae3ec9" },
  transport: { label: "Transport", emoji: "🚆", color: "#495057" },
  airport: { label: "Airport", emoji: "✈️", color: "#343a40" },
  station: { label: "Station", emoji: "🚉", color: "#495057" },
  food: { label: "Food", emoji: "🍜", color: "#f03e3e" },
  city: { label: "City", emoji: "🏙️", color: "#3b5bdb" },
  region: { label: "Region", emoji: "🗺️", color: "#364fc7" },
  other: { label: "Other", emoji: "📍", color: "#868e96" },
};

export const STATUS: Record<PersonalStatus, { label: string; emoji: string }> = {
  wantToVisit: { label: "Want to Visit", emoji: "❤️" },
  maybe: { label: "Maybe", emoji: "📌" },
  visited: { label: "Visited", emoji: "✅" },
  favourite: { label: "Favourite", emoji: "⭐" },
  notInterested: { label: "Not Interested", emoji: "🚫" },
};

export const FACT: Record<TravelFactType, { label: string; emoji: string }> = {
  generalTip: { label: "Tips", emoji: "💡" },
  recommendedTime: { label: "Best time", emoji: "🕐" },
  bestSeason: { label: "Best season", emoji: "🍂" },
  openingHours: { label: "Opening hours", emoji: "🚪" },
  price: { label: "Price", emoji: "💰" },
  reservation: { label: "Booking", emoji: "📅" },
  transportation: { label: "Getting there", emoji: "🚆" },
  duration: { label: "Duration", emoji: "⏳" },
  photography: { label: "Photography", emoji: "📷" },
  food: { label: "Food", emoji: "🍜" },
  warning: { label: "Warning", emoji: "⚠️" },
  accessibility: { label: "Accessibility", emoji: "♿" },
  accommodation: { label: "Accommodation", emoji: "🛏️" },
  activity: { label: "Things to do", emoji: "🎯" },
  route: { label: "Route", emoji: "🧭" },
  ticket: { label: "Tickets", emoji: "🎟️" },
  nearby: { label: "Nearby", emoji: "📍" },
  itinerary: { label: "Itinerary", emoji: "🗓️" },
  other: { label: "Other", emoji: "📝" },
};

/** Facts whose value changes over time: shown as latest vs earlier saved values. */
export const TIME_SENSITIVE = new Set<TravelFactType>(["price", "openingHours", "ticket", "reservation"]);

export const SOURCE: Record<SourceType, string> = {
  instagram: "Instagram", tiktok: "TikTok", youtube: "YouTube", googleMaps: "Google Maps", appleMaps: "Apple Maps",
  booking: "Booking.com", tripadvisor: "Tripadvisor", airbnb: "Airbnb", reddit: "Reddit", safari: "Web",
  facebook: "Facebook", pinterest: "Pinterest", xiaohongshu: "RedNote", other: "Other", unknown: "Unknown",
};

export const PROCESSING: Record<ProcessingStatus, { label: string; tone: "ok" | "warn" | "bad" | "muted" | "busy" }> = {
  discovered: { label: "Queued", tone: "muted" },
  loading: { label: "Loading", tone: "busy" },
  ocrProcessing: { label: "Reading text", tone: "busy" },
  ocrComplete: { label: "Text read", tone: "busy" },
  classifying: { label: "Classifying", tone: "busy" },
  extracting: { label: "Extracting", tone: "busy" },
  resolvingPlaces: { label: "Finding places", tone: "busy" },
  extractingImages: { label: "Extracting photos", tone: "busy" },
  complete: { label: "Processed", tone: "ok" },
  notTravel: { label: "Not travel", tone: "muted" },
  needsReview: { label: "Needs review", tone: "warn" },
  waitingForNetwork: { label: "Waiting for network", tone: "warn" },
  needsMedia: { label: "Needs video", tone: "warn" },
  ignored: { label: "Ignored", tone: "muted" },
  failed: { label: "Failed", tone: "bad" },
};

export function formatDate(iso: string | null | undefined, withYear = true): string {
  if (!iso) return "Unknown date";
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short", ...(withYear ? { year: "numeric" } : {}) });
}

export function formatKm(km: number): string {
  return km < 1 ? `${Math.round(km * 1000)} m` : `${km.toFixed(km < 10 ? 1 : 0)} km`;
}

export function pct(v: number): string {
  return `${Math.round(v * 100)}%`;
}

/** Place page information cards. Only cards with facts are shown; each fact keeps its source. */
export const INFO_CARDS: { key: string; title: string; types: TravelFactType[] }[] = [
  { key: "time", title: "🕐 Best Time", types: ["recommendedTime", "bestSeason", "openingHours", "duration"] },
  { key: "getting", title: "🚆 Getting There", types: ["transportation", "route"] },
  { key: "cost", title: "🎟 Tickets & Cost", types: ["price", "ticket", "reservation"] },
  { key: "food", title: "🍜 Food", types: ["food"] },
  { key: "photo", title: "📸 Photography", types: ["photography"] },
  { key: "todo", title: "🎯 Things To Do", types: ["activity"] },
  { key: "stay", title: "🏨 Stay", types: ["accommodation"] },
  { key: "warnings", title: "⚠️ Warnings", types: ["warning", "accessibility"] },
  { key: "tips", title: "💡 Local Tips", types: ["generalTip", "other"] },
  { key: "nearby", title: "📍 Nearby Places", types: ["nearby"] },
  { key: "itinerary", title: "🗓 Itinerary", types: ["itinerary"] },
];

export function formatTime(sec: number | null | undefined): string {
  if (sec == null) return "";
  const s = Math.max(0, Math.floor(sec));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

export const SOURCE_KIND: Record<string, string> = {
  screenshot: "Screenshot",
  audio: "Audio",
  caption: "Caption",
  keyframe: "On-screen text",
  user: "Added by you",
};
