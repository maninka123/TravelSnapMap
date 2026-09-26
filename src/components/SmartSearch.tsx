import { useEffect, useState } from "react";
import { PlaceService } from "../api/services";
import type { Fact, SearchChip, SmartSearchResult } from "../api/types";
import { CATEGORY, FACT, SOURCE } from "../lib/labels";
import { Flag } from "./common";

export const SEARCH_EXAMPLES = [
  "Places in Japan good for sunrise",
  "Restaurants I saved from Instagram",
  "Where someone mentioned avoiding crowds",
  "Things requiring advance booking",
];

/**
 * Smart Search as you type (debounced). Understands countries, cities, kinds of place, sources, your
 * visit status and travel ideas like "sunrise" or "avoid crowds" — all matched locally against saved facts.
 */
export function useSmartSearch(query: string, verifiedOnly = false): { result?: SmartSearchResult; busy: boolean } {
  const [result, setResult] = useState<SmartSearchResult>();
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    const q = query.trim();
    if (!q) {
      setResult(undefined);
      setBusy(false);
      return;
    }
    setBusy(true);
    let alive = true;
    const timer = setTimeout(() => {
      PlaceService.smartSearch(q, verifiedOnly)
        .then((r) => { if (alive) setResult(r); })
        .catch(() => { if (alive) setResult({ chips: [], hits: [] }); })
        .finally(() => { if (alive) setBusy(false); });
    }, 220);
    return () => { alive = false; clearTimeout(timer); };
  }, [query, verifiedOnly]);
  return { result, busy };
}

/** How the query was understood, as small tokens ("🇯🇵 Japan", "🌅 Sunrise", "📷 Instagram"). */
export function SearchChips({ chips, count }: { chips: SearchChip[]; count?: number }) {
  if (chips.length === 0 && count === undefined) return null;
  return (
    <div className="search-chips">
      {chips.map((c, i) => (
        <span key={i} className={`search-chip kind-${c.kind}`}>
          {c.kind === "country" ? <Flag code={c.code} name={c.label} /> :
            c.kind === "category" && c.code ? <span>{CATEGORY[c.code as keyof typeof CATEGORY]?.emoji}</span> :
            c.emoji ? <span>{c.emoji}</span> : null}
          {c.label}
        </span>
      ))}
      {count !== undefined && <span className="muted small">{count === 0 ? "No matches" : `${count} match${count === 1 ? "" : "es"}`}</span>}
    </div>
  );
}

/** One saved fact that made a place match, with where it came from. */
export function MatchSnippet({ fact }: { fact: Fact }) {
  const source = fact.reelId ? "Instagram Reel" : fact.sourceType && fact.sourceType !== "unknown" ? SOURCE[fact.sourceType] : "Screenshot";
  return (
    <div className="match-snippet">
      <span className="match-icon">{FACT[fact.type]?.emoji ?? "💬"}</span>
      <span className="match-text">{fact.text}</span>
      <span className="match-source">{source}{fact.creator ? ` · ${fact.creator}` : ""}</span>
    </div>
  );
}
