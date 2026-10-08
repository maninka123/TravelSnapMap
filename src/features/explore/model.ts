// Explore: the shared state behind the map, the list beside it and the library grid — and the pure logic for
// filtering and search suggestions (unit-tested in model.test.ts).
import type { PersonalStatus, Place } from "../../api/types";
import { CATEGORY } from "../../lib/labels";
import { groupOf, type CategoryGroup } from "../../lib/categoryIcons";
import { areaKey } from "../../lib/countryBubbles";

/** A country or city picked from search, a bubble or the list: everything is limited to it. */
export type Scope = { kind: "country" | "city"; key: string; label: string; code: string | null; parent?: Scope; compact?: boolean };

export interface ExploreFilters {
  query: string;
  groups: CategoryGroup[];
  statuses: PersonalStatus[];
  scope?: Scope;
}

export const EMPTY_FILTERS: ExploreFilters = { query: "", groups: [], statuses: [] };

/** Quick status filters shown as chips (the rest live in the Filters popover). */
export const QUICK_STATUS: { key: "all" | PersonalStatus; label: string }[] = [
  { key: "all", label: "All" },
  { key: "wantToVisit", label: "Want to visit" },
  { key: "visited", label: "Visited" },
];

const KEY = "explore.filters";

/** Filters and scope survive switching views and restarting the app. */
export function loadFilters(): ExploreFilters {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const v = JSON.parse(raw) as Partial<ExploreFilters>;
      return { query: "", groups: Array.isArray(v.groups) ? v.groups : [], statuses: Array.isArray(v.statuses) ? v.statuses : [], scope: v.scope };
    }
  } catch { /* fall back to no filters */ }
  return EMPTY_FILTERS;
}

export function saveFilters(f: ExploreFilters) {
  try { localStorage.setItem(KEY, JSON.stringify({ groups: f.groups, statuses: f.statuses, scope: f.scope })); } catch { /* not remembered */ }
}

export function inScope(p: Place, scope?: Scope): boolean {
  if (!scope) return true;
  return scope.kind === "country" ? p.countryCode === scope.key : areaKey(p) === scope.key;
}

/** Kind, status and text filters (not the scope). Text matches names, alternative names, city and country. */
export function matchesFilters(p: Place, f: Pick<ExploreFilters, "query" | "groups" | "statuses">): boolean {
  const q = f.query.toLowerCase().trim();
  return (f.groups.length === 0 || f.groups.includes(groupOf(p.category))) &&
    (f.statuses.length === 0 || f.statuses.includes(p.personalStatus)) &&
    (!q || [p.canonicalName, ...p.alternativeNames, p.city, p.metro, p.country].join(" ").toLowerCase().includes(q));
}

export function applyFilters(places: Place[], f: ExploreFilters): Place[] {
  return places.filter((p) => inScope(p, f.scope) && matchesFilters(p, f));
}

export function activeFilterCount(f: ExploreFilters): number {
  return f.groups.length + f.statuses.length;
}

/** Counts per kind-of-place group and per status, for filter chips. */
export function tallies(places: Place[]) {
  const groups = new Map<CategoryGroup, number>();
  const statuses = new Map<PersonalStatus, number>();
  for (const p of places) {
    const g = groupOf(p.category);
    groups.set(g, (groups.get(g) ?? 0) + 1);
    statuses.set(p.personalStatus, (statuses.get(p.personalStatus) ?? 0) + 1);
  }
  return { groups, statuses };
}

export type Suggestion =
  | { kind: "place"; key: string; label: string; sub: string; place: Place }
  | { kind: "city" | "country"; key: string; label: string; sub: string; code: string | null };

const n = (k: number) => `${k} saved place${k === 1 ? "" : "s"}`;

/**
 * Suggestions for the search field, from the places in scope only. Nothing typed: your countries, or — once a
 * country is picked — its cities and places. While typing: names starting with the text first, then words starting
 * with it, then anywhere.
 */
export function suggest(places: Place[], query: string, scope?: Scope): Suggestion[] {
  const q = query.toLowerCase().trim();
  if (!q) return browse(places, scope);
  const rank = (text: string | null | undefined) => {
    const t = (text ?? "").toLowerCase();
    if (t.startsWith(q)) return 0;
    if (t.split(/[\s,()-]+/).some((w) => w.startsWith(q))) return 1;
    return t.includes(q) ? 2 : 9;
  };
  const scored: [number, Suggestion][] = [];
  const countries = new Map<string, { name: string; count: number }>();
  const cities = new Map<string, { country: string | null; code: string | null; count: number }>();
  for (const p of places) {
    if (p.countryCode && p.country) countries.set(p.countryCode, { name: p.country, count: (countries.get(p.countryCode)?.count ?? 0) + 1 });
    const city = areaKey(p);
    if (city && city !== p.canonicalName) cities.set(city, { country: p.country, code: p.countryCode, count: (cities.get(city)?.count ?? 0) + 1 });
    const r = Math.min(rank(p.canonicalName), ...p.alternativeNames.map(rank));
    if (r < 9) scored.push([r, placeSuggestion(p)]);
  }
  for (const [code, c] of scope ? [] : countries) {
    const r = Math.min(rank(c.name), code.toLowerCase() === q ? 0 : 9);
    if (r < 9) scored.push([r - 0.5, { kind: "country", key: code, label: c.name, sub: n(c.count), code }]);
  }
  for (const [city, c] of scope?.kind === "city" ? [] : cities) {
    const r = rank(city);
    if (r < 9) scored.push([r - 0.25, { kind: "city", key: city, label: city, sub: [c.country, n(c.count)].filter(Boolean).join(" · "), code: c.code }]);
  }
  return scored.sort((a, b) => a[0] - b[0] || a[1].label.localeCompare(b[1].label)).slice(0, 8).map(([, s]) => s);
}

function placeSuggestion(p: Place): Suggestion {
  const cat = CATEGORY[p.category] ?? CATEGORY.other;
  return { kind: "place", key: p.id, label: p.canonicalName, sub: [cat.label, p.city !== p.canonicalName ? p.city : null, p.country].filter(Boolean).join(" · "), place: p };
}

function browse(places: Place[], scope?: Scope): Suggestion[] {
  const count = <K,>(key: (p: Place) => K | null | undefined) => {
    const m = new Map<K, Place[]>();
    places.forEach((p) => { const k = key(p); if (k != null) m.set(k, [...(m.get(k) ?? []), p]); });
    return [...m.entries()].sort((a, b) => b[1].length - a[1].length);
  };
  if (!scope) {
    return count((p) => p.countryCode).slice(0, 10).map(([code, ps]) => ({ kind: "country", key: code, label: ps[0].country ?? code, sub: n(ps.length), code }));
  }
  if (scope.kind === "country") {
    const cities: Suggestion[] = count((p) => (areaKey(p) !== p.canonicalName ? areaKey(p) : null)).slice(0, 6)
      .map(([city, ps]) => ({ kind: "city", key: city, label: city, sub: n(ps.length), code: scope.code }));
    const shown = new Set(cities.map((c) => c.key));
    const rest = places.filter((p) => !shown.has(areaKey(p))).sort((a, b) => b.sourceCount - a.sourceCount).slice(0, 10 - cities.length).map(placeSuggestion);
    return [...cities, ...rest];
  }
  return [...places].sort((a, b) => b.sourceCount - a.sourceCount).slice(0, 10).map(placeSuggestion);
}

/** The scope a suggestion leads to (keeping the country as the parent of a city). */
export function scopeFor(s: Exclude<Suggestion, { kind: "place" }>, current?: Scope): Scope {
  return {
    kind: s.kind, key: s.key, label: s.label, code: s.kind === "country" ? s.key : s.code,
    parent: s.kind === "city" ? (current?.kind === "country" ? current : s.code ? { kind: "country", key: s.code, label: s.code, code: s.code } : undefined) : undefined,
  };
}

/** Places grouped for lists: by country when several are shown, otherwise by city (biggest groups first). */
export function groupForList(places: Place[]): { key: string; label: string; code: string | null; kind: "country" | "city"; items: Place[] }[] {
  const byCountry = new Set(places.map((p) => p.countryCode ?? "?")).size > 1;
  const g = new Map<string, { key: string; label: string; code: string | null; kind: "country" | "city"; items: Place[] }>();
  for (const p of places) {
    const key = byCountry ? p.countryCode ?? "?" : areaKey(p) || "Other";
    const e = g.get(key) ?? { key, label: byCountry ? p.country ?? "Unknown country" : key, code: p.countryCode, kind: byCountry ? "country" as const : "city" as const, items: [] };
    e.items.push(p);
    g.set(key, e);
  }
  return [...g.values()].sort((a, b) => b.items.length - a.items.length || a.label.localeCompare(b.label));
}
