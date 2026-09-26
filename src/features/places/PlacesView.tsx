import { useMemo, useState } from "react";
import { PlaceService } from "../../api/services";
import type { PlaceFilter } from "../../api/types";
import { CategoryChip, Empty, Flag, Pill, PlaceLine, StatusBadge, Thumb } from "../../components/common";
import { QuickAddPlace } from "./QuickAddPlace";
import { CATEGORY, STATUS } from "../../lib/labels";
import { useLoad, useNav } from "../../lib/nav";

export function PlacesView() {
  const nav = useNav();
  const [layout, setLayout] = useState<"grid" | "list">("grid");
  const [filter, setFilter] = useState<PlaceFilter>({ sort: "recent" });
  const [adding, setAdding] = useState(false);
  const { data: places = [], error } = useLoad(() => PlaceService.list(filter), [JSON.stringify(filter)]);
  const { data: all = [] } = useLoad(() => PlaceService.list({}), []);
  const { data: warnings = {} } = useLoad(() => PlaceService.warnings(), []);

  const countries = useMemo(() => {
    const m = new Map<string, string>();
    all.forEach((p) => p.countryCode && m.set(p.countryCode, p.country ?? p.countryCode));
    return [...m.entries()].sort((a, b) => a[1].localeCompare(b[1]));
  }, [all]);
  const cities = useMemo(() => {
    const s = new Set<string>();
    all.filter((p) => !filter.countryCode || p.countryCode === filter.countryCode).forEach((p) => p.city && s.add(p.city));
    return [...s].sort();
  }, [all, filter.countryCode]);

  const set = (patch: Partial<PlaceFilter>) => setFilter((f) => ({ ...f, ...patch }));

  // Group by country for the list layout.
  const groups = useMemo(() => {
    const g = new Map<string, typeof places>();
    places.forEach((p) => {
      const key = p.country ?? "Unknown";
      g.set(key, [...(g.get(key) ?? []), p]);
    });
    return [...g.entries()].sort((a, b) => b[1].length - a[1].length);
  }, [places]);

  return (
    <div className="page">
      <div className="page-head">
        <h2>Places</h2>
        <button className="btn primary" onClick={() => setAdding(true)}>＋ Add Place</button>
        <div className="segmented">
          <button className={layout === "grid" ? "active" : ""} onClick={() => setLayout("grid")}>Grid</button>
          <button className={layout === "list" ? "active" : ""} onClick={() => setLayout("list")}>List</button>
        </div>
      </div>

      <div className="toolbar">
        <input className="grow" style={{ maxWidth: 360 }} placeholder="Search names, cities, tips, creators, apps…"
               value={filter.search ?? ""} onChange={(e) => set({ search: e.target.value || undefined })} />
        <select value={filter.countryCode ?? ""} onChange={(e) => set({ countryCode: e.target.value || undefined, city: undefined })}>
          <option value="">All countries</option>
          {countries.map(([code, name]) => <option key={code} value={code}>{name}</option>)}
        </select>
        <select value={filter.city ?? ""} onChange={(e) => set({ city: e.target.value || undefined })}>
          <option value="">All cities</option>
          {cities.map((c) => <option key={c}>{c}</option>)}
        </select>
        <select value={filter.category ?? ""} onChange={(e) => set({ category: e.target.value || undefined })}>
          <option value="">All categories</option>
          {Object.entries(CATEGORY).map(([k, c]) => <option key={k} value={k}>{c.emoji} {c.label}</option>)}
        </select>
        <select value={filter.status ?? ""} onChange={(e) => set({ status: e.target.value || undefined })}>
          <option value="">Any status</option>
          {Object.entries(STATUS).map(([k, s]) => <option key={k} value={k}>{s.emoji} {s.label}</option>)}
        </select>
        <select value={filter.sort ?? "recent"} onChange={(e) => set({ sort: e.target.value as PlaceFilter["sort"] })}>
          <option value="recent">Recently added</option>
          <option value="name">Name</option>
          <option value="sources">Most sources</option>
        </select>
      </div>

      {error && <p className="bad">{error}</p>}
      {places.length === 0 ? (
        filter.search ? <Empty icon="🔎" title="No matches">No saved place matches “{filter.search}”.</Empty> :
        <Empty icon="📍" title="No places yet">Scan your screenshots, import a Reel from Sources, or add a place you know with ＋ Add Place.</Empty>
      ) : layout === "grid" ? (
        <div className="grid">
          {places.map((p) => (
            <div key={p.id} className="tile" onClick={() => nav.openPlace(p.id)}>
              <Thumb path={p.heroImagePath ?? p.thumbnailPath} fallback={CATEGORY[p.category]?.emoji} />
              <div className="tile-body">
                <div className="row"><span className="tile-title grow">{p.canonicalName}</span><StatusBadge place={p} /></div>
                <PlaceLine place={p} />
                <div className="row wrap">
                  <CategoryChip category={p.category} />
                  <span className="muted small">{p.sourceCount} source{p.sourceCount === 1 ? "" : "s"}</span>
                  {p.verification === "needsReview" && <Pill tone="warn">unconfirmed</Pill>}
                  {warnings[p.id] && <Pill tone="warn" title={warnings[p.id].map((w) => w.title).join("\n")}>⚠️ {warnings[p.id].length}</Pill>}
                  {p.memoryCount > 0 && <span className="muted small">📷 {p.memoryCount}</span>}
                </div>

              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="stack">
          {groups.map(([country, items]) => (
            <div key={country}>
              <h4 className="row" style={{ margin: "10px 0 6px", gap: 6 }}><Flag code={items[0].countryCode} name={country} /> {country} · {items.length}</h4>
              <div className="list">
                {items.map((p) => (
                  <div key={p.id} className="list-row" onClick={() => nav.openPlace(p.id)}>
                    <Thumb path={p.heroImagePath ?? p.thumbnailPath} fallback={CATEGORY[p.category]?.emoji} />
                    <div className="grow">
                      <div className="strong">{p.canonicalName}</div>
                      <div className="muted small">{[p.city, CATEGORY[p.category]?.label].filter(Boolean).join(" · ")}</div>
                    </div>
                    <span className="muted small">{p.sourceCount} sources</span>
                    {warnings[p.id] && <span title={warnings[p.id].map((w) => w.title).join("\n")}>⚠️</span>}
                    <StatusBadge place={p} />
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
      {adding && <QuickAddPlace onClose={() => setAdding(false)} />}
    </div>
  );
}
