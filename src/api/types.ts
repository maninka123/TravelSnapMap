// Mirrors the Rust records (camelCase JSON). Keep in sync with src-tauri/src/db/records.rs.

export type PlaceCategory =
  | "attraction" | "restaurant" | "cafe" | "hotel" | "accommodation" | "viewpoint" | "nature" | "beach"
  | "hiking" | "temple" | "religiousSite" | "museum" | "historicSite" | "shopping" | "activity" | "nightlife"
  | "transport" | "airport" | "station" | "food" | "city" | "region" | "other";

export type PersonalStatus = "wantToVisit" | "maybe" | "visited" | "favourite" | "notInterested";

export type TravelFactType =
  | "generalTip" | "recommendedTime" | "bestSeason" | "openingHours" | "price" | "reservation"
  | "transportation" | "duration" | "photography" | "food" | "warning" | "accessibility"
  | "accommodation" | "activity" | "route" | "ticket" | "nearby" | "itinerary" | "other";

export type SourceType =
  | "instagram" | "tiktok" | "youtube" | "googleMaps" | "appleMaps" | "booking" | "tripadvisor" | "airbnb"
  | "reddit" | "safari" | "facebook" | "pinterest" | "xiaohongshu" | "other" | "unknown";

export type ProcessingStatus =
  | "discovered" | "loading" | "ocrProcessing" | "ocrComplete" | "classifying" | "extracting"
  | "resolvingPlaces" | "extractingImages" | "complete" | "notTravel" | "needsReview"
  | "waitingForNetwork" | "needsMedia" | "ignored" | "failed";

export type Verification = "verified" | "needsReview" | "userVerified";
export type DataOrigin = "ai" | "mapKit" | "local" | "user";
export type ReviewKind = "travelClassification" | "placeResolution" | "duplicatePlace" | "photoCrop" | "processingFailure";

export interface Rect { x: number; y: number; width: number; height: number }

export interface Place {
  id: string;
  canonicalName: string;
  alternativeNames: string[];
  mapIdentifier: string | null;
  latitude: number;
  longitude: number;
  address: string | null;
  city: string | null;
  region: string | null;
  country: string | null;
  countryCode: string | null;
  category: PlaceCategory;
  verification: Verification;
  origin: DataOrigin;
  isUserVerified: boolean;
  personalStatus: PersonalStatus;
  notes: string;
  summaryText: string | null;
  summaryFactIds: string[];
  summaryGeneratedAt: string | null;
  heroImageId: string | null;
  createdAt: string;
  updatedAt: string;
  sourceCount: number;
  heroImagePath: string | null;
  thumbnailPath: string | null;
  /** After the trip: when you went (YYYY-MM-DD), your own notes, your own photo as cover. */
  visitedAt: string | null;
  visitNotes: string;
  coverMemoryId: string | null;
  memoryCount: number;
}

export interface Screenshot {
  id: string;
  photosId: string;
  creationDate: string | null;
  width: number;
  height: number;
  processedAt: string | null;
  aiPromptVersion: number;
  status: ProcessingStatus;
  statusDetail: string | null;
  failureCount: number;
  classification: "unknown" | "travel" | "notTravel" | "uncertain";
  userClassification: string | null;
  travelConfidence: number;
  localTravelScore: number;
  sourceType: SourceType;
  creator: string | null;
  ocrFullText: string;
  imagePath: string | null;
  thumbnailPath: string | null;
  escalationLevel: number;
  aiModel: string | null;
  aiThinking: boolean;
  aiImageUsed: boolean;
  aiInputTokens: number;
  aiOutputTokens: number;
  aiLatencyMs: number;
  aiRetryCount: number;
  aiCost: number;
  placeCount: number;
  openReviewCount: number;
}

export interface OcrBlock {
  id: string;
  idx: number;
  text: string;
  confidence: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type FactSourceKind = "screenshot" | "audio" | "caption" | "keyframe" | "user";

export interface Fact {
  id: string;
  placeId: string;
  screenshotId: string | null;
  reelId: string | null;
  sourceKind: FactSourceKind;
  sourceTimeSec: number | null;
  type: TravelFactType;
  text: string;
  sourceQuote: string | null;
  confidence: number;
  sourceBlockIds: string[];
  validFrom: string | null;
  origin: DataOrigin;
  createdAt: string;
  sourceType: SourceType | null;
  creator: string | null;
}

export interface PlaceImage {
  id: string;
  placeId: string;
  screenshotId: string | null;
  crop: Rect;
  imagePath: string | null;
  qualityScore: number;
  regionType: string;
  regionConfidence: number;
  isAccepted: boolean;
  origin: DataOrigin;
  duplicateSourceIds: string[];
}

export interface Link {
  placeId: string;
  screenshotId: string;
  extractedName: string;
  confidence: number;
  origin: DataOrigin;
  isUserVerified: boolean;
}

export interface PlaceCandidate {
  name: string;
  latitude: number;
  longitude: number;
  address?: string | null;
  city?: string | null;
  region?: string | null;
  country?: string | null;
  countryCode?: string | null;
  mapIdentifier?: string | null;
  category?: string | null;
  score?: number;
}

export interface ExtractedPlace {
  display_name: string;
  city?: string | null;
  country?: string | null;
  category?: string | null;
}

export interface Review {
  id: string;
  kind: ReviewKind;
  screenshotId: string | null;
  reelId: string | null;
  message: string;
  extractedPlace: ExtractedPlace | null;
  candidates: PlaceCandidate[];
  placeAId: string | null;
  placeBId: string | null;
  imageId: string | null;
  isResolved: boolean;
  createdAt: string;
  screenshotThumbnail: string | null;
}

export interface ReviewEntry {
  review: Review;
  placeA: Place | null;
  placeB: Place | null;
  image: PlaceImage | null;
}

export interface Trip {
  id: string;
  name: string;
  startDate: string | null;
  endDate: string | null;
  notes: string;
  createdAt: string;
  placeCount: number;
}

export interface TripEntry {
  id: string;
  tripId: string;
  position: number;
  day: number | null;
  place: Place;
}

export interface PlaceDetail {
  place: Place;
  facts: Fact[];
  screenshots: Screenshot[];
  links: Link[];
  images: PlaceImage[];
  reels: Reel[];
  nearby: { place: Place; distanceKm: number }[];
  trips: Trip[];
  memories: Memory[];
  warnings: PlaceWarning[];
}

export interface ScreenshotDetail {
  screenshot: Screenshot;
  blocks: OcrBlock[];
  places: Place[];
  links: Link[];
  facts: Fact[];
  images: PlaceImage[];
  reviews: Review[];
}

export interface QueueSnapshot {
  running: boolean;
  paused: boolean;
  mode: string;
  runId: string | null;
  phase: string;
  inPhotos: number;
  alreadyKnown: number;
  newlyDiscovered: number;
  total: number;
  processed: number;
  travel: number;
  notTravel: number;
  needsReview: number;
  failed: number;
  waiting: number;
  remaining: number;
  lastError: string | null;
}

export interface Overview {
  screenshotCounts: Record<string, number>;
  openReviews: number;
  places: number;
  unconfirmedPlaces: number;
  queue: QueueSnapshot;
  model: string;
  aiConfigured: boolean;
  apiKeySource: string;
}

export interface AppConfig {
  model: string;
  baseUrl: string;
  useThinkingByDefault: boolean;
  allowThinkingEscalation: boolean;
  allowVisionRequests: boolean;
  maxOutputTokens: number;
  thinkingMaxOutputTokens: number;
  visionImageMaxPixelSize: number;
  requestTimeoutSecs: number;
  maxRetries: number;
  dailyAiRequestLimit: number;
  localSkipScore: number;
  minimumLocalConfidenceForSkippingAi: number;
  minimumAiConfidenceForAutoAcceptance: number;
  travelReviewThreshold: number;
  placeAutoAcceptScore: number;
  placeReviewScore: number;
  regionAutoAcceptConfidence: number;
  regionReviewConfidence: number;
  duplicateImageDistance: number;
  maxConcurrentScreenshots: number;
  scanFromYear: number | null;
  scanToYear: number | null;
  nearbyRadiusKm: number;
  transcriptionLocale: string;
  autoProcessNewScreenshots: boolean;
  screenshotFolders: string[];
  maxKeyframes: number;
  ytDlpPath: string;
  cookiesFromBrowser: string;
  pricing: Pricing;
}

export interface Settings {
  config: AppConfig;
  defaults: AppConfig;
  hasApiKey: boolean;
  apiKeySource: string;
}

export interface Diagnostics {
  totalScreenshots: number;
  travelScreenshots: number;
  notTravel: number;
  skippedLocally: number;
  needsReview: number;
  failed: number;
  remaining: number;
  places: number;
  aiRequests: number;
  aiTextRequests: number;
  aiVisionRequests: number;
  aiThinkingRequests: number;
  aiFailures: number;
  aiInputTokens: number;
  aiOutputTokens: number;
  aiCacheHitTokens: number;
  aiEstimatedCost: number;
  aiAverageLatencyMs: number;
  openReviews: number;
  metrics: [string, number][];
}

export interface PlaceFilter {
  search?: string;
  category?: string;
  status?: string;
  countryCode?: string;
  city?: string;
  verifiedOnly?: boolean;
  sort?: "recent" | "name" | "sources";
}

export type ScreenshotView =
  | "all" | "processed" | "needsReview" | "multiplePlaces" | "noPlace"
  | "ignored" | "notTravel" | "failed" | "pending" | "everything";

export interface TranscriptSegment { start: number; end: number; text: string }

export interface Keyframe {
  id: string;
  timeSec: number;
  imagePath: string;
  ocrText: string;
  isCover: boolean;
}

export interface Reel {
  id: string;
  url: string;
  shortcode: string | null;
  creator: string | null;
  caption: string | null;
  mediaPath: string | null;
  audioPath: string | null;
  thumbnailPath: string | null;
  durationSec: number | null;
  status: ProcessingStatus;
  statusDetail: string | null;
  classification: string;
  travelConfidence: number;
  transcript: TranscriptSegment[];
  transcriptLocale: string | null;
  mediaSource: string | null;
  aiModel: string | null;
  aiCost: number;
  createdAt: string;
  processedAt: string | null;
  placeCount: number;
  openReviewCount: number;
  stages: Partial<Record<ReelStage, { status: StageStatus; detail: string | null }>>;
  transcriptLocaleOverride: string | null;
  transcriptConfidence: number | null;
  placesExtracted: number;
  placesAutoResolved: number;
}

export type ReelStage = "caption" | "video" | "audio" | "transcript" | "keyframes" | "ocr" | "ai" | "places";
export type StageStatus = "pending" | "running" | "done" | "skipped" | "failed";

export interface ReelDetail {
  reel: Reel;
  keyframes: Keyframe[];
  places: Place[];
  facts: Fact[];
  images: PlaceImage[];
  reviews: Review[];
}

export interface PhotoVideo { id: string; creationDate: string | null; durationSec: number; width: number; height: number }

/** One entry of the combined Sources library (screenshots and Reels together). */
export type LibraryItem =
  | { kind: "screenshot"; date: string | null; screenshot: Screenshot }
  | { kind: "reel"; date: string | null; reel: Reel };

export type RunMode =
  | { kind: "scanNew" }
  | { kind: "validation"; count: number; random: boolean }
  | { kind: "retry" }
  | { kind: "scanFolder"; path: string };

export interface SourceSummary { total?: number; known?: number; new?: number; error?: string }
export interface ScanPreview { photos: SourceSummary; folders: { path: string; summary: SourceSummary }[] }

export interface RunRecord {
  id: string;
  kind: string;
  requested: number;
  sample: string;
  screenshotCount: number;
  startedAt: string;
  finishedAt: string | null;
}

export interface RunRow {
  screenshotId: string;
  thumbnailPath: string | null;
  creationDate: string | null;
  status: ProcessingStatus;
  statusDetail: string | null;
  sourceType: SourceType;
  travelConfidence: number;
  escalationLevel: number;
  places: string[];
  placesExtracted: number;
  placesAutoResolved: number;
  ocrMs: number;
  aiMs: number;
  pipelineMs: number;
  aiCost: number;
}

export interface RunReport {
  run: RunRecord;
  total: number;
  processed: number;
  travel: number;
  notTravel: number;
  failed: number;
  needsReview: number;
  waiting: number;
  pending: number;
  skippedLocally: number;
  aiRequests: number;
  placesFound: number;
  placesExtracted: number;
  placesAutoResolved: number;
  resolutionRate: number;
  avgOcrMs: number;
  avgAiMs: number;
  avgTotalMs: number;
  totalCost: number;
  costPerTravelScreenshot: number;
  inputTokens: number;
  outputTokens: number;
  rows: RunRow[];
}

export interface SpeechLocale { id: string; name: string; onDevice: boolean; engine: string }

export interface Rates { inputCacheHit: number; inputCacheMiss: number; output: number }
/** Estimated AI cost settings (DeepSeek bills peak and off-peak hours differently). */
export interface Pricing {
  mode: "timeOfDay" | "peak" | "offPeak";
  peak: Rates;
  offPeak: Rates;
  peakHoursUtc: [number, number][];
  source: string;
  /** Prices typed by the user; otherwise the model's standard prices are used automatically. */
  custom: boolean;
}

/** Estimated AI spend so far under a pricing setting, with every mode for comparison. */
export interface CostSummary {
  total: number;
  byTimeOfDay: number;
  alwaysPeak: number;
  alwaysOffPeak: number;
  screenshots: number;
  reels: number;
  other: number;
  requests: number;
  peakRequests: number;
  screenshotsProcessed: number;
  screenshotsRemaining: number;
  per100Screenshots: number;
  projectedRemaining: number;
}

/** "Ticket price saved 18 months ago", "Two sources give different opening times"… */
export interface PlaceWarning {
  kind: "stale" | "conflict";
  title: string;
  detail: string;
  factIds: string[];
}

/** One of your own photos attached to a visited place (the original stays in Photos). */
export interface Memory {
  id: string;
  placeId: string;
  photosId: string;
  takenAt: string | null;
  latitude: number | null;
  longitude: number | null;
  imagePath: string | null;
  thumbnailPath: string | null;
  createdAt: string;
}

/** A photo in your Photos library (not a screenshot), offered for "My visit". */
export interface OwnPhoto {
  id: string;
  creationDate: string | null;
  latitude?: number | null;
  longitude?: number | null;
  distanceM?: number | null;
}
