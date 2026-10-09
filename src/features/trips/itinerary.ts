// Itinerary logic for trips (pure, unit-tested): days, ordering, moving stops between days.
import type { Trip, TripEntry } from "../../api/types";

export interface Stop { id: string; day: number | null }

/** Days shown for a trip: every day that has stops, at least the trip's length, plus any added empty days. */
export function dayList(entries: Stop[], trip: Pick<Trip, "startDate" | "endDate">, extraDays = 0): number[] {
  const used = entries.map((e) => e.day).filter((d): d is number => d != null);
  const length = tripLength(trip) ?? 0;
  const max = Math.max(0, ...used, length) + extraDays;
  return Array.from({ length: max }, (_, i) => i + 1);
}

/** Number of days between the trip's dates (inclusive), if both are set and sensible. */
export function tripLength(trip: Pick<Trip, "startDate" | "endDate">): number | null {
  if (!trip.startDate || !trip.endDate) return null;
  const a = Date.parse(trip.startDate), b = Date.parse(trip.endDate);
  if (Number.isNaN(a) || Number.isNaN(b) || b < a) return null;
  return Math.min(60, Math.round((b - a) / 86_400_000) + 1);
}

/** The calendar date of a day number, when the trip has a start date (e.g. "Sat 28 Mar"). */
export function dayDate(trip: Pick<Trip, "startDate">, day: number): string | null {
  if (!trip.startDate) return null;
  const d = new Date(`${trip.startDate}T00:00:00`);
  if (Number.isNaN(d.getTime())) return null;
  d.setDate(d.getDate() + day - 1);
  return d.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

/** Stops grouped by day (in order), unscheduled last. */
export function byDay<T extends Stop>(entries: T[]): Map<number | null, T[]> {
  const m = new Map<number | null, T[]>();
  for (const e of entries) {
    const list = m.get(e.day);
    if (list) list.push(e); else m.set(e.day, [e]);
  }
  return m;
}

/**
 * Moves stop `id` to `day`, before stop `beforeId` (or to the end of that day). Returns the full new order — what the
 * backend stores — with each stop's day. Unknown ids leave the order unchanged.
 */
export function moveStop(entries: Stop[], id: string, day: number | null, beforeId?: string | null): Stop[] {
  const moving = entries.find((e) => e.id === id);
  if (!moving || id === beforeId) return entries;
  const rest = entries.filter((e) => e.id !== id);
  const moved = { ...moving, day };
  let at = beforeId ? rest.findIndex((e) => e.id === beforeId) : -1;
  if (at < 0) {
    // End of that day: after its last stop, or where the day belongs among the others.
    const lastOfDay = rest.map((e) => e.day).lastIndexOf(day);
    at = lastOfDay >= 0 ? lastOfDay + 1 : rest.findIndex((e) => sortKey(e.day) > sortKey(day));
    if (at < 0) at = rest.length;
  }
  const next = [...rest.slice(0, at), moved, ...rest.slice(at)];
  // Keep days in order (a stop dropped into Day 1 never ends up after Day 3 in the stored order).
  return next.map((e, i) => ({ e, i })).sort((a, b) => sortKey(a.e.day) - sortKey(b.e.day) || a.i - b.i).map(({ e }) => e);
}

/** One step up or down within the whole itinerary (keyboard alternative to dragging). */
export function nudge(entries: Stop[], id: string, delta: -1 | 1): Stop[] {
  const i = entries.findIndex((e) => e.id === id);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= entries.length || entries[j].day !== entries[i].day) return entries;
  const next = [...entries];
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

const sortKey = (day: number | null) => (day == null ? Number.MAX_SAFE_INTEGER : day);

/** Cities and counts for the trip summary, biggest first. */
export function citySummary(entries: TripEntry[]): [string, number][] {
  const m = new Map<string, number>();
  entries.forEach((e) => {
    const key = e.place.metro ?? e.place.city ?? e.place.country ?? "Other";
    m.set(key, (m.get(key) ?? 0) + 1);
  });
  return [...m.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}

export function formatRange(trip: Pick<Trip, "startDate" | "endDate">): string | null {
  const fmt = (s: string) => new Date(`${s}T00:00:00`).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  if (trip.startDate && trip.endDate) return `${fmt(trip.startDate)} – ${fmt(trip.endDate)}`;
  if (trip.startDate) return `From ${fmt(trip.startDate)}`;
  return null;
}
