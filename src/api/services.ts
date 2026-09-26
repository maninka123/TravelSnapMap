// The only module that talks to the Tauri backend. Views use these services, never `invoke` directly.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppConfig, CostSummary, Diagnostics, Pricing, LibraryItem, PhotoVideo, ReelDetail, Reel, RunMode, RunRecord, RunReport, ScanPreview, SpeechLocale, Overview, PlaceCandidate, PlaceDetail, PlaceFilter, Place, QueueSnapshot, Rect,
  ReviewEntry, Screenshot, ScreenshotDetail, ScreenshotView, Settings, Trip, TripEntry,
} from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Local file (thumbnail, crop) → URL the webview can load. */
export function fileUrl(path: string | null | undefined): string | undefined {
  if (!path) return undefined;
  return isTauri ? convertFileSrc(path) : undefined;
}

export const AppService = {
  overview: () => invoke<Overview>("get_overview"),
  diagnostics: () => invoke<Diagnostics>("diagnostics"),
  costSummary: (pricing: Pricing) => invoke<CostSummary>("cost_summary", { pricing }),
  onLibraryChanged: (cb: () => void): Promise<UnlistenFn> => listen("library-changed", cb),
  /** Fired when background work (e.g. Reel import) changes stored data. */
  onDataChanged: (cb: () => void): Promise<UnlistenFn> => listen("data-changed", cb),
};

export const PhotoLibraryService = {
  permissionStatus: () => invoke<string>("photos_permission_status"),
  requestPermission: () => invoke<string>("request_photos_permission"),
};

export const ProcessingService = {
  /** Scan New Screenshots (Photos + folders), a validation run, retry, or one folder. */
  start: (mode: RunMode) => invoke<void>("start_processing", { mode }),
  scanPreview: () => invoke<ScanPreview>("scan_preview"),
  runs: () => invoke<RunRecord[]>("list_runs"),
  runReport: (id: string) => invoke<RunReport | null>("run_report", { id }),
  addFolder: (path: string) => invoke<void>("add_screenshot_folder", { path }),
  removeFolder: (path: string) => invoke<void>("remove_screenshot_folder", { path }),
  pause: () => invoke<void>("pause_processing"),
  resume: () => invoke<void>("resume_processing"),
  cancel: () => invoke<void>("cancel_processing"),
  status: () => invoke<QueueSnapshot>("processing_status"),
  reprocess: (scope: "failed" | "needsReview" | "outdated" | "all" | "selected", ids?: string[]) =>
    invoke<number>("reprocess", { scope, ids: ids ?? null }),
  onProgress: (cb: (s: QueueSnapshot) => void): Promise<UnlistenFn> =>
    listen<QueueSnapshot>("processing-progress", (e) => cb(e.payload)),
};

export const PlaceService = {
  list: (filter: PlaceFilter = {}) => invoke<Place[]>("list_places", { filter }),
  detail: (id: string) => invoke<PlaceDetail>("get_place_detail", { id }),
  update: (id: string, field: "canonicalName" | "category" | "personalStatus" | "notes" | "heroImageId", value: string | null) =>
    invoke<void>("update_place", { id, field, value }),
  setLocation: (id: string, candidate: PlaceCandidate) => invoke<void>("set_place_location", { id, candidate }),
  remove: (id: string) => invoke<void>("delete_place", { id }),
  merge: (sourceId: string, targetId: string) => invoke<void>("merge_places", { sourceId, targetId }),
  split: (placeId: string, screenshotIds: string[], candidate: PlaceCandidate) =>
    invoke<string>("split_place", { placeId, screenshotIds, candidate }),
  removeScreenshot: (placeId: string, screenshotId: string) => invoke<void>("remove_place_screenshot", { placeId, screenshotId }),
  deleteFact: (id: string) => invoke<void>("delete_fact", { id }),
  removeImage: (id: string) => invoke<void>("remove_image", { id }),
  summarize: (id: string) => invoke<string>("summarize_place", { id }),
  /** Map provider search — coordinates always come from here, never from AI. */
  searchMap: (query: string) => invoke<PlaceCandidate[]>("search_map", { query }),
};

export const ScreenshotService = {
  list: (view: ScreenshotView, search?: string, limit = 400) =>
    invoke<Screenshot[]>("list_screenshots", { filter: { view, search: search || null, limit } }),
  detail: (id: string) => invoke<ScreenshotDetail>("get_screenshot_detail", { id }),
  action: (id: string, action: "reprocess" | "ignore" | "markTravel" | "markNotTravel") =>
    invoke<void>("screenshot_action", { id, action }),
  addPlace: (screenshotId: string, candidate: PlaceCandidate) => invoke<string>("add_place_to_screenshot", { screenshotId, candidate }),
  correctPlace: (screenshotId: string, wrongPlaceId: string, candidate: PlaceCandidate) =>
    invoke<string>("correct_place", { screenshotId, wrongPlaceId, candidate }),
  saveCrop: (screenshotId: string, placeId: string, rect: Rect) => invoke<string>("save_manual_crop", { screenshotId, placeId, rect }),
};

export const ReviewService = {
  list: () => invoke<ReviewEntry[]>("list_reviews"),
  resolve: (id: string, action: string, candidate?: PlaceCandidate) =>
    invoke<void>("resolve_review", { id, action, candidate: candidate ?? null }),
};

export const TripService = {
  list: () => invoke<Trip[]>("list_trips"),
  create: (name: string) => invoke<string>("create_trip", { name }),
  update: (id: string, name: string, startDate: string | null, endDate: string | null, notes: string) =>
    invoke<void>("update_trip", { id, name, startDate, endDate, notes }),
  remove: (id: string) => invoke<void>("delete_trip", { id }),
  entries: (tripId: string) => invoke<TripEntry[]>("trip_entries", { tripId }),
  addPlace: (tripId: string, placeId: string) => invoke<void>("add_to_trip", { tripId, placeId }),
  updateEntry: (entryId: string, day: number | null, position: number | null) =>
    invoke<void>("update_trip_entry", { entryId, day, position }),
  removeEntry: (entryId: string) => invoke<void>("remove_trip_entry", { entryId }),
};

export const SettingsService = {
  get: () => invoke<Settings>("get_settings"),
  save: (config: AppConfig) => invoke<void>("save_settings", { config }),
  saveApiKey: (key: string | null) => invoke<void>("save_api_key", { key }),
};

export const ReelService = {
  list: () => invoke<Reel[]>("list_reels"),
  detail: (id: string) => invoke<ReelDetail>("get_reel_detail", { id }),
  /** Paste an Instagram Reel/Post URL. Processing continues in the background. */
  importUrl: (url: string) => invoke<string>("import_reel", { url }),
  /** Every Instagram link in pasted text — processed one after another in the background. */
  importMany: (text: string) => invoke<BulkReelImport>("import_reels", { text }),
  /** Every Instagram link inside a text file (.txt, .md, .csv…). */
  importFile: (path: string) => invoke<BulkReelImport>("import_reels_from_file", { path }),
  /** Fallback when Instagram media isn't accessible: use a video saved in Photos. */
  listPhotoVideos: () => invoke<PhotoVideo[]>("list_photo_videos"),
  attachPhotoVideo: (reelId: string | null, assetId: string) => invoke<string>("import_reel_video_from_photos", { reelId, assetId }),
  action: (id: string, action: "reprocess" | "ignore" | "delete") => invoke<void>("reel_action", { id, action }),
  toolStatus: () => invoke<{ ytDlp: string | null }>("reel_tool_status"),
  /** Re-transcribe with a language ("auto" to detect again); the extraction is updated afterwards. */
  retranscribe: (id: string, locale: string) => invoke<void>("retranscribe_reel", { id, locale }),
  speechLocales: () => invoke<SpeechLocale[]>("speech_locales"),
};

export const LibraryService = {
  /** Screenshots and Reels together, newest first. */
  items: async (view: ScreenshotView, search?: string): Promise<LibraryItem[]> => {
    const [shots, reels] = await Promise.all([
      ScreenshotService.list(view, search),
      view === "all" || view === "everything" || view === "needsReview" || view === "processed" || view === "failed"
        ? ReelService.list() : Promise.resolve([] as Reel[]),
    ]);
    const q = search?.toLowerCase().trim();
    const reelItems: LibraryItem[] = reels
      .filter((r) => view === "all" || view === "everything"
        || (view === "needsReview" && r.status === "needsReview")
        || (view === "processed" && r.status === "complete")
        || (view === "failed" && (r.status === "failed" || r.status === "needsMedia")))
      .filter((r) => !q || [r.caption, r.creator, r.url, ...r.transcript.map((t) => t.text)].join(" ").toLowerCase().includes(q))
      .map((reel) => ({ kind: "reel", date: reel.createdAt, reel }));
    const shotItems: LibraryItem[] = shots.map((screenshot) => ({ kind: "screenshot", date: screenshot.creationDate, screenshot }));
    return [...reelItems, ...shotItems].sort((a, b) => (b.date ?? "").localeCompare(a.date ?? ""));
  },
};

export interface BackupSummary { path: string; sizeBytes: number; places: number; screenshots: number; reels: number; files: number }

export const BackupService = {
  /** Database, place photos/crops, Reel audio/frames and settings — never the API key. */
  backup: (path: string) => invoke<BackupSummary>("backup_library", { path }),
  exportPlaces: (path: string, format: "json" | "geojson") => invoke<number>("export_places", { path, format }),
};

export interface BulkReelImport { found: number; added: number; alreadyImported: number; ids: string[] }
