// Sample library for the demo build, UI tests and screenshots. Never part of the production app.
import type {
  Fact, Place, PlaceCategory, PlaceImage, Reel, Review, Screenshot, SourceType, TravelFactType, Trip,
} from "../api/types";

const T0 = Date.parse("2026-09-01T09:00:00Z");
const iso = (daysAgo: number) => new Date(T0 - daysAgo * 86_400_000).toISOString();

/** A soft landscape "photo" (original SVG art) so cards have pictures without shipping stock images. */
export function sceneSvg(seed: number, w = 640, h = 420): string {
  const palettes = [
    ["#f6d5a8", "#e98a6b", "#5b6c8f", "#2f3e5c"], ["#cfe7f5", "#7fb7d9", "#4a7f6b", "#24453a"],
    ["#fde2e4", "#f4a6a6", "#7d8cc4", "#3b3f6b"], ["#e3f0d8", "#9cc99a", "#4f8a6a", "#24513f"],
    ["#ffe9c7", "#f7b267", "#a86b52", "#4a3242"], ["#dbe7f1", "#a9c4dc", "#6b7d99", "#2c3552"],
  ];
  const [sky, sun, hill, ground] = palettes[seed % palettes.length];
  const r = (n: number) => ((seed * 9301 + n * 49297) % 233280) / 233280;
  const peaks = Array.from({ length: 5 }, (_, i) => `${(i * w) / 4},${h * (0.45 + r(i) * 0.2)}`).join(" ");
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}">
<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="${sky}"/><stop offset="1" stop-color="#ffffff"/></linearGradient></defs>
<rect width="${w}" height="${h}" fill="url(#s)"/><circle cx="${w * (0.2 + r(9) * 0.6)}" cy="${h * 0.3}" r="${h * 0.09}" fill="${sun}" opacity=".85"/>
<polygon points="0,${h} ${peaks} ${w},${h}" fill="${hill}" opacity=".8"/>
<path d="M0 ${h * 0.78} Q ${w * 0.5} ${h * (0.68 + r(3) * 0.1)} ${w} ${h * 0.8} V ${h} H 0Z" fill="${ground}"/></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

/** A phone screenshot: app chrome, a photo and caption lines. */
export function screenshotSvg(seed: number, title: string): string {
  const w = 300, h = 650;
  const photo = sceneSvg(seed, 300, 300);
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 ${w} ${h}">
<rect width="${w}" height="${h}" fill="#ffffff"/><rect y="0" width="${w}" height="64" fill="#fafafa"/>
<circle cx="26" cy="40" r="12" fill="#e1306c" opacity=".8"/><rect x="46" y="34" width="90" height="10" rx="5" fill="#222" opacity=".75"/>
<image href="${photo}" x="0" y="70" width="300" height="300"/>
<text x="16" y="402" font-family="-apple-system,Helvetica" font-size="17" font-weight="700" fill="#111">${title.replace(/&/g, "&amp;").replace(/</g, "&lt;")}</text>
${[430, 452, 474, 496, 518].map((y, i) => `<rect x="16" y="${y}" width="${200 + ((seed + i) % 4) * 18}" height="9" rx="4.5" fill="#999" opacity=".45"/>`).join("")}</svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
}

type Seed = [name: string, category: PlaceCategory, lat: number, lon: number, city: string, country: string, code: string, status?: Place["personalStatus"]];

const SEEDS: Seed[] = [
  ["Fushimi Inari Taisha", "temple", 34.9671, 135.7727, "Kyoto", "Japan", "JP", "visited"],
  ["Kiyomizu-dera", "temple", 34.9949, 135.785, "Kyoto", "Japan", "JP"],
  ["Nishiki Market", "food", 35.005, 135.7649, "Kyoto", "Japan", "JP", "favourite"],
  ["Arashiyama Bamboo Grove", "nature", 35.017, 135.6713, "Kyoto", "Japan", "JP"],
  ["Gion", "historicSite", 35.0037, 135.7788, "Kyoto", "Japan", "JP"],
  ["% Arabica Higashiyama", "cafe", 34.9986, 135.7808, "Kyoto", "Japan", "JP"],
  ["Kyoto", "city", 35.0116, 135.7681, "Kyoto", "Japan", "JP"],
  ["Tokyo Skytree", "viewpoint", 35.7101, 139.8107, "Tokyo", "Japan", "JP"],
  ["Senso-ji", "temple", 35.7148, 139.7967, "Tokyo", "Japan", "JP", "visited"],
  ["Shibuya Sky", "viewpoint", 35.6585, 139.7022, "Tokyo", "Japan", "JP"],
  ["teamLab Planets", "museum", 35.6492, 139.7898, "Tokyo", "Japan", "JP"],
  ["Ichiran Shibuya", "restaurant", 35.6614, 139.7003, "Tokyo", "Japan", "JP", "maybe"],
  ["Park Hyatt Tokyo", "hotel", 35.6856, 139.6909, "Tokyo", "Japan", "JP"],
  ["Lake Kawaguchi", "nature", 35.5173, 138.7561, "Fujikawaguchiko", "Japan", "JP"],
  ["Chureito Pagoda", "viewpoint", 35.5011, 138.8013, "Fujiyoshida", "Japan", "JP"],
  ["Sydney Opera House", "attraction", -33.8568, 151.2153, "Sydney", "Australia", "AU", "visited"],
  ["Bondi to Coogee Walk", "hiking", -33.8985, 151.2724, "Sydney", "Australia", "AU", "favourite"],
  ["Bourke Street Bakery", "cafe", -33.8869, 151.2124, "Sydney", "Australia", "AU"],
  ["Blue Mountains", "nature", -33.7, 150.3, "Katoomba", "Australia", "AU"],
  ["Sigiriya", "historicSite", 7.957, 80.7603, "Sigiriya", "Sri Lanka", "LK"],
  ["Nine Arch Bridge", "attraction", 6.8768, 81.0608, "Ella", "Sri Lanka", "LK"],
  ["Mirissa Beach", "beach", 5.9483, 80.4716, "Mirissa", "Sri Lanka", "LK"],
  ["Temple of the Tooth", "religiousSite", 7.2936, 80.6413, "Kandy", "Sri Lanka", "LK"],
  ["Louvre Museum", "museum", 48.8606, 2.3376, "Paris", "France", "FR"],
  ["Le Comptoir du Relais", "restaurant", 48.8519, 2.3389, "Paris", "France", "FR"],
  ["Montmartre", "historicSite", 48.8867, 2.3431, "Paris", "France", "FR"],
  ["Positano", "city", 40.628, 14.485, "Positano", "Italy", "IT"],
  ["Path of the Gods", "hiking", 40.6244, 14.535, "Agerola", "Italy", "IT"],
  ["Blue Lagoon", "nature", 63.8804, -22.4495, "Grindavík", "Iceland", "IS"],
  ["Skógafoss", "nature", 63.5321, -19.5114, "Skógar", "Iceland", "IS"],
];

const FACTS: [number, TravelFactType, string][] = [
  [0, "recommendedTime", "Go before 8 AM — the lower gates are packed by 10."],
  [0, "duration", "The full loop to the summit takes 2–3 hours."],
  [0, "generalTip", "Most people turn back at Yotsutsuji intersection; the view there is the best one."],
  [1, "openingHours", "Open 6:00–18:00 (until 21:30 during the spring and autumn illuminations)."],
  [1, "price", "Entry ¥400 for adults."],
  [2, "food", "Try the tako tamago (candied baby octopus with a quail egg)."],
  [2, "openingHours", "Most stalls open 10:00–17:00; many close on Wednesdays."],
  [3, "recommendedTime", "Arrive at sunrise for an empty path."],
  [3, "transportation", "JR Sagano Line to Saga-Arashiyama, then 10 minutes on foot."],
  [7, "price", "Tembo Deck tickets ¥2,100 on weekdays when booked online."],
  [7, "price", "Tickets ¥2,400 at the counter."],
  [7, "recommendedTime", "Book the slot just before sunset to see day and night views."],
  [9, "reservation", "Sunset slots sell out about two weeks ahead — book online."],
  [10, "warning", "You walk through knee-deep water: wear shorts or roll-up trousers."],
  [11, "food", "Order the extra-rich broth and the soft-boiled egg."],
  [14, "photography", "Climb the 398 steps behind the shrine for the classic Fuji + pagoda shot."],
  [15, "ticket", "Guided tours from AU$43, every 30 minutes."],
  [16, "duration", "6 km, about 2 hours with swim stops."],
  [16, "warning", "Little shade on the cliffs — bring water and sunscreen."],
  [19, "recommendedTime", "Climb at 7 AM when the gates open, before the heat and the tour buses."],
  [19, "price", "Foreign visitors US$36."],
  [20, "recommendedTime", "The Ella–Kandy blue train crosses at around 9:15 and 11:45."],
  [21, "bestSeason", "Whale-watching season is December to April."],
  [23, "reservation", "Book a timed entry online; Friday evenings are quieter."],
  [27, "transportation", "Start from Bomerano and walk downhill to Nocelle."],
  [28, "price", "Comfort entry from 11,990 ISK."],
  [28, "warning", "Book well ahead in summer; same-day tickets are rare."],
];

const SOURCES: SourceType[] = ["instagram", "tiktok", "instagram", "googleMaps", "safari", "xiaohongshu", "instagram"];
const CREATORS = ["@kyotofoodie", "@wanderwithmia", "@tokyocheapo", "@aussieescapes", null, "@lanka.trails", "@europe.on.foot"];

export interface DemoLibrary {
  places: Place[];
  facts: Fact[];
  screenshots: Screenshot[];
  links: { placeId: string; screenshotId: string }[];
  images: PlaceImage[];
  reels: Reel[];
  reelLinks: { placeId: string; reelId: string }[];
  reviews: Review[];
  trips: Trip[];
  tripPlaces: { id: string; tripId: string; placeId: string; position: number; day: number | null }[];
}

export function demoLibrary(): DemoLibrary {
  const places: Place[] = SEEDS.map(([name, category, lat, lon, city, country, code, status], i) => ({
    id: `p${i}`, canonicalName: name, alternativeNames: i === 7 ? ["東京スカイツリー"] : [], mapIdentifier: `m-${i}`,
    latitude: lat, longitude: lon, address: null, city, region: null, country, countryCode: code, category,
    verification: i === 28 ? "needsReview" : "verified", origin: "mapKit", isUserVerified: i % 5 === 0,
    personalStatus: status ?? "wantToVisit", notes: i === 2 ? "Go hungry. Cash only at most stalls." : "",
    summaryText: i === 0 ? "Thousands of vermilion torii gates winding up Mount Inari — saved for an early-morning hike before the crowds." : null,
    summaryFactIds: [], summaryGeneratedAt: null, heroImageId: null, createdAt: iso(60 - i), updatedAt: iso(30 - (i % 20)),
    sourceCount: 1 + (i % 3), heroImagePath: i % 4 === 3 ? null : sceneSvg(i), thumbnailPath: sceneSvg(i, 320, 240),
    visitedAt: status === "visited" ? "2025-04-12" : null, visitNotes: status === "visited" ? "Went at 7am, almost empty." : "",
    coverMemoryId: null, memoryCount: status === "visited" ? 3 : 0, userPhotoCount: 0, heroFocus: null,
  }));

  const screenshots: Screenshot[] = [];
  const links: DemoLibrary["links"] = [];
  places.forEach((p, i) => {
    for (let k = 0; k < (i % 7 === 0 ? 0 : p.sourceCount - (i % 3 === 2 ? 1 : 0)); k++) {
      const id = `s${screenshots.length}`;
      screenshots.push(shot(id, screenshots.length, `${p.canonicalName}`, "complete", 1));
      links.push({ placeId: p.id, screenshotId: id });
    }
  });
  // Uncertain and failed ones for Review and Import.
  screenshots.push(shot("s-review", 90, "Blue Lagoon — must visit!", "needsReview", 1, 1));
  screenshots.push(shot("s-travel?", 91, "Weekend ideas ✨", "needsReview", 0, 1));
  screenshots.push(shot("s-fail", 92, "", "failed", 0));
  screenshots.push(shot("s-wait", 93, "", "waitingForNetwork", 0));
  for (let k = 0; k < 6; k++) screenshots.push(shot(`s-nt${k}`, 100 + k, "Group chat", "notTravel", 0));
  links.push({ placeId: "p28", screenshotId: "s-review" });

  const facts: Fact[] = FACTS.map(([pi, type, text], i) => {
    const link = links.find((l) => l.placeId === `p${pi}`);
    return {
      id: `f${i}`, placeId: `p${pi}`, screenshotId: link?.screenshotId ?? null, reelId: link ? null : "r0",
      sourceKind: link ? "screenshot" : "audio", sourceTimeSec: link ? null : 12 + i, type, text,
      sourceQuote: null, confidence: 0.85, sourceBlockIds: [], validFrom: iso(400 - i * 9), origin: "ai",
      createdAt: iso(50 - i), sourceType: "instagram", creator: CREATORS[i % CREATORS.length],
    };
  });

  const reels: Reel[] = [
    reel("r0", "@kyotofoodie", "3 days in Kyoto: temples at sunrise, the best market snacks and a bamboo forest 🍡", "complete", 4, 41),
    reel("r1", "@wanderwithmia", "Sri Lanka by train 🚂 Kandy → Ella in 7 hours", "complete", 2, 58),
    reel("r2", "@tokyocheapo", "Tokyo views for under ¥2,500", "needsReview", 1, 33),
    reel("r3", null, null, "needsMedia", 0, null),
  ];
  const reelLinks = [
    { placeId: "p0", reelId: "r0" }, { placeId: "p2", reelId: "r0" }, { placeId: "p3", reelId: "r0" }, { placeId: "p1", reelId: "r0" },
    { placeId: "p20", reelId: "r1" }, { placeId: "p22", reelId: "r1" }, { placeId: "p9", reelId: "r2" },
  ];

  const reviews: Review[] = [
    review("rv0", "placeResolution", "s-review", null, 'I found "Blue Lagoon" — two places share this name.', "Blue Lagoon", [
      { name: "Blue Lagoon", latitude: 63.8804, longitude: -22.4495, city: "Grindavík", country: "Iceland", countryCode: "IS", address: "Norðurljósavegur 9, Grindavík" },
      { name: "Blue Lagoon", latitude: 36.0128, longitude: 14.3236, city: "Comino", country: "Malta", countryCode: "MT", address: "Comino, Malta" },
    ], "p28"),
    review("rv1", "travelClassification", "s-travel?", null, "Looks like weekend ideas, but no specific place is named.", null, []),
    review("rv2", "duplicatePlace", null, null, "These may be the same place.", "Shibuya Sky", [], "p9", "p7"),
    review("rv3", "placeResolution", null, "r2", 'I found "Tokyo Tower" but couldn’t place it on the map — no close match nearby.', "Tokyo Tower", [
      { name: "Tokyo Tower", latitude: 35.6586, longitude: 139.7454, city: "Tokyo", country: "Japan", countryCode: "JP", address: "4-2-8 Shibakoen, Minato" },
    ]),
    review("rv4", "processingFailure", "s-fail", null, "Processing failed 3 times: The file was moved or deleted.", null, []),
  ];

  const trips: Trip[] = [
    { id: "t0", name: "Japan in spring", startDate: "2027-03-28", endDate: "2027-04-08", notes: "JR Pass from day 3. Book Shibuya Sky two weeks ahead.", createdAt: iso(20), placeCount: 7 },
    { id: "t1", name: "Sri Lanka loop", startDate: null, endDate: null, notes: "", createdAt: iso(8), placeCount: 3 },
  ];
  const tripPlaces = [
    ...[7, 8, 9, 10].map((pi, k) => ({ id: `tp${k}`, tripId: "t0", placeId: `p${pi}`, position: k, day: k < 2 ? 1 : 2 })),
    ...[0, 2, 1].map((pi, k) => ({ id: `tp${k + 4}`, tripId: "t0", placeId: `p${pi}`, position: k, day: 4 })),
    ...[19, 22, 20].map((pi, k) => ({ id: `tp${k + 7}`, tripId: "t1", placeId: `p${pi}`, position: k, day: null })),
  ];

  const images: PlaceImage[] = places.slice(0, 4).map((p, i) => ({
    id: `img${i}`, placeId: p.id, screenshotId: links.find((l) => l.placeId === p.id)?.screenshotId ?? null,
    crop: { x: 0, y: 0.1, width: 1, height: 0.45 }, imagePath: sceneSvg(i + 10), qualityScore: 0.8, regionType: "photograph",
    regionConfidence: 0.9, isAccepted: true, origin: "local", duplicateSourceIds: [], caption: "", reelId: null, createdAt: iso(10),
  }));

  return { places, facts, screenshots, links, images, reels, reelLinks, reviews, trips, tripPlaces };

  function shot(id: string, n: number, title: string, status: Screenshot["status"], placeCount: number, openReviewCount = 0): Screenshot {
    const travel = status !== "notTravel" && status !== "failed" && status !== "waitingForNetwork";
    return {
      id, photosId: `ph-${id}`, creationDate: iso(n * 3 + 2), width: 1179, height: 2556, processedAt: travel ? iso(n) : null,
      aiPromptVersion: 9, status, statusDetail: status === "failed" ? "The file was moved or deleted" : status === "waitingForNetwork" ? "iCloud Photos download will be retried" : null,
      failureCount: status === "failed" ? 3 : 0, classification: travel ? "travel" : status === "notTravel" ? "notTravel" : "unknown",
      userClassification: null, travelConfidence: travel ? 0.92 : 0.1, localTravelScore: travel ? 0.7 : 0.05,
      sourceType: SOURCES[n % SOURCES.length], creator: CREATORS[n % CREATORS.length], ocrFullText: title,
      imagePath: screenshotSvg(n, title || "Screenshot"), thumbnailPath: screenshotSvg(n, title || "Screenshot"),
      escalationLevel: travel ? 2 : 1, aiModel: travel ? "deepseek-chat" : null, aiThinking: false, aiImageUsed: false,
      aiInputTokens: travel ? 640 : 0, aiOutputTokens: travel ? 180 : 0, aiLatencyMs: travel ? 2100 : 0, aiRetryCount: 0,
      aiCost: travel ? 0.00011 : 0, placeCount, openReviewCount,
    };
  }

  function reel(id: string, creator: string | null, caption: string | null, status: Reel["status"], placeCount: number, duration: number | null): Reel {
    const done = status === "complete" || status === "needsReview";
    return {
      id, url: `https://www.instagram.com/reel/C${id.toUpperCase()}x9demo/`, shortcode: `C${id}`, creator, caption, mediaPath: null, audioPath: null,
      thumbnailPath: done ? screenshotSvg(40 + id.length + placeCount, caption?.slice(0, 22) ?? "Reel") : null, durationSec: duration,
      status, statusDetail: status === "needsMedia" ? "Instagram didn't share this video. Import it from Photos instead." : null,
      classification: done ? "travel" : "unknown", travelConfidence: done ? 0.95 : 0,
      transcript: done ? [
        { start: 0, end: 6, text: "Okay, day one — we're starting at Fushimi Inari before sunrise." },
        { start: 6, end: 14, text: "Honestly go before eight, after that it's shoulder to shoulder." },
        { start: 14, end: 22, text: "Then breakfast at Nishiki Market, get the octopus thing, trust me." },
      ] : [],
      transcriptLocale: done ? "en-US" : null, mediaSource: done ? "ytdlp" : null, aiModel: done ? "deepseek-chat" : null,
      aiCost: done ? 0.0004 : 0, createdAt: iso(Number(id.slice(1)) * 4 + 1), processedAt: done ? iso(1) : null,
      placeCount, openReviewCount: status === "needsReview" ? 1 : 0,
      stages: done
        ? { caption: st("done"), video: st("done"), audio: st("done"), transcript: st("done"), keyframes: st("done"), ocr: st("done"), ai: st("done"), places: st("done") }
        : { caption: st("failed"), video: st("failed"), audio: st("skipped"), transcript: st("skipped") },
      transcriptLocaleOverride: null, transcriptConfidence: done ? 0.91 : null, placesExtracted: placeCount, placesAutoResolved: placeCount,
    };
  }

  function review(id: string, kind: Review["kind"], screenshotId: string | null, reelId: string | null, message: string, name: string | null,
                  candidates: Review["candidates"], a: string | null = null, b: string | null = null): Review {
    return {
      id, kind, screenshotId, reelId, message, extractedPlace: name ? { display_name: name } : null, candidates,
      placeAId: a, placeBId: b, imageId: null, isResolved: false, resolution: null, resolvedAt: null, resolvedPlaceId: null,
      createdAt: iso(2), screenshotThumbnail: screenshotId ? screenshotSvg(90, name ?? "Screenshot") : null,
    };
  }
}

function st(status: "done" | "failed" | "skipped") {
  return { status, detail: null };
}
