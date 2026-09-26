import type { PlaceCategory } from "../api/types";
import { CATEGORY } from "./labels";

/**
 * One consistent icon set for kinds of place — used on map pins, chips, lists and filters.
 * Line glyphs on a 24×24 grid (stroke 2, round caps), adapted from Lucide (ISC licence).
 */
export const CATEGORY_GLYPHS: Record<PlaceCategory, string[]> = {
  restaurant: ["M3 2v7c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2V2", "M7 2v20", "M21 15V2a5 5 0 0 0-5 5v6c0 1.1.9 2 2 2h3Zm0 0v7"],
  food: ["M3 12h18a9 9 0 0 1-18 0Z", "M8 21h8", "M12 3v5", "M8 5v3", "M16 5v3"],
  cafe: ["M10 2v2", "M14 2v2", "M16 8a1 1 0 0 1 1 1v8a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4V9a1 1 0 0 1 1-1h14a4 4 0 1 1 0 8h-1"],
  hotel: ["M2 4v16", "M2 8h18a2 2 0 0 1 2 2v10", "M2 17h20", "M6 8v9"],
  accommodation: ["M3 10 12 3l9 7", "M5 9v11h14V9", "M10 20v-6h4v6"],
  attraction: ["M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01Z"],
  viewpoint: ["M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3Z", "M15 13a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"],
  nature: ["M17 14l3 3.3a1 1 0 0 1-.7 1.7H4.7a1 1 0 0 1-.7-1.7L7 14h-.3a1 1 0 0 1-.7-1.7L9 9h-.2A1 1 0 0 1 8 7.3L12 3l4 4.3a1 1 0 0 1-.8 1.7H15l3 3.3a1 1 0 0 1-.7 1.7Z", "M12 22v-3"],
  beach: ["M2 6c.6.5 1.2 1 2.5 1C7 7 7 5 9.5 5c2.6 0 2.4 2 5 2 2.5 0 2.5-2 5-2 1.3 0 1.9.5 2.5 1", "M2 12c.6.5 1.2 1 2.5 1 2.5 0 2.5-2 5-2 2.6 0 2.4 2 5 2 2.5 0 2.5-2 5-2 1.3 0 1.9.5 2.5 1", "M2 18c.6.5 1.2 1 2.5 1 2.5 0 2.5-2 5-2 2.6 0 2.4 2 5 2 2.5 0 2.5-2 5-2 1.3 0 1.9.5 2.5 1"],
  hiking: ["M8 3l4 8 5-5 5 15H2Z"],
  temple: ["M3 4h18", "M5 7h14", "M7 7v14", "M17 7v14", "M6 12h12"],
  religiousSite: ["M12 3a4 4 0 0 0-4 4v3h8V7a4 4 0 0 0-4-4Z", "M4 21V13h16v8", "M2 21h20", "M10 21v-4a2 2 0 0 1 4 0v4", "M12 1v2"],
  museum: ["M3 22h18", "M6 18v-7", "M10 18v-7", "M14 18v-7", "M18 18v-7", "M12 2l8 5H4Z"],
  historicSite: ["M22 20v-9H2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2Z", "M18 11V4H6v7", "M15 22v-4a3 3 0 0 0-6 0v4", "M6 4V2", "M18 4V2", "M10 4V2", "M14 4V2"],
  shopping: ["M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4Z", "M3 6h18", "M16 10a4 4 0 0 1-8 0"],
  activity: ["M2 9a3 3 0 0 1 0 6v2a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-2a3 3 0 0 1 0-6V7a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z", "M13 5v2", "M13 17v2", "M13 11v2"],
  nightlife: ["M8 22h8", "M7 10h10", "M12 15v7", "M12 15a5 5 0 0 0 5-5c0-2-.5-4-2-8H9c-1.5 4-2 6-2 8a5 5 0 0 0 5 5Z"],
  transport: ["M4 6a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2Z", "M4 11h16", "M8 15h.01", "M16 15h.01", "M6 18l-1 3", "M18 18l1 3"],
  airport: ["M17.8 19.2 16 11l3.5-3.5C21 6 21.5 4 21 3c-1-.5-3 0-4.5 1.5L13 8 4.8 6.2c-.5-.1-.9.1-1.1.5l-.3.5c-.2.5-.1 1 .3 1.3L9 12l-2 3H4l-1 1 3 2 2 3 1-1v-3l3-2 3.5 5.3c.3.4.8.5 1.3.3l.5-.2c.4-.3.6-.7.5-1.2Z"],
  station: ["M4 15V5a3 3 0 0 1 3-3h10a3 3 0 0 1 3 3v10a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3Z", "M4 11h16", "M12 2v9", "M8 18l-2 4", "M16 18l2 4"],
  city: ["M6 22V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v18Z", "M6 12H4a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2h2", "M18 9h2a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-2", "M10 6h4", "M10 10h4", "M10 14h4", "M10 18h4"],
  region: ["M14.1 4.4 9.9 2.3a2 2 0 0 0-1.8 0L3.6 4.6A1 1 0 0 0 3 5.5v15.3a.5.5 0 0 0 .7.4l4.4-2.2a2 2 0 0 1 1.8 0l4.2 2.1a2 2 0 0 0 1.8 0l4.5-2.3a1 1 0 0 0 .6-.9V3.2a.5.5 0 0 0-.7-.4l-4.4 2.2a2 2 0 0 1-1.8 0Z", "M15 5.8v15", "M9 3.2v15"],
  other: ["M20 10c0 6-8 12-8 12s-8-6-8-12a8 8 0 0 1 16 0Z", "M15 10a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"],
};

/** Accepts a place category or a group key; icon and colour always come from the group. */
const glyphs = (c: string) => CATEGORY_GLYPHS[GROUPS[groupOf(c)].glyph];
export const colorOf = (c: string) => GROUPS[groupOf(c)].color;

/** The bare glyph, in the current text colour (for chips and inline use). */
export function CategoryGlyph({ category, size = 14, strokeWidth = 2 }: { category: string; size?: number; strokeWidth?: number }) {
  return (
    <svg className="cat-glyph" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor"
         strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {glyphs(category).map((d, i) => <path key={i} d={d} />)}
    </svg>
  );
}

/** The glyph on a round badge in the category colour — the same look as the map pins. */
export function CategoryBadge({ category, size = 28 }: { category: string; size?: number }) {
  return (
    <span className="cat-badge" style={{ width: size, height: size, background: colorOf(category) }} title={(CATEGORY[category as PlaceCategory] ?? GROUPS[groupOf(category)]).label}>
      <CategoryGlyph category={category} size={Math.round(size * 0.56)} strokeWidth={2.2} />
    </span>
  );
}

/**
 * Map pin images (drawn synchronously with Path2D so they can be added the moment a style loads).
 * Returns ImageData at 2× for crisp pins: a coloured disc, white ring, soft shadow and white glyph.
 */
export function pinImage(category: string, selected = false): ImageData {
  const ratio = 2;
  const size = (selected ? 44 : 32) * ratio;
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = size;
  const ctx = canvas.getContext("2d")!;
  const c = size / 2;
  const r = size / 2 - 4 * ratio;
  ctx.shadowColor = "rgba(0,0,0,0.32)";
  ctx.shadowBlur = 4 * ratio;
  ctx.shadowOffsetY = 1 * ratio;
  ctx.beginPath();
  ctx.arc(c, c, r, 0, Math.PI * 2);
  ctx.fillStyle = "#ffffff";
  ctx.fill();
  ctx.shadowColor = "transparent";
  ctx.beginPath();
  ctx.arc(c, c, r - (selected ? 3 : 2) * ratio, 0, Math.PI * 2);
  ctx.fillStyle = colorOf(category);
  ctx.fill();
  const glyph = r * 1.1;
  ctx.save();
  ctx.translate(c - glyph / 2, c - glyph / 2);
  ctx.scale(glyph / 24, glyph / 24);
  ctx.strokeStyle = "#ffffff";
  ctx.lineWidth = 2.2;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  for (const d of glyphs(category)) ctx.stroke(new Path2D(d));
  ctx.restore();
  return ctx.getImageData(0, 0, size, size);
}

/** The 9 groups shown in the app (map pins, filters, badges). Each place keeps its exact kind as a label. */
export type CategoryGroup = "food" | "stay" | "sights" | "nature" | "shopping" | "activities" | "transport" | "areas" | "other";

export const GROUPS: Record<CategoryGroup, { label: string; glyph: PlaceCategory; color: string; members: PlaceCategory[] }> = {
  food: { label: "Food & drink", glyph: "restaurant", color: "#e03131", members: ["restaurant", "food", "cafe", "nightlife"] },
  stay: { label: "Stay", glyph: "hotel", color: "#1c7ed6", members: ["hotel", "accommodation"] },
  sights: { label: "Sights", glyph: "attraction", color: "#f08c00", members: ["attraction", "viewpoint", "museum", "historicSite", "temple", "religiousSite"] },
  nature: { label: "Nature", glyph: "nature", color: "#2f9e44", members: ["nature", "beach", "hiking"] },
  shopping: { label: "Shopping", glyph: "shopping", color: "#d6336c", members: ["shopping"] },
  activities: { label: "Activities", glyph: "activity", color: "#0ca678", members: ["activity"] },
  transport: { label: "Transport", glyph: "station", color: "#495057", members: ["transport", "airport", "station"] },
  areas: { label: "Cities & regions", glyph: "city", color: "#3b5bdb", members: ["city", "region"] },
  other: { label: "Other", glyph: "other", color: "#868e96", members: ["other"] },
};

const GROUP_OF = new Map<string, CategoryGroup>(
  (Object.entries(GROUPS) as [CategoryGroup, (typeof GROUPS)[CategoryGroup]][]).flatMap(([g, def]) => def.members.map((m) => [m, g] as [string, CategoryGroup])),
);

export function groupOf(category: string): CategoryGroup {
  return GROUP_OF.get(category) ?? (category in GROUPS ? (category as CategoryGroup) : "other");
}
