import { isDark } from "./theme";

/** Base-map styles (OpenFreeMap, no API key). The choice is remembered on this Mac. */
export type MapStyleId = "light" | "bright" | "colourful" | "dark" | "blue" | "auto";

export const MAP_STYLES: { id: MapStyleId; label: string; hint: string }[] = [
  { id: "light", label: "Light", hint: "Quiet grey — your places stand out" },
  { id: "bright", label: "Bright", hint: "Soft colours, more detail" },
  { id: "colourful", label: "Colourful", hint: "Full-colour atlas" },
  { id: "dark", label: "Dark", hint: "For night-time use" },
  { id: "blue", label: "Blue", hint: "Dark blue" },
  { id: "auto", label: "Match appearance", hint: "Light or Dark with the app" },
];

const KEY = "map.style";
const URLS: Record<Exclude<MapStyleId, "auto">, string> = {
  light: "positron", bright: "bright", colourful: "liberty", dark: "dark", blue: "fiord",
};

export function getMapStyleId(): MapStyleId {
  try {
    const v = localStorage.getItem(KEY) as MapStyleId | null;
    if (v && MAP_STYLES.some((s) => s.id === v)) return v;
  } catch { /* storage unavailable: use the default */ }
  return "auto";
}

export function setMapStyleId(id: MapStyleId) {
  try { localStorage.setItem(KEY, id); } catch { /* not remembered, still applied */ }
}

/** The style URL, and whether it's dark (labels on top of it flip to light text). */
export function resolveMapStyle(id: MapStyleId = getMapStyleId()): { url: string; dark: boolean } {
  const concrete = id === "auto" ? (isDark() ? "dark" : "light") : id;
  return { url: `https://tiles.openfreemap.org/styles/${URLS[concrete]}`, dark: concrete === "dark" || concrete === "blue" };
}
