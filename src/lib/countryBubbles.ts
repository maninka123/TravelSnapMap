import type { Map as MLMap } from "maplibre-gl";
import type { Place } from "../api/types";
import { GROUPS, groupOf, type CategoryGroup } from "./categoryIcons";

// Flag artwork bundled with the app (same set as the flags in the UI).
const FLAG_URLS = import.meta.glob("/node_modules/flag-icons/flags/4x3/*.svg", { query: "?url", import: "default", eager: true }) as Record<string, string>;
const flagUrl = (cc: string) => FLAG_URLS[`/node_modules/flag-icons/flags/4x3/${cc.toLowerCase()}.svg`];

export interface CountryAggregate {
  cc: string;
  name: string;
  count: number;
  lng: number;
  lat: number;
  places: Place[];
  /** e.g. "5 Sights · 3 Food & drink" */
  summary: string;
}

/** "18 Sights · 5 Food & drink · 3 Stay" — the biggest groups first, at most three. */
export function groupSummary(counts: Partial<Record<CategoryGroup, number>>): string {
  return (Object.entries(counts) as [CategoryGroup, number][])
    .filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1]).slice(0, 3)
    .map(([g, n]) => `${n} ${GROUPS[g].label}`).join(" · ");
}

/** One aggregate per country, positioned at the middle of that country's saved places. */
export function countryAggregates(places: Place[]): CountryAggregate[] {
  const by = new Map<string, Place[]>();
  places.forEach((p) => { if (p.countryCode) by.set(p.countryCode, [...(by.get(p.countryCode) ?? []), p]); });
  return [...by.entries()].map(([cc, ps]) => {
    const counts: Partial<Record<CategoryGroup, number>> = {};
    ps.forEach((p) => { const g = groupOf(p.category); counts[g] = (counts[g] ?? 0) + 1; });
    return {
      cc, name: ps[0].country ?? cc, count: ps.length, places: ps, summary: groupSummary(counts),
      lng: ps.reduce((a, p) => a + p.longitude, 0) / ps.length,
      lat: ps.reduce((a, p) => a + p.latitude, 0) / ps.length,
    };
  });
}

export function countryGeoJSON(aggs: CountryAggregate[]): GeoJSON.FeatureCollection {
  return {
    type: "FeatureCollection",
    features: aggs.map((a) => ({
      type: "Feature", geometry: { type: "Point", coordinates: [a.lng, a.lat] },
      properties: { cc: a.cc, name: a.name, count: a.count, summary: a.summary, image: `country-${a.cc}-${a.count}` },
    })),
  };
}

const loaded = new Map<string, Promise<HTMLImageElement | null>>();
function loadFlag(cc: string): Promise<HTMLImageElement | null> {
  const url = flagUrl(cc);
  if (!url) return Promise.resolve(null);
  if (!loaded.has(url)) {
    loaded.set(url, new Promise((resolve) => {
      const img = new Image();
      img.onload = () => resolve(img);
      img.onerror = () => resolve(null);
      img.src = url;
    }));
  }
  return loaded.get(url)!;
}

/** A white pill with the flag and the count (drawn at 2× for crisp edges). */
function bubbleImage(flag: HTMLImageElement | null, count: number, cc: string): ImageData {
  const r = 2;
  const text = count.toLocaleString();
  const probe = document.createElement("canvas").getContext("2d")!;
  probe.font = `700 ${14 * r}px -apple-system, system-ui, sans-serif`;
  const textW = probe.measureText(text).width;
  const h = 30 * r, pad = 5 * r, flagW = 26 * r, flagH = 19.5 * r;
  const w = Math.ceil(pad + flagW + 7 * r + textW + 10 * r);
  const canvas = document.createElement("canvas");
  canvas.width = w + 8 * r;
  canvas.height = h + 8 * r;
  const ctx = canvas.getContext("2d")!;
  ctx.translate(4 * r, 3 * r);
  ctx.shadowColor = "rgba(0,0,0,0.28)";
  ctx.shadowBlur = 5 * r;
  ctx.shadowOffsetY = 1 * r;
  ctx.fillStyle = "#ffffff";
  ctx.beginPath();
  ctx.roundRect(0, 0, w, h, h / 2);
  ctx.fill();
  ctx.shadowColor = "transparent";
  const fx = pad, fy = (h - flagH) / 2;
  ctx.save();
  ctx.beginPath();
  ctx.roundRect(fx, fy, flagW, flagH, 3 * r);
  ctx.clip();
  if (flag) ctx.drawImage(flag, fx, fy, flagW, flagH);
  else {
    ctx.fillStyle = "#e9ecef";
    ctx.fillRect(fx, fy, flagW, flagH);
    ctx.fillStyle = "#495057";
    ctx.font = `700 ${10 * r}px -apple-system, system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(cc.toUpperCase(), fx + flagW / 2, fy + flagH / 2 + 0.5 * r);
    ctx.textAlign = "left";
  }
  ctx.restore();
  ctx.strokeStyle = "rgba(0,0,0,0.12)";
  ctx.lineWidth = 1 * r;
  ctx.beginPath();
  ctx.roundRect(fx, fy, flagW, flagH, 3 * r);
  ctx.stroke();
  ctx.fillStyle = "#1c1c1e";
  ctx.font = `700 ${14 * r}px -apple-system, system-ui, sans-serif`;
  ctx.textBaseline = "middle";
  ctx.fillText(text, fx + flagW + 7 * r, h / 2 + 0.5 * r);
  try {
    return ctx.getImageData(0, 0, canvas.width, canvas.height);
  } catch {
    // If the flag image can't be read back (canvas security), fall back to the country code.
    return flag ? bubbleImage(null, count, cc) : new ImageData(1, 1);
  }
}

/** Adds any missing bubble images, then asks the source to lay out again so they appear. */
export async function ensureCountryImages(map: MLMap, aggs: CountryAggregate[], refresh: () => void) {
  const missing = aggs.filter((a) => !map.hasImage(`country-${a.cc}-${a.count}`));
  if (missing.length === 0) return;
  const flags = await Promise.all(missing.map((a) => loadFlag(a.cc)));
  missing.forEach((a, i) => {
    const id = `country-${a.cc}-${a.count}`;
    if (!map.hasImage(id)) map.addImage(id, bubbleImage(flags[i], a.count, a.cc), { pixelRatio: 2 });
  });
  refresh();
}

// MARK: City / region bubbles (inside a picked country)

/** Which city/region a place belongs to on the map (city, else region, else its own name). */
export const areaKey = (p: Place) => p.city ?? p.region ?? p.canonicalName;

export interface AreaAggregate { key: string; count: number; lng: number; lat: number; summary: string; image: string; places: Place[] }

/** One bubble per city/region with 2+ places; single-place areas are returned separately (shown as pins). */
export function areaAggregates(places: Place[]): { areas: AreaAggregate[]; singles: Place[] } {
  const by = new Map<string, Place[]>();
  places.forEach((p) => by.set(areaKey(p), [...(by.get(areaKey(p)) ?? []), p]));
  const areas: AreaAggregate[] = [];
  const singles: Place[] = [];
  for (const [key, ps] of by) {
    if (ps.length === 1) { singles.push(ps[0]); continue; }
    const counts: Partial<Record<CategoryGroup, number>> = {};
    ps.forEach((p) => { const g = groupOf(p.category); counts[g] = (counts[g] ?? 0) + 1; });
    areas.push({
      key, count: ps.length, places: ps, summary: groupSummary(counts), image: `area-${hashKey(key)}-${ps.length}`,
      lng: ps.reduce((a, p) => a + p.longitude, 0) / ps.length,
      lat: ps.reduce((a, p) => a + p.latitude, 0) / ps.length,
    });
  }
  return { areas, singles };
}

export function areaGeoJSON(areas: AreaAggregate[]): GeoJSON.FeatureCollection {
  return {
    type: "FeatureCollection",
    features: areas.map((a) => ({
      type: "Feature", geometry: { type: "Point", coordinates: [a.lng, a.lat] },
      properties: { key: a.key, count: a.count, summary: a.summary, image: a.image },
    })),
  };
}

function hashKey(s: string): string {
  let h = 0;
  for (const c of s) h = (h * 31 + c.charCodeAt(0)) | 0;
  return (h >>> 0).toString(36);
}

/** A white pill: city name, then the count in a teal circle (the cluster colour). */
function areaBubbleImage(name: string, count: number): ImageData {
  const r = 2;
  const font = `600 ${13 * r}px -apple-system, system-ui, sans-serif`;
  const probe = document.createElement("canvas").getContext("2d")!;
  probe.font = font;
  const label = name.length > 22 ? `${name.slice(0, 21)}…` : name;
  const textW = probe.measureText(label).width;
  const h = 30 * r, dot = 22 * r, padL = 11 * r;
  const w = Math.ceil(padL + textW + 8 * r + dot + 4 * r);
  const canvas = document.createElement("canvas");
  canvas.width = w + 8 * r;
  canvas.height = h + 8 * r;
  const ctx = canvas.getContext("2d")!;
  ctx.translate(4 * r, 3 * r);
  ctx.shadowColor = "rgba(0,0,0,0.28)";
  ctx.shadowBlur = 5 * r;
  ctx.shadowOffsetY = 1 * r;
  ctx.fillStyle = "#ffffff";
  ctx.beginPath();
  ctx.roundRect(0, 0, w, h, h / 2);
  ctx.fill();
  ctx.shadowColor = "transparent";
  ctx.fillStyle = "#1c1c1e";
  ctx.font = font;
  ctx.textBaseline = "middle";
  ctx.fillText(label, padL, h / 2 + 0.5 * r);
  const cx = w - 4 * r - dot / 2;
  ctx.fillStyle = "#0f8a7e";
  ctx.beginPath();
  ctx.arc(cx, h / 2, dot / 2, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = "#ffffff";
  ctx.font = `700 ${11.5 * r}px -apple-system, system-ui, sans-serif`;
  ctx.textAlign = "center";
  ctx.fillText(String(count), cx, h / 2 + 0.5 * r);
  return ctx.getImageData(0, 0, canvas.width, canvas.height);
}

export function ensureAreaImages(map: MLMap, areas: AreaAggregate[]) {
  for (const a of areas) if (!map.hasImage(a.image)) map.addImage(a.image, areaBubbleImage(a.key, a.count), { pixelRatio: 2 });
}
