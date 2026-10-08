import { Search, X } from "lucide-react";
import { useMemo, useRef, useState } from "react";
import type { Place } from "../../api/types";
import { Flag } from "../../components/common";
import { CategoryBadge } from "../../lib/categoryIcons";
import { inScope, scopeFor, suggest, type Scope, type Suggestion } from "./model";

/**
 * Type-ahead search over your saved places, cities and countries (never the internet). Picking a country or city
 * scopes Explore to it (shown as a token you can remove with ✕ or Backspace); picking a place selects it.
 */
export function ExploreSearch({ places, query, onQuery, scope, onScope, onPickPlace }: {
  places: Place[]; query: string; onQuery: (q: string) => void; scope?: Scope; onScope: (s: Scope | undefined) => void; onPickPlace: (p: Place) => void;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const scoped = useMemo(() => places.filter((p) => inScope(p, scope)), [places, scope]);
  const suggestions = useMemo(() => suggest(scoped, query, scope), [scoped, query, scope]);
  const up = () => onScope(scope?.kind === "city" && scope.parent ? scope.parent : undefined);

  const pick = (s: Suggestion) => {
    if (s.kind === "place") {
      setOpen(false);
      onQuery("");
      onPickPlace(s.place);
      inputRef.current?.blur();
    } else {
      onScope(scopeFor(s, scope));
      onQuery("");
      setActive(0);
      setOpen(true); // keep it open so the country's cities (or the city's places) are offered next
    }
  };

  return (
    <div className="search-wrap">
      <label className="search-field">
        <Search size={15} aria-hidden="true" />
        {scope && (
          <span className="search-token">
            {scope.kind === "country" && scope.code && <Flag code={scope.code} name={scope.label} />} {scope.label}
            <button aria-label={`Show all places, not only ${scope.label}`} onMouseDown={(e) => e.preventDefault()} onClick={up}><X size={11} strokeWidth={2.6} /></button>
          </span>
        )}
        <input ref={inputRef} data-search placeholder={scope ? `Search in ${scope.label}` : "Search places, cities, countries"} value={query}
               role="combobox" aria-expanded={open} aria-controls="explore-suggest" aria-autocomplete="list" aria-label="Search saved places"
               onChange={(e) => { onQuery(e.target.value); setOpen(true); setActive(0); }}
               onFocus={() => setOpen(true)}
               onBlur={() => window.setTimeout(() => setOpen(false), 120)}
               onKeyDown={(e) => {
                 if (e.key === "Backspace" && !query && scope) { up(); return; }
                 if (e.key === "Escape") { setOpen(false); if (query) onQuery(""); return; }
                 if (!suggestions.length) return;
                 if (e.key === "ArrowDown") { e.preventDefault(); setOpen(true); setActive((i) => Math.min(i + 1, suggestions.length - 1)); }
                 else if (e.key === "ArrowUp") { e.preventDefault(); setActive((i) => Math.max(i - 1, 0)); }
                 else if (e.key === "Enter" && open) { e.preventDefault(); pick(suggestions[active]); }
               }} />
        {query && <button className="search-clear" aria-label="Clear search" onMouseDown={(e) => e.preventDefault()} onClick={() => onQuery("")}><X size={11} strokeWidth={3} /></button>}
      </label>
      {open && (suggestions.length > 0 || query.trim()) && (
        <div className="search-suggest" role="listbox" id="explore-suggest">
          {!query.trim() && suggestions.length > 0 && (
            <div className="search-suggest-title eyebrow">{scope?.kind === "country" ? `Cities and places in ${scope.label}` : scope ? `Places in ${scope.label}` : "Your countries"}</div>
          )}
          {suggestions.length === 0 && <div className="search-suggest-empty">No saved places {scope ? `in ${scope.label} ` : ""}match “{query.trim()}”.</div>}
          {suggestions.map((s, i) => (
            <button key={`${s.kind}-${s.key}`} role="option" aria-selected={i === active} className={`suggest-row ${i === active ? "active" : ""}`}
                    onMouseDown={(e) => e.preventDefault()} onMouseEnter={() => setActive(i)} onClick={() => pick(s)}>
              {s.kind === "place" ? <CategoryBadge category={s.place.category} size={28} />
                : s.kind === "country" ? <span className="result-icon"><Flag code={s.key} name={s.label} /></span>
                : <CategoryBadge category="city" size={28} />}
              <span className="suggest-text">
                <span className="suggest-name"><Highlight text={s.label} query={query} /></span>
                <span className="suggest-sub">{s.sub}</span>
              </span>
              <span className="suggest-kind">{s.kind === "place" ? "" : s.kind === "city" ? "City" : "Country"}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function Highlight({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  const i = q ? text.toLowerCase().indexOf(q.toLowerCase()) : -1;
  if (i < 0) return <>{text}</>;
  return <>{text.slice(0, i)}<mark>{text.slice(i, i + q.length)}</mark>{text.slice(i + q.length)}</>;
}
