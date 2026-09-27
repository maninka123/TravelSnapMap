import type { Place } from "../api/types";

/** Places within this distance of each other are treated as the same city on the map (suburbs, districts). */
const METRO_KM = 25;

const toRad = (d: number) => (d * Math.PI) / 180;
function km(a: Place, b: Place): number {
  const dLat = toRad(b.latitude - a.latitude);
  const dLon = toRad(b.longitude - a.longitude);
  const h = Math.sin(dLat / 2) ** 2 + Math.cos(toRad(a.latitude)) * Math.cos(toRad(b.latitude)) * Math.sin(dLon / 2) ** 2;
  return 12742 * Math.asin(Math.sqrt(h));
}

/** "Huangpu District" → "Huangpu"-style admin suffixes never name a city on their own. */
const DISTRICT = /\s(district|new area|county|municipality|ward|qu)$/i;
const isLatin = (s: string) => /^[\p{Script=Latin}\p{N}\s'’.,()-]+$/u.test(s);

/**
 * The city each place belongs to for grouping. Real hub cities are picked first (the city names you've saved
 * most, with a bonus when you saved the city itself), then every place within 25 km of a hub joins it
 * (Surry Hills, Haymarket → Sydney; Pudong → Shanghai; Roissy-en-France → Paris). Places far from any hub keep
 * their own city. Places in different countries never join each other.
 */
export function withMetro(places: Place[]): Place[] {
  const own = (p: Place) => p.city ?? p.region ?? p.canonicalName;
  type Hub = { name: string; cc: string | null; weight: number };
  const byName = new Map<string, Place[]>();
  places.forEach((p) => {
    const k = `${p.countryCode ?? ""}|${own(p)}`;
    byName.set(k, [...(byName.get(k) ?? []), p]);
  });
  const hubs: Hub[] = [...byName.values()].map((ps) => {
    const name = own(ps[0]);
    const isCityPlace = ps.some((p) => (p.category === "city" || p.category === "region") && p.canonicalName.toLowerCase() === name.toLowerCase());
    return {
      name, cc: ps[0].countryCode,
      weight: ps.length * 10 + (isCityPlace ? 15 : 0) + (isLatin(name) ? 2 : 0) - (DISTRICT.test(name) ? 30 : 0)
        + (ps[0].country && ps[0].country.toLowerCase() === name.toLowerCase() ? 5 : 0), // Singapore over Changi
    };
  }).sort((a, b) => b.weight - a.weight || a.name.localeCompare(b.name));

  const metro = new Map<string, string>();
  for (const hub of hubs) {
    // A hub only counts while it still has places of its own (e.g. Newtown's all went to Sydney → no "Newtown").
    const mine = places.filter((p) => !metro.has(p.id) && p.countryCode === hub.cc && own(p) === hub.name);
    if (mine.length === 0) continue;
    const centre = { latitude: mine.reduce((a, p) => a + p.latitude, 0) / mine.length, longitude: mine.reduce((a, p) => a + p.longitude, 0) / mine.length } as Place;
    for (const p of places) {
      if (metro.has(p.id) || p.countryCode !== hub.cc) continue;
      if (own(p) === hub.name || km(p, centre) <= METRO_KM) metro.set(p.id, hub.name);
    }
  }
  return places.map((p) => ({ ...p, metro: metro.get(p.id) ?? own(p) }));
}
