import { useMemo, useState } from "react";
import { PlaceService } from "../../api/services";
import type { PlaceFilter } from "../../api/types";
import { CategoryChip, Empty, Flag, Pill, PlaceLine, StatusBadge, Thumb } from "../../components/common";
import { QuickAddPlace } from "./QuickAddPlace";
import { CATEGORY, STATUS } from "../../lib/labels";
import { CategoryBadge, GROUPS, groupOf, type CategoryGroup } from "../../lib/categoryIcons";
import { useLoad, useNav } from "../../lib/nav";
import { withMetro } from "../../lib/metro";

export function PlacesView() {
  const nav = useNav();
  const [layout, setLayout] = useState<"grid" | "list">("grid");
  const [filter, setFilter] = useState<PlaceFilter>({ sort: "recent" });
  const [adding, setAdding] = useState(false);
  const { data: loaded, error } = useLoad(() => PlaceService.list(filter), [JSON.stringify(filter)]);
  // Kind of place is filtered by group (Food & drink, Stay, Sights…), the same groups as the map.
  const [group, setGroup] = useState<CategoryGroup | "">("");
  const { data: all = [] } = useLoad(() => PlaceService.list({}), []);
  const places = useMemo(() => {
    // Cities are worked out from your whole library, so a filter never changes which city a place is in.
    const metro = new Map(withMetro(all).map((p) => [p.id, p.metro]));
    return (loaded ?? []).map((p) => ({ ...p, metro: metro.get(p.id) })).filter((p) => !group || groupOf(p.category) === group);
  }, [loaded, all, group]);
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

  // Only offer categories/statuses you actually have (within the chosen country/city), with counts.
  const available = useMemo(() => {
    const inArea = all.filter((p) => (!filter.countryCode || p.countryCode === filter.countryCode) && (!filter.city || p.city === filter.city));
    const tally = (key: (p: (typeof all)[number]) => string) => {
      const m = new Map<string, number>();
      inArea.forEach((p) => m.set(key(p), (m.get(key(p)) ?? 0) + 1));
      return m;
    };
    return { categories: tally((p) => groupOf(p.category)), statuses: tally((p) => p.personalStatus) };
  }, [all, filter.countryCode, filter.city]);

  const set = (patch: Partial<PlaceFilter>) => setFilter((f) => ({ ...f, ...patch }));

  // Grouped by country (biggest first). Inside a country there are no sub-headings, but places in the same
  // city sit together: the city itself first, then its places (in the chosen sort order).
  const groups = useMemo(() => {
    const g = new Map<string, typeof places>();
    places.forEach((p) => {
      const key = p.country ?? "Unknown";
      g.set(key, [...(g.get(key) ?? []), p]);
    });
    const cityOf = (p: (typeof places)[number]) => (p.metro ?? p.city ?? p.canonicalName).toLowerCase();
    return [...g.entries()]
      .sort((a, b) => b[1].length - a[1].length || a[0].localeCompare(b[0]))
      .map(([country, items]) => {
        const perCity = new Map<string, number>();
        items.forEach((p) => perCity.set(cityOf(p), (perCity.get(cityOf(p)) ?? 0) + 1));
        const order = new Map(items.map((p, i) => [p.id, i]));
        const isArea = (p: (typeof places)[number]) => p.category === "city" || p.category === "region";
        const sorted = [...items].sort((a, b) => {
          const ca = cityOf(a), cb = cityOf(b);
          if (ca !== cb) return (perCity.get(cb)! - perCity.get(ca)!) || ca.localeCompare(cb);
          if (isArea(a) !== isArea(b)) return isArea(a) ? -1 : 1;
          return order.get(a.id)! - order.get(b.id)!;
        });
        return [country, sorted] as const;
      });
  }, [places]);
  // Opening a place lets the viewer step through (and move on after removing) in this same order.
  const order = useMemo(() => groups.flatMap(([, items]) => items.map((p) => ({ type: "place" as const, id: p.id }))), [groups]);

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
        <select value={filter.countryCode ?? ""} onChange={(e) => { setGroup(""); set({ countryCode: e.target.value || undefined, city: undefined, status: undefined }); }}>
          <option value="">All countries</option>
          {countries.map(([code, name]) => <option key={code} value={code}>{name}</option>)}
        </select>
        <select value={filter.city ?? ""} onChange={(e) => set({ city: e.target.value || undefined })}>
          <option value="">All cities</option>
          {cities.map((c) => <option key={c}>{c}</option>)}
        </select>
        <select value={group} onChange={(e) => setGroup(e.target.value as CategoryGroup | "")}>
          <option value="">All kinds</option>
          {Object.entries(GROUPS).filter(([k]) => available.categories.has(k)).map(([k, g]) => <option key={k} value={k}>{g.label} ({available.categories.get(k)})</option>)}
        </select>
        <select value={filter.status ?? ""} onChange={(e) => set({ status: e.target.value || undefined })}>
          <option value="">Any status</option>
          {Object.entries(STATUS).filter(([k]) => available.statuses.has(k)).map(([k, s]) => <option key={k} value={k}>{s.emoji} {s.label} ({available.statuses.get(k)})</option>)}
        </select>
        <select value={filter.sort ?? "recent"} onChange={(e) => set({ sort: e.target.value as PlaceFilter["sort"] })}>
          <option value="recent">Recently added</option>
          <option value="name">Name</option>
          <option value="sources">Most sources</option>
        </select>
      </div>

      {error && <p className="bad">{error}</p>}
      {!loaded ? null : places.length === 0 ? (
        filter.search ? <Empty icon="🔎" title="No matches">No saved place matches “{filter.search}”.</Empty> :
        <Empty icon="📍" title="No places yet">Scan your screenshots, import a Reel from Sources, or add a place you know with ＋ Add Place.</Empty>
      ) : layout === "grid" ? (
        <div className="stack">
          {groups.map(([country, items]) => (
            <section key={country} className="country-section">
              <h3 className="country-head"><Flag code={items[0].countryCode} name={country} /> {country}<span className="muted">{items.length}</span></h3>
              <div className="grid">
                {items.map((p) => (
                <div key={p.id} className="tile" onClick={() => nav.openPlace(p.id, order)}>
                  <Thumb path={p.heroImagePath ?? p.thumbnailPath} focus={p.heroFocus} fallback={<CategoryBadge category={p.category} size={44} />} />
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
            </section>
          ))}
        </div>
      ) : (
        <div className="stack">
          {groups.map(([country, items]) => (
            <div key={country}>
              <h3 className="country-head"><Flag code={items[0].countryCode} name={country} /> {country}<span className="muted">{items.length}</span></h3>
              <div className="list">
                {items.map((p) => (
                  <div key={p.id} className="list-row" onClick={() => nav.openPlace(p.id, order)}>
                    <Thumb path={p.heroImagePath ?? p.thumbnailPath} focus={p.heroFocus} fallback={<CategoryBadge category={p.category} size={44} />} />
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
