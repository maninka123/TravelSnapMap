import type { PersonalStatus, PlaceCategory, ProcessingStatus, SourceType, TravelFactType } from "../api/types";

export const CATEGORY: Record<PlaceCategory, { label: string; color: string }> = {
  attraction: { label: "Attraction", color: "#f08c00" },
  restaurant: { label: "Restaurant", color: "#e03131" },
  cafe: { label: "Café", color: "#c2255c" },
  hotel: { label: "Hotel", color: "#1c7ed6" },
  accommodation: { label: "Accommodation", color: "#1971c2" },
  viewpoint: { label: "Viewpoint", color: "#e8590c" },
  nature: { label: "Nature", color: "#2f9e44" },
  beach: { label: "Beach", color: "#15aabf" },
  hiking: { label: "Hiking", color: "#5c940d" },
  temple: { label: "Temple", color: "#9c36b5" },
  religiousSite: { label: "Religious site", color: "#862e9c" },
  museum: { label: "Museum", color: "#6741d9" },
  historicSite: { label: "Historic site", color: "#a8662b" },
  shopping: { label: "Shopping", color: "#d6336c" },
  activity: { label: "Activity", color: "#0ca678" },
  nightlife: { label: "Nightlife", color: "#ae3ec9" },
  transport: { label: "Transport", color: "#495057" },
  airport: { label: "Airport", color: "#343a40" },
  station: { label: "Station", color: "#495057" },
  food: { label: "Food", color: "#f03e3e" },
  city: { label: "City", color: "#3b5bdb" },
  region: { label: "Region", color: "#364fc7" },
  other: { label: "Other", color: "#868e96" },
};

export const STATUS: Record<PersonalStatus, { label: string }> = {
  wantToVisit: { label: "Want to visit" },
  maybe: { label: "Maybe" },
  visited: { label: "Visited" },
  favourite: { label: "Favourite" },
  notInterested: { label: "Not interested" },
};

export const FACT: Record<TravelFactType, { label: string }> = {
  generalTip: { label: "Tips" },
  recommendedTime: { label: "Best time" },
  bestSeason: { label: "Best season" },
  openingHours: { label: "Opening hours" },
  price: { label: "Price" },
  reservation: { label: "Booking" },
  transportation: { label: "Getting there" },
  duration: { label: "Duration" },
  photography: { label: "Photography" },
  food: { label: "Food" },
  warning: { label: "Warning" },
  accessibility: { label: "Accessibility" },
  accommodation: { label: "Accommodation" },
  activity: { label: "Things to do" },
  route: { label: "Route" },
  ticket: { label: "Tickets" },
  nearby: { label: "Nearby" },
  itinerary: { label: "Itinerary" },
  other: { label: "Other" },
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
  { key: "time", title: "Best time", types: ["recommendedTime", "bestSeason", "openingHours", "duration"] },
  { key: "getting", title: "Getting there", types: ["transportation", "route"] },
  { key: "cost", title: "Tickets & cost", types: ["price", "ticket", "reservation"] },
  { key: "food", title: "Food", types: ["food"] },
  { key: "photo", title: "Photography", types: ["photography"] },
  { key: "todo", title: "Things to do", types: ["activity"] },
  { key: "stay", title: "Stay", types: ["accommodation"] },
  { key: "warnings", title: "Warnings", types: ["warning", "accessibility"] },
  { key: "tips", title: "Local tips", types: ["generalTip", "other"] },
  { key: "nearby", title: "Nearby saved places", types: ["nearby"] },
  { key: "itinerary", title: "Itinerary", types: ["itinerary"] },
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
