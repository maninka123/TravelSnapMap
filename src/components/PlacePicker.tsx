import { useEffect, useMemo, useState } from "react";
import { PlaceService } from "../api/services";
import type { Place, PlaceCandidate } from "../api/types";
import { CategoryBadge } from "../lib/categoryIcons";
import { useLoad } from "../lib/nav";
import { ErrorNote, Flag, Modal } from "./common";

/** A saved place as a map candidate (same coordinates and Apple Maps id, so it links to that place). */
export function placeToCandidate(p: Place): PlaceCandidate {
  return {
    name: p.canonicalName, latitude: p.latitude, longitude: p.longitude, address: p.address, city: p.city,
    region: p.region, country: p.country, countryCode: p.countryCode, mapIdentifier: p.mapIdentifier, category: p.category, score: 1,
  };
}

/**
 * Suggestions as you type: places you've already saved first, then Apple Maps results (Apple Maps results
 * that are already saved are marked, so nothing gets added twice).
 */
export function usePlaceSuggestions(query: string, includeSaved = true) {
  const { data: saved = [] } = useLoad(() => (includeSaved ? PlaceService.list({}) : Promise.resolve([] as Place[])), [includeSaved]);
  const [map, setMap] = useState<PlaceCandidate[]>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const q = query.trim();

  useEffect(() => {
    if (q.length < 2) { setMap(undefined); setBusy(false); return; }
    setBusy(true);
    let alive = true;
    const timer = setTimeout(() => {
      PlaceService.searchMap(q)
        .then((r) => { if (alive) { setMap(r); setError(undefined); } })
        .catch((e) => { if (alive) setError(String(e)); })
        .finally(() => { if (alive) setBusy(false); });
    }, 350);
    return () => { alive = false; clearTimeout(timer); };
  }, [q]);

  const savedMatches = useMemo(() => {
    if (q.length < 2) return [];
    const lq = q.toLowerCase();
    const rank = (p: Place) => {
      const names = [p.canonicalName, ...p.alternativeNames].map((n) => n.toLowerCase());
      if (names.some((n) => n.startsWith(lq))) return 0;
      if (names.some((n) => n.split(/[\s,()-]+/).some((w) => w.startsWith(lq)))) return 1;
      if (names.some((n) => n.includes(lq))) return 2;
      return (p.city ?? "").toLowerCase().startsWith(lq) ? 3 : 9;
    };
    return saved.map((p) => [rank(p), p] as const).filter(([r]) => r < 9)
      .sort((a, b) => a[0] - b[0] || a[1].canonicalName.localeCompare(b[1].canonicalName)).slice(0, 5).map(([, p]) => p);
  }, [saved, q]);

  /** The saved place an Apple Maps result is (same Apple Maps id, or same name within ~200 m). */
  const savedFor = (c: PlaceCandidate) => saved.find((p) =>
    (p.mapIdentifier && p.mapIdentifier === c.mapIdentifier) ||
    (p.canonicalName.toLowerCase() === c.name.toLowerCase() && Math.abs(p.latitude - c.latitude) < 0.002 && Math.abs(p.longitude - c.longitude) < 0.002));

  return { savedMatches, map, busy, error, savedFor };
}

/** The two result lists: "Already in your places" and "From Apple Maps". */
export function PlacePickList({ query, includeSaved = true, onPickSaved, onPickMap, action = "Choose" }: {
  query: string; includeSaved?: boolean; action?: string;
  onPickSaved: (p: Place) => void; onPickMap: (c: PlaceCandidate, alreadySaved?: Place) => void;
}) {
  const { savedMatches, map, busy, error, savedFor } = usePlaceSuggestions(query, includeSaved);
  const q = query.trim();
  if (q.length < 2) return <p className="muted small pick-hint">Start typing a place name — your saved places and Apple Maps suggestions appear here.</p>;
  return (
    <div className="pick-list">
      {savedMatches.length > 0 && (
        <section>
          <h5>Already in your places</h5>
          {savedMatches.map((p) => (
            <button key={p.id} className="pick-row" onClick={() => onPickSaved(p)}>
              <CategoryBadge category={p.category} size={30} />
              <span className="pick-text">
                <span className="pick-name">{p.canonicalName}</span>
                <span className="pick-sub"><Flag code={p.countryCode} name={p.country} /> {[p.city !== p.canonicalName ? p.city : null, p.country].filter(Boolean).join(", ")}</span>
              </span>
              <span className="saved-badge">Saved</span>
              <span className="pick-action">{action}</span>
            </button>
          ))}
        </section>
      )}
      <section>
        <h5>From Apple Maps {busy && <span className="pick-spinner" />}</h5>
        <ErrorNote error={error} />
        {map?.length === 0 && !busy && <p className="muted small">No Apple Maps results. Try adding the city or country.</p>}
        {map?.map((c, i) => {
          const existing = savedFor(c);
          return (
            <button key={`${c.mapIdentifier ?? c.name}-${i}`} className="pick-row" onClick={() => onPickMap(c, existing)}>
              <CategoryBadge category={c.category ?? "other"} size={30} />
              <span className="pick-text">
                <span className="pick-name">{c.name}</span>
                <span className="pick-sub"><Flag code={c.countryCode} name={c.country} /> {[c.address, c.country].filter(Boolean).join(" · ")}</span>
              </span>
              {existing && <span className="saved-badge" title={`Already saved as “${existing.canonicalName}”`}>Saved</span>}
              <span className="pick-action">{action}</span>
            </button>
          );
        })}
      </section>
    </div>
  );
}

/** Find a place: saved places first, then Apple Maps. Coordinates always come from the map provider. */
export function PlaceSearchDialog({ title = "Find the place", initialQuery = "", includeSaved = true, onPick, onClose }: {
  title?: string; initialQuery?: string; includeSaved?: boolean; onPick: (c: PlaceCandidate) => void; onClose: () => void;
}) {
  const [query, setQuery] = useState(initialQuery);
  return (
    <Modal title={title} onClose={onClose}>
      <input autoFocus value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Type a place — e.g. Blue Lagoon, Iceland" />
      <PlacePickList query={query} includeSaved={includeSaved}
        onPickSaved={(p) => onPick(placeToCandidate(p))} onPickMap={(c, existing) => onPick(existing ? placeToCandidate(existing) : c)} />
    </Modal>
  );
}
