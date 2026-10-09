// In-memory stand-in for the Rust backend, over Tauri's official IPC mocks. Used by `npm run demo` (browser preview
// for design work and screenshots) and by the UI tests. Never included in the production app.
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import type {
  AppConfig, Fact, Place, PlaceCandidate, PlaceDetail, PlaceWarning, QueueSnapshot, Reel, ReviewEntry, Screenshot, ScreenshotView, Trip,
} from "../api/types";
import { demoLibrary, type DemoLibrary } from "./demoData";

export interface DemoOptions {
  /** Start with an empty library (first-run experience). */
  empty?: boolean;
  /** Pretend the AI key is missing. */
  noApiKey?: boolean;
  /** Make every call fail with this message (error states). */
  failWith?: string;
}

export interface DemoBackend {
  lib: DemoLibrary;
  calls: { cmd: string; args: Record<string, unknown> }[];
  settings: { config: AppConfig; hasApiKey: boolean };
}

const IDLE: QueueSnapshot = {
  running: false, paused: false, mode: "idle", runId: null, phase: "Idle", inPhotos: 0, alreadyKnown: 0, newlyDiscovered: 0,
  total: 0, processed: 0, travel: 0, notTravel: 0, needsReview: 0, failed: 0, waiting: 0, remaining: 0, lastError: null,
};

export const DEFAULT_CONFIG: AppConfig = {
  model: "deepseek-chat", baseUrl: "https://api.deepseek.com", useThinkingByDefault: false, allowThinkingEscalation: true,
  allowVisionRequests: false, maxOutputTokens: 1200, thinkingMaxOutputTokens: 4000, visionImageMaxPixelSize: 1024, requestTimeoutSecs: 60,
  maxRetries: 3, dailyAiRequestLimit: 0, localSkipScore: 0.15, minimumLocalConfidenceForSkippingAi: 0.9,
  minimumAiConfidenceForAutoAcceptance: 0.8, travelReviewThreshold: 0.5, placeAutoAcceptScore: 0.8, placeReviewScore: 0.5,
  regionAutoAcceptConfidence: 0.85, regionReviewConfidence: 0.6, duplicateImageDistance: 0.35, maxConcurrentScreenshots: 3,
  scanFromYear: null, scanToYear: null, nearbyRadiusKm: 5, transcriptionLocale: "auto", autoProcessNewScreenshots: false,
  screenshotFolders: [], maxKeyframes: 8, ytDlpPath: "", cookiesFromBrowser: "",
  pricing: {
    mode: "timeOfDay", peak: { inputCacheHit: 0.07, inputCacheMiss: 0.27, output: 1.1 }, offPeak: { inputCacheHit: 0.035, inputCacheMiss: 0.135, output: 0.55 },
    peakHoursUtc: [[0, 16]], source: "DeepSeek pricing page, checked 2026-09", custom: false,
  },
};

export function installDemoBackend(options: DemoOptions = {}): DemoBackend {
  const lib = options.empty ? emptyLibrary() : demoLibrary();
  const state: DemoBackend = { lib, calls: [], settings: { config: structuredClone(DEFAULT_CONFIG), hasApiKey: !options.noApiKey } };
  const w = window as unknown as { __TAURI_INTERNALS__: Record<string, unknown> };

  mockIPC((cmd, raw) => {
    const args = (raw ?? {}) as Record<string, any>; // eslint-disable-line @typescript-eslint/no-explicit-any
    state.calls.push({ cmd, args });
    if (options.failWith && !cmd.startsWith("plugin:")) throw new Error(options.failWith);
    const result = handle(state, cmd, args);
    if (MUTATIONS.has(cmd)) queueMicrotask(() => void emit("data-changed"));
    return result;
  }, { shouldMockEvents: true });
  // Sample images are data: URLs; real paths would go through the asset protocol.
  w.__TAURI_INTERNALS__.convertFileSrc = (p: string) => p;
  w.__TAURI_INTERNALS__.metadata = { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } };
  return state;
}

const MUTATIONS = new Set([
  "update_place", "set_place_location", "delete_place", "merge_places", "add_manual_place", "create_trip", "update_trip", "delete_trip",
  "add_to_trip", "update_trip_entry", "remove_trip_entry", "resolve_review", "change_review", "screenshot_action", "reel_action",
  "import_reels", "save_settings", "reorder_trip", "import_screenshot_files", "save_api_key", "delete_fact", "update_fact", "add_fact", "remove_place_screenshot",
]);

function emptyLibrary(): DemoLibrary {
  return { places: [], facts: [], screenshots: [], links: [], images: [], reels: [], reelLinks: [], reviews: [], trips: [], tripPlaces: [] };
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function handle(s: DemoBackend, cmd: string, a: Record<string, any>): unknown {
  const { lib } = s;
  const place = (id: string) => lib.places.find((p) => p.id === id);
  const counts = () => {
    const c: Record<string, number> = {};
    lib.screenshots.forEach((x) => { c[x.status] = (c[x.status] ?? 0) + 1; });
    return c;
  };
  switch (cmd) {
    // App
    case "get_overview":
      return {
        screenshotCounts: counts(), openReviews: lib.reviews.filter((r) => !r.isResolved).length, places: lib.places.length,
        unconfirmedPlaces: lib.places.filter((p) => p.verification === "needsReview").length, queue: IDLE, model: s.settings.config.model,
        aiConfigured: s.settings.hasApiKey, apiKeySource: s.settings.hasApiKey ? "macOS Keychain" : "",
      };
    case "diagnostics": {
      const c = counts();
      return {
        totalScreenshots: lib.screenshots.length, travelScreenshots: (c.complete ?? 0) + (c.needsReview ?? 0), notTravel: c.notTravel ?? 0,
        skippedLocally: c.notTravel ?? 0, needsReview: c.needsReview ?? 0, failed: c.failed ?? 0, remaining: c.waitingForNetwork ?? 0,
        places: lib.places.length, aiRequests: 41, aiTextRequests: 41, aiVisionRequests: 0, aiThinkingRequests: 2, aiFailures: 1,
        aiInputTokens: 26_240, aiOutputTokens: 7_380, aiCacheHitTokens: 9_100, aiEstimatedCost: 0.0118, aiAverageLatencyMs: 2140,
        openReviews: lib.reviews.filter((r) => !r.isResolved).length,
        metrics: [["maps.autoResolved", 38], ["maps.review", 3], ["maps.unresolved", 1], ["places.created", 30], ["pipeline.total.ms", 180000], ["pipeline.total.count", 44], ["ocr.ms", 13200], ["ocr.count", 44], ["ai.extract.ms", 88000], ["ai.extract.count", 41]],
      };
    }
    case "cost_summary":
      return { total: 0.0118, byTimeOfDay: 0.0118, alwaysPeak: 0.0151, alwaysOffPeak: 0.0076, screenshots: 0.0101, reels: 0.0017, other: 0,
        requests: 41, peakRequests: 18, screenshotsProcessed: 44, screenshotsRemaining: 1, per100Screenshots: 0.023, projectedRemaining: 0.0002 };
    case "photos_permission_status": case "request_photos_permission": return lib.places.length ? "authorized" : "notDetermined";
    case "processing_status": return IDLE;
    case "scan_preview": return { photos: { total: lib.screenshots.length + 12, known: lib.screenshots.length, new: 12 }, folders: [] };
    case "list_runs": return lib.places.length ? [{ id: "run1", kind: "validation", requested: 25, sample: "random", screenshotCount: 25, startedAt: "2026-08-20T10:00:00Z", finishedAt: "2026-08-20T10:06:00Z" }] : [];
    case "run_report": return null;
    case "start_processing": case "pause_processing": case "resume_processing": case "cancel_processing": case "add_screenshot_folder": case "remove_screenshot_folder": return null;
    case "reprocess": return 0;
    case "open_external": return null;

    // Places
    case "list_places": {
      const f = a.filter ?? {};
      const q = (f.search ?? "").toLowerCase();
      let list = lib.places.filter((p) => (!f.verifiedOnly || p.verification !== "needsReview")
        && (!f.countryCode || p.countryCode === f.countryCode) && (!f.city || p.city === f.city) && (!f.status || p.personalStatus === f.status)
        && (!q || [p.canonicalName, p.city, p.country, ...lib.facts.filter((x) => x.placeId === p.id).map((x) => x.text)].join(" ").toLowerCase().includes(q)));
      if (f.sort === "name") list = [...list].sort((x, y) => x.canonicalName.localeCompare(y.canonicalName));
      else if (f.sort === "sources") list = [...list].sort((x, y) => y.sourceCount - x.sourceCount);
      else list = [...list].sort((x, y) => y.createdAt.localeCompare(x.createdAt));
      return list;
    }
    case "get_place_detail": return placeDetail(lib, a.id);
    case "place_warnings": {
      const out: Record<string, PlaceWarning[]> = {};
      out.p7 = [{ kind: "conflict", title: "Two sources give different ticket prices", detail: "¥2,100 online vs ¥2,400 at the counter", factIds: ["f9", "f10"] }];
      out.p28 = [{ kind: "stale", title: "Price saved over a year ago", detail: "Check the current price before you go", factIds: ["f25"] }];
      return Object.fromEntries(Object.entries(out).filter(([id]) => place(id)));
    }
    case "update_place": {
      const p = place(a.id);
      if (p) Object.assign(p, { [a.field]: a.value }, a.field === "canonicalName" || a.field === "category" ? { isUserVerified: true } : {});
      return null;
    }
    case "set_place_location": {
      const p = place(a.id);
      const c = a.candidate as PlaceCandidate;
      if (p) Object.assign(p, { canonicalName: c.name, latitude: c.latitude, longitude: c.longitude, city: c.city ?? p.city, country: c.country ?? p.country, countryCode: c.countryCode ?? p.countryCode, verification: "userVerified", isUserVerified: true });
      return a.id;
    }
    case "delete_place": lib.places = lib.places.filter((p) => p.id !== a.id); return null;
    case "merge_places": lib.places = lib.places.filter((p) => p.id !== a.sourceId); return null;
    case "search_map": {
      const q = String(a.query ?? "").toLowerCase();
      return lib.places.filter((p) => p.canonicalName.toLowerCase().includes(q)).slice(0, 5).map((p) => ({
        name: p.canonicalName, latitude: p.latitude, longitude: p.longitude, city: p.city, country: p.country, countryCode: p.countryCode, mapIdentifier: p.mapIdentifier,
      }));
    }
    case "add_manual_place": {
      const c = a.candidate as PlaceCandidate;
      const existing = lib.places.find((p) => p.mapIdentifier && p.mapIdentifier === c.mapIdentifier);
      if (existing) return { id: existing.id, existing: true };
      const id = `p${lib.places.length + 100}`;
      lib.places.push({ ...lib.places[0], id, canonicalName: c.name, latitude: c.latitude, longitude: c.longitude, city: c.city ?? null, country: c.country ?? null,
        countryCode: c.countryCode ?? null, category: (a.category ?? "other") as Place["category"], origin: "manual", personalStatus: a.status, notes: a.notes ?? "",
        sourceCount: 0, heroImagePath: null, thumbnailPath: null, summaryText: null, createdAt: new Date().toISOString() });
      return { id, existing: false };
    }
    case "delete_fact": lib.facts = lib.facts.filter((f) => f.id !== a.id); return null;
    case "update_fact": { const f = lib.facts.find((x) => x.id === a.id); if (f) Object.assign(f, { text: a.text, type: a.factType, origin: "user" }); return null; }
    case "add_fact": {
      const id = `f${lib.facts.length + 100}`;
      lib.facts.push({ ...lib.facts[0], id, placeId: a.placeId, type: a.factType, text: a.text, screenshotId: a.screenshotId, reelId: a.reelId, origin: "user", sourceKind: "user" });
      return id;
    }
    case "summarize_place": return "A short summary written only from your saved tips.";
    case "photos_near_place": case "photos_between": return [];
    case "photo_previews": return {};
    case "remove_place_screenshot": lib.links = lib.links.filter((l) => !(l.placeId === a.placeId && l.screenshotId === a.screenshotId)); return null;

    // Sources
    case "list_screenshots": return screenshots(lib, a.filter.view, a.filter.search).slice(0, a.filter.limit ?? 300);
    case "count_screenshots": return screenshots(lib, a.filter.view, a.filter.search).length;
    case "screenshot_refs": return screenshots(lib, a.filter.view, a.filter.search).map((x) => ({ id: x.id, date: x.creationDate }));
    case "get_screenshot_detail": {
      const shot = lib.screenshots.find((x) => x.id === a.id)!;
      const placeIds = lib.links.filter((l) => l.screenshotId === a.id).map((l) => l.placeId);
      return {
        screenshot: shot, places: lib.places.filter((p) => placeIds.includes(p.id)),
        blocks: shot.ocrFullText ? [{ id: "b0", idx: 0, text: shot.ocrFullText, confidence: 0.98, x: 0.05, y: 0.6, width: 0.7, height: 0.04 }] : [],
        links: placeIds.map((placeId) => ({ placeId, screenshotId: a.id, extractedName: place(placeId)?.canonicalName ?? "", confidence: 0.9, origin: "ai", isUserVerified: false })),
        facts: lib.facts.filter((f) => f.screenshotId === a.id), images: lib.images.filter((i) => i.screenshotId === a.id),
        reviews: lib.reviews.filter((r) => r.screenshotId === a.id),
      };
    }
    case "screenshot_action": {
      const shot = lib.screenshots.find((x) => x.id === a.id);
      if (shot) shot.status = a.action === "ignore" ? "ignored" : a.action === "markNotTravel" ? "notTravel" : a.action === "reprocess" ? "discovered" : shot.status;
      return null;
    }
    case "list_reels": return lib.reels;
    case "get_reel_detail": {
      const reel = lib.reels.find((r) => r.id === a.id)!;
      const ids = lib.reelLinks.filter((l) => l.reelId === a.id).map((l) => l.placeId);
      return { reel, keyframes: [], places: lib.places.filter((p) => ids.includes(p.id)), facts: lib.facts.filter((f) => f.reelId === a.id), images: [], reviews: lib.reviews.filter((r) => r.reelId === a.id) };
    }
    case "import_reel": return "r0";
    case "import_reels": case "import_reels_from_file": {
      const found = new Set([...String(a.text ?? "").matchAll(/instagram\.com\/(?:[\w.]+\/)?(?:reel|reels|p|tv)\/([\w-]+)/gi)].map((m) => m[1])).size;
      return { found, added: found, alreadyImported: 0, ids: [] };
    }
    case "reel_action": { const r = lib.reels.find((x) => x.id === a.id); if (r && a.action === "ignore") r.status = "ignored"; return null; }
    case "reel_tool_status": return { ytDlp: "/opt/homebrew/bin/yt-dlp" };
    case "speech_locales": return [{ id: "en-US", name: "English (US)", onDevice: true, engine: "speech" }, { id: "ja-JP", name: "Japanese", onDevice: true, engine: "speech" }];
    case "list_photo_videos": return [];

    // Review
    case "list_reviews": return lib.reviews.filter((r) => !r.isResolved).map((r) => entry(lib, r));
    case "recent_reviews": return lib.reviews.filter((r) => r.isResolved).map((r) => entry(lib, r));
    case "source_reviews": return lib.reviews.filter((r) => (a.screenshotId && r.screenshotId === a.screenshotId) || (a.reelId && r.reelId === a.reelId)).map((r) => entry(lib, r));
    case "resolve_review": case "change_review": {
      const r = lib.reviews.find((x) => x.id === a.id);
      if (r) Object.assign(r, { isResolved: true, resolution: a.action, resolvedAt: new Date().toISOString() });
      return null;
    }

    // Trips
    case "list_trips": return lib.trips.map((t) => ({ ...t, placeCount: lib.tripPlaces.filter((tp) => tp.tripId === t.id).length }));
    case "create_trip": { const id = `t${lib.trips.length + 10}`; lib.trips.unshift({ id, name: a.name, startDate: null, endDate: null, notes: "", createdAt: new Date().toISOString(), placeCount: 0 }); return id; }
    case "update_trip": { const t = lib.trips.find((x) => x.id === a.id); if (t) Object.assign(t, { name: a.name, startDate: a.startDate, endDate: a.endDate, notes: a.notes } satisfies Partial<Trip>); return null; }
    case "delete_trip": lib.trips = lib.trips.filter((t) => t.id !== a.id); lib.tripPlaces = lib.tripPlaces.filter((t) => t.tripId !== a.id); return null;
    case "trip_entries": return lib.tripPlaces.filter((t) => t.tripId === a.tripId).sort((x, y) => (x.day ?? 99) - (y.day ?? 99) || x.position - y.position)
      .map((t) => ({ id: t.id, tripId: t.tripId, position: t.position, day: t.day, place: place(t.placeId)! })).filter((e) => e.place);
    case "add_to_trip": lib.tripPlaces.push({ id: `tp${lib.tripPlaces.length + 50}`, tripId: a.tripId, placeId: a.placeId, position: lib.tripPlaces.length, day: null }); return null;
    case "update_trip_entry": { const t = lib.tripPlaces.find((x) => x.id === a.entryId); if (t) { t.day = a.day; if (a.position != null) t.position = a.position; } return null; }
    case "remove_trip_entry": lib.tripPlaces = lib.tripPlaces.filter((t) => t.id !== a.entryId); return null;

    // Settings
    case "get_settings": return { config: s.settings.config, defaults: DEFAULT_CONFIG, hasApiKey: s.settings.hasApiKey, apiKeySource: s.settings.hasApiKey ? "macOS Keychain" : "" };
    case "save_settings": s.settings.config = a.config; return null;
    case "save_api_key": s.settings.hasApiKey = !!a.key; return null;
    case "restore_backup": return { places: 12, screenshots: 40, reels: 2, trips: 1, schemaVersion: 7 };
    case "cancel_restore": case "restart_app": case "reveal_data_folder": return null;
    case "import_screenshot_files": return { added: (a.paths ?? []).length, alreadyImported: 0, skipped: 0, queuedBehindScan: false };
    case "reorder_trip": {
      (a.order as { id: string; day: number | null }[]).forEach((o, i) => { const t = lib.tripPlaces.find((x) => x.id === o.id); if (t) { t.day = o.day; t.position = i; } });
      return null;
    }
    case "export_trip": return null;
    case "backup_library": return { path: a.path, sizeBytes: 18_400_000, places: lib.places.length, screenshots: lib.screenshots.length, reels: lib.reels.length, files: 210 };
    case "export_places": return lib.places.length;

    // Plugins (dialogs): nothing picked in the preview.
    case "plugin:dialog|open": case "plugin:dialog|save": return null;
  }
  console.warn(`[demo] unhandled command ${cmd}`, a);
  return null;
}

function screenshots(lib: DemoLibrary, view: ScreenshotView, search?: string | null): Screenshot[] {
  const q = (search ?? "").toLowerCase();
  return lib.screenshots.filter((s) => {
    const ok = {
      all: s.classification === "travel" && s.status !== "ignored", processed: s.status === "complete", needsReview: s.status === "needsReview",
      multiplePlaces: s.placeCount > 1, noPlace: s.classification === "travel" && s.placeCount === 0, ignored: s.status === "ignored",
      notTravel: s.status === "notTravel", failed: s.status === "failed", pending: s.status === "waitingForNetwork" || s.status === "discovered", everything: true,
    }[view];
    return ok && (!q || s.ocrFullText.toLowerCase().includes(q) || (s.creator ?? "").toLowerCase().includes(q));
  });
}

function entry(lib: DemoLibrary, r: DemoLibrary["reviews"][number]): ReviewEntry {
  return { review: r, placeA: lib.places.find((p) => p.id === r.placeAId) ?? null, placeB: lib.places.find((p) => p.id === r.placeBId) ?? null, image: null };
}

function placeDetail(lib: DemoLibrary, id: string): PlaceDetail {
  const place = lib.places.find((p) => p.id === id)!;
  const shotIds = lib.links.filter((l) => l.placeId === id).map((l) => l.screenshotId);
  const reelIds = lib.reelLinks.filter((l) => l.placeId === id).map((l) => l.reelId);
  const near = lib.places.filter((p) => p.id !== id && Math.abs(p.latitude - place.latitude) < 0.1 && Math.abs(p.longitude - place.longitude) < 0.1)
    .map((p) => ({ place: p, distanceKm: Math.hypot(p.latitude - place.latitude, (p.longitude - place.longitude) * Math.cos((place.latitude * Math.PI) / 180)) * 111 }))
    .sort((x, y) => x.distanceKm - y.distanceKm).slice(0, 6);
  const facts: Fact[] = lib.facts.filter((f) => f.placeId === id);
  return {
    place, facts, screenshots: lib.screenshots.filter((s) => shotIds.includes(s.id)),
    links: shotIds.map((screenshotId) => ({ placeId: id, screenshotId, extractedName: place.canonicalName, confidence: 0.9, origin: "ai", isUserVerified: false })),
    images: lib.images.filter((i) => i.placeId === id), reels: lib.reels.filter((r: Reel) => reelIds.includes(r.id)), nearby: near,
    trips: lib.trips.filter((t) => lib.tripPlaces.some((tp) => tp.tripId === t.id && tp.placeId === id)), memories: [],
    warnings: id === "p7" ? [{ kind: "conflict", title: "Two sources give different ticket prices", detail: "¥2,100 online vs ¥2,400 at the counter", factIds: ["f9", "f10"] }] : [],
  };
}
