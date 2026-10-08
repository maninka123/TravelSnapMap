import type { LngLatBounds } from "maplibre-gl";
import { Download, LayoutGrid, Map as MapIcon, MapPin, PanelLeft, Plus, SlidersHorizontal } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { PlaceService } from "../../api/services";
import type { PersonalStatus, Place } from "../../api/types";
import { Empty } from "../../components/common";
import { Popover, Segmented } from "../../components/ui";
import { CategoryBadge, GROUPS, type CategoryGroup } from "../../lib/categoryIcons";
import { StatusIcon } from "../../lib/icons";
import { STATUS } from "../../lib/labels";
import { withMetro } from "../../lib/metro";
import { useLoad, useNav } from "../../lib/nav";
import { MapView } from "../map/MapView";
import { QuickAddPlace } from "../places/QuickAddPlace";
import { ExploreSearch } from "./ExploreSearch";
import { Library, type LibrarySort } from "./Library";
import { PlaceList } from "./PlaceList";
import { activeFilterCount, applyFilters, inScope, loadFilters, matchesFilters, QUICK_STATUS, saveFilters, tallies, type ExploreFilters, type Scope } from "./model";

/** Explore: your saved places on the map (with a list that follows it) or as a library — one search, one set of filters. */
export function ExploreView() {
  const nav = useNav();
  const { data: loaded, error } = useLoad(() => PlaceService.list({}), []);
  const { data: warnings = {} } = useLoad(() => PlaceService.warnings(), []);
  // Suburbs/districts are grouped under their city (Surry Hills → Sydney, Pudong → Shanghai).
  const all = useMemo(() => withMetro(loaded ?? []), [loaded]);
  // The map shows confirmed places; unconfirmed ones wait in Import → Review (the library shows them, marked).
  const confirmed = useMemo(() => all.filter((p) => p.verification !== "needsReview"), [all]);

  const [filters, setFilters] = useState<ExploreFilters>(loadFilters);
  useEffect(() => saveFilters(filters), [filters]);
  const update = (patch: Partial<ExploreFilters>) => setFilters((f) => ({ ...f, ...patch }));
  const [selectedId, setSelectedId] = useState<string>();
  const [flyKey, setFlyKey] = useState(0);
  const [hoverId, setHoverId] = useState<string>();
  const [bounds, setBounds] = useState<LngLatBounds>();
  const [inViewOnly, setInViewOnly] = useState(true);
  const [listHidden, setListHidden] = useState(false);
  const [sort, setSort] = useState<LibrarySort>("recent");
  const [adding, setAdding] = useState(false);
  const mode = nav.exploreMode;

  // A scope that no longer has any places (e.g. after deleting) is dropped.
  useEffect(() => {
    if (filters.scope && loaded && !confirmed.some((p) => inScope(p, filters.scope))) update({ scope: undefined });
  }, [loaded]); // eslint-disable-line react-hooks/exhaustive-deps

  // "Show on map" from elsewhere in the app.
  useEffect(() => {
    if (!nav.focusPlaceId || !loaded) return;
    const p = all.find((x) => x.id === nav.focusPlaceId);
    nav.clearFocus();
    if (!p) return;
    nav.setExploreMode("map");
    setFilters((f) => ({ ...f, query: "", scope: undefined, groups: [], statuses: [] }));
    setSelectedId(p.id);
    setFlyKey((k) => k + 1);
  }, [nav.focusPlaceId, loaded]); // eslint-disable-line react-hooks/exhaustive-deps

  const textAndKind = useMemo(() => confirmed.filter((p) => matchesFilters(p, filters)), [confirmed, filters]);
  const mapPlaces = textAndKind;
  const scoped = useMemo(() => textAndKind.filter((p) => inScope(p, filters.scope)), [textAndKind, filters.scope]);
  const listPlaces = useMemo(() => (inViewOnly && bounds ? scoped.filter((p) => bounds.contains([p.longitude, p.latitude])) : scoped), [scoped, bounds, inViewOnly]);
  const libraryPlaces = useMemo(() => applyFilters(all, filters), [all, filters]);
  const counts = useMemo(() => tallies(all.filter((p) => inScope(p, filters.scope))), [all, filters.scope]);
  const selected = all.find((p) => p.id === selectedId);

  const pickPlace = (p: Place) => {
    if (mode === "library") { nav.openPlace(p.id); return; }
    setSelectedId(p.id);
    setFlyKey((k) => k + 1);
  };
  const setScope = (scope: Scope | undefined) => update({ scope });
  const quick = filters.statuses.length === 1 ? filters.statuses[0] : filters.statuses.length === 0 ? "all" : "custom";
  const filterCount = activeFilterCount(filters);
  const empty = loaded && all.length === 0;

  return (
    <div className="explore">
      <header className="explore-bar">
        <ExploreSearch places={mode === "map" ? confirmed : all} query={filters.query} onQuery={(query) => update({ query })}
                       scope={filters.scope} onScope={setScope} onPickPlace={pickPlace} />
        <div className="row" role="group" aria-label="Status" style={{ gap: 4 }}>
          {QUICK_STATUS.map((s) => (
            <button key={s.key} className={`chip-btn ${quick === s.key ? "active" : ""}`} aria-pressed={quick === s.key}
                    onClick={() => update({ statuses: s.key === "all" ? [] : [s.key as PersonalStatus] })}>
              {s.key !== "all" && <StatusIcon status={s.key as PersonalStatus} size={12} />}{s.label}
            </button>
          ))}
        </div>
        <Popover trigger={<button className={`btn small ${filterCount ? "is-on" : ""}`}><SlidersHorizontal size={14} /> Filters{filterCount ? ` · ${filterCount}` : ""}</button>}>
          <FiltersPanel counts={counts} filters={filters} onChange={update} />
        </Popover>
        <Segmented label="View" value={mode} onChange={nav.setExploreMode} options={[
          { value: "map", label: <><MapIcon size={14} /> Map</>, title: "Map with a list that follows it" },
          { value: "library", label: <><LayoutGrid size={14} /> Library</>, title: "All places as cards" },
        ]} />
        {mode === "library" && (
          <select aria-label="Sort" value={sort} onChange={(e) => setSort(e.target.value as LibrarySort)}>
            <option value="recent">Recently added</option><option value="name">Name</option><option value="sources">Most sources</option>
          </select>
        )}
        {mode === "map" && (
          <button className={`btn small icon-only ${listHidden ? "" : "is-on"}`} onClick={() => setListHidden((h) => !h)}
                  title={listHidden ? "Show the list" : "Hide the list"} aria-label={listHidden ? "Show the list" : "Hide the list"} aria-pressed={!listHidden}>
            <PanelLeft size={15} />
          </button>
        )}
        <button className="btn primary small" onClick={() => setAdding(true)}><Plus size={14} /> Add place</button>
      </header>

      {error && <div className="error-note" style={{ margin: 16 }}>Couldn't load your places: {error}</div>}
      <div className="explore-body">
        {mode === "map" ? (
          <div className={`explore-split ${listHidden ? "list-hidden" : ""}`}>
            <PlaceList places={listPlaces} total={scoped.length} inViewOnly={inViewOnly} onInViewOnly={setInViewOnly}
                       selectedId={selectedId} onPick={pickPlace} onHover={setHoverId}
                       onGroup={(g) => setScope(g.kind === "country"
                         ? { kind: "country", key: g.code ?? g.key, label: g.label, code: g.code }
                         : { kind: "city", key: g.key, label: g.label, code: g.code, parent: filters.scope?.kind === "country" ? filters.scope : g.code ? { kind: "country", key: g.code, label: g.code, code: g.code } : undefined })} />
            <MapView places={mapPlaces} scope={filters.scope} onScope={setScope} selected={selected}
                     onSelect={(p) => setSelectedId(p?.id)} flyKey={flyKey} hoverId={hoverId} onBounds={setBounds}
                     onOpen={(p) => nav.openPlace(p.id, scoped.map((x) => ({ type: "place" as const, id: x.id })))} />
            {empty && <FirstPlaces onAdd={() => setAdding(true)} />}
          </div>
        ) : empty ? (
          <div style={{ flex: 1, overflow: "auto" }}><FirstPlaces onAdd={() => setAdding(true)} inline /></div>
        ) : libraryPlaces.length === 0 && loaded ? (
          <div style={{ flex: 1 }}>
            <Empty icon={<MapPin size={26} />} title="No places match" actions={<button className="btn" onClick={() => setFilters((f) => ({ ...f, query: "", groups: [], statuses: [], scope: undefined }))}>Clear search and filters</button>}>
              Nothing saved {filters.scope ? `in ${filters.scope.label} ` : ""}matches {filters.query ? `“${filters.query}”` : "these filters"}.
            </Empty>
          </div>
        ) : (
          <Library places={libraryPlaces} sort={sort} warnings={warnings}
                   onOpen={(p, order) => nav.openPlace(p.id, order.map((x) => ({ type: "place" as const, id: x.id })))} />
        )}
      </div>
      {adding && <QuickAddPlace onClose={() => setAdding(false)} />}
    </div>
  );
}

/** Kinds of place and statuses as toggles, with counts for the current country/city. Applies instantly. */
function FiltersPanel({ counts, filters, onChange }: { counts: ReturnType<typeof tallies>; filters: ExploreFilters; onChange: (p: Partial<ExploreFilters>) => void }) {
  const toggle = <T,>(list: T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
  return (
    <div style={{ width: 340 }}>
      <div className="filters-section">
        <h5>Kind of place</h5>
        <div className="filters-chips">
          {[...counts.groups.entries()].sort((a, b) => b[1] - a[1]).map(([g, n]) => (
            <button key={g} className={`chip-btn ${filters.groups.includes(g) ? "active" : ""}`} aria-pressed={filters.groups.includes(g)}
                    onClick={() => onChange({ groups: toggle(filters.groups, g as CategoryGroup) })}>
              <CategoryBadge category={g} size={16} /> {GROUPS[g].label} <span className="chip-count">{n}</span>
            </button>
          ))}
          {counts.groups.size === 0 && <span className="muted small">No places yet.</span>}
        </div>
      </div>
      <div className="filters-section">
        <h5>Status</h5>
        <div className="filters-chips">
          {(Object.keys(STATUS) as PersonalStatus[]).filter((s) => counts.statuses.has(s)).map((s) => (
            <button key={s} className={`chip-btn ${filters.statuses.includes(s) ? "active" : ""}`} aria-pressed={filters.statuses.includes(s)}
                    onClick={() => onChange({ statuses: toggle(filters.statuses, s) })}>
              <StatusIcon status={s} size={12} /> {STATUS[s].label} <span className="chip-count">{counts.statuses.get(s)}</span>
            </button>
          ))}
        </div>
      </div>
      <div className="filters-foot">
        <button className="btn ghost small" disabled={filters.groups.length + filters.statuses.length === 0} onClick={() => onChange({ groups: [], statuses: [] })}>Clear filters</button>
      </div>
    </div>
  );
}

/** Nothing saved yet: say what to do next, over the map or in the library. */
function FirstPlaces({ onAdd, inline = false }: { onAdd: () => void; inline?: boolean }) {
  const nav = useNav();
  const body = (
    <Empty icon={<MapPin size={26} />} title="Your travel map starts here"
           actions={<>
             <button className="btn primary" onClick={() => nav.go("import", { import: "add" })}><Download size={14} /> Import screenshots or Reels</button>
             <button className="btn" onClick={onAdd}><Plus size={14} /> Add a place you know</button>
           </>}>
      Import travel screenshots or Instagram Reels and TravelSnapMap finds the places in them — or add a place yourself.
    </Empty>
  );
  if (inline) return body;
  return <div className="map-empty"><div className="card">{body}</div></div>;
}
