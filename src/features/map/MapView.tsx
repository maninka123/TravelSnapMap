import maplibregl, { type GeoJSONSource, type LngLatBounds, type Map as MLMap } from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import { AppService, PhotoLibraryService } from "../../api/services";
import { ScanControls } from "../../components/ScanControls";
import { PlaceService } from "../../api/services";
import type { Place, PersonalStatus } from "../../api/types";
import { Flag, Thumb } from "../../components/common";
import { CATEGORY, STATUS } from "../../lib/labels";
import { openUrl } from "../../lib/open";
import { QuickAddPlace } from "../places/QuickAddPlace";
import { useAction, useLoad, useNav } from "../../lib/nav";
import { CategoryBadge, CategoryGlyph, colorOf, GROUPS, groupOf, pinImage, type CategoryGroup } from "../../lib/categoryIcons";
import { areaAggregates, areaGeoJSON, areaKey, countryAggregates, countryGeoJSON, ensureAreaImages, ensureCountryImages } from "../../lib/countryBubbles";
import { getMapStyleId, MAP_STYLES, resolveMapStyle, setMapStyleId, type MapStyleId } from "../../lib/mapStyle";

/** Base-map labels that compete with your places: road names/shields, POIs, water lines, villages. */
const NOISY_LABELS = /^(highway-name|highway-shield|road_shield|road-shield|waterway|water_name_line|airport|poi|label_other|label_village)/;

/** Hides the busiest base-map labels; call once the style has loaded. */
export function quietBasemap(map: MLMap) {
  for (const layer of map.getStyle().layers ?? []) {
    if (layer.type === "symbol" && NOISY_LABELS.test(layer.id)) map.setLayoutProperty(layer.id, "visibility", "none");
  }
}

const CLUSTER = "#0f8a7e";

export function MapView() {
  const nav = useNav();
  // The map shows verified places only; provisional ones wait in the Review inbox.
  const { data: loaded } = useLoad(() => PlaceService.list({ verifiedOnly: true }), []);
  const places = useMemo(() => loaded ?? [], [loaded]);
  const [categories, setCategories] = useState<CategoryGroup[]>([]);
  const [statuses, setStatuses] = useState<PersonalStatus[]>([]);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Place>();
  const [adding, setAdding] = useState(false);
  const [suggestOpen, setSuggestOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [areaOpen, setAreaOpen] = useState(() => { try { return localStorage.getItem("map.areaOpen") === "1"; } catch { return false; } });
  const [bounds, setBounds] = useState<LngLatBounds>();
  const [styleId, setStyleId] = useState<MapStyleId>(getMapStyleId);
  const [styleMenu, setStyleMenu] = useState(false);
  const selectedRef = useRef<string>("");
  const enterScopeRef = useRef<(s: Scope | undefined) => void>(() => {});
  const scopeRef = useRef<Scope | undefined>(undefined);
  const appliedStyle = useRef(resolveMapStyle(getMapStyleId()).url);

  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MLMap | undefined>(undefined);
  const placesRef = useRef<Place[]>([]);

  useEffect(() => { try { localStorage.setItem("map.areaOpen", areaOpen ? "1" : "0"); } catch { /* per-viewer convenience only */ } }, [areaOpen]);

  // A country or city picked from the search: everything (pins, suggestions, filters, list) is limited to it.
  const [scope, setScope] = useState<Scope>();
  // How the map groups places, following Country → City/Region → Place.
  // The view switch: Smart (Country → City → Place) or a fixed level. Remembered on this Mac.
  const [view, setView] = useState<MapViewKind>(() => {
    try { const v = localStorage.getItem("map.view") as MapViewKind | null; return v && VIEWS.some((x) => x.key === v) ? v : "smart"; } catch { return "smart"; }
  });
  useEffect(() => { try { localStorage.setItem("map.view", view); } catch { /* convenience only */ } }, [view]);
  const plan = planFor(view, scope);
  const planRef = useRef<Plan>(plan);
  const viewRef = useRef<MapViewKind>(view);
  viewRef.current = view;
  const scoped = useMemo(() => places.filter((p) => !scope || (scope.kind === "country" ? p.countryCode === scope.key : areaKey(p) === scope.key)), [places, scope]);

  // Drop filter choices that don't exist in the new country/city, so a hidden filter never empties the map.
  useEffect(() => {
    const keep = <T,>(list: T[], has: (v: T) => boolean) => (list.every(has) ? list : list.filter(has)); // same array when unchanged: no re-render
    setCategories((c) => keep(c, (x) => scoped.some((p) => groupOf(p.category) === x)));
    setStatuses((st) => keep(st, (x) => scoped.some((p) => p.personalStatus === x)));
  }, [scoped]);

  // Keep the open card in sync after edits (e.g. a status change) and close it if the place is gone.
  useEffect(() => {
    setSelected((s) => (s ? places.find((p) => p.id === s.id) : s));
  }, [places]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim();
    return scoped.filter((p) =>
      (categories.length === 0 || categories.includes(groupOf(p.category))) &&
      (statuses.length === 0 || statuses.includes(p.personalStatus)) &&
      (!q || [p.canonicalName, ...p.alternativeNames, p.city, p.country].join(" ").toLowerCase().includes(q)));
  }, [scoped, categories, statuses, search]);

  // Saved places inside the visible map area (not an internet search — only your own places).
  const inView = useMemo(() => (bounds ? filtered.filter((p) => bounds.contains([p.longitude, p.latitude])) : filtered), [filtered, bounds]);

  const suggestions = useMemo(() => suggest(scoped, search, scope), [scoped, search, scope]);

  const enterScope = (next: Scope | undefined) => {
    setScope(next);
    if (next?.kind === "country") next.compact = isCompact(mapRef.current, places.filter((p) => p.countryCode === next.key));
    setSearch("");
    setActive(0);
    const map = mapRef.current;
    const inScope = places.filter((p) => !next || (next.kind === "country" ? p.countryCode === next.key : areaKey(p) === next.key));
    // A city: zoom in past the clustering level so every place shows with its own icon.
    // A country whose places sit close together (e.g. Sri Lanka): zoom just past clustering so all show.
    if (map) fit(map, inScope, next?.kind === "city" ? CITY_ZOOM : next?.kind === "country" ? "compact" : undefined);
  };

  const pick = (s: Suggestion) => {
    const map = mapRef.current;
    if (s.kind === "place") {
      setSuggestOpen(false);
      setSearch(s.place.canonicalName);
      setSelected(s.place);
      map?.easeTo({ center: [s.place.longitude, s.place.latitude], zoom: Math.max(map.getZoom(), 12) });
    } else {
      // Keep the list open so the country's cities (or the city's places) are offered next.
      enterScope({
        kind: s.kind, key: s.key, label: s.label,
        code: s.kind === "country" ? s.key : s.code,
        parent: s.kind === "city" && scope?.kind === "country" ? scope : undefined,
      });
      setSuggestOpen(true);
    }
  };

  // Create the map once.
  useEffect(() => {
    if (!containerRef.current) return;
    const map = new maplibregl.Map({ container: containerRef.current, style: resolveMapStyle().url, center: [20, 25], zoom: 1.4, attributionControl: { compact: true } });
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "bottom-left");
    map.addControl(new FitAllControl(() => fit(map, placesRef.current)), "bottom-left");
    mapRef.current = map;
    const hover = new maplibregl.Popup({ closeButton: false, closeOnClick: false, offset: 18, className: "map-hover" });

    // Runs for the first style and again whenever you pick another map style (setStyle drops custom layers).
    map.on("style.load", () => {
      const dark = resolveMapStyle(getMapStyleId()).dark;
      quietBasemap(map);
      map.addSource("places", {
        type: "geojson", data: toGeoJSON(placesRef.current), ...planRef.current.clustering,
        // Per-group counts inside each cluster, for the hover summary ("18 Sights · 5 Food & drink").
        clusterProperties: Object.fromEntries(Object.keys(GROUPS).map((g) => [g, ["+", ["case", ["==", ["get", "category"], g], 1, 0]]])),
      });
      const aggs = planRef.current.countries ? countryAggregates(placesRef.current) : [];
      const areaData = planRef.current.areas ? areaAggregates(placesRef.current).areas : [];
      ensureAreaImages(map, areaData);
      map.addSource("areas", {
        type: "geojson", data: areaGeoJSON(areaData), cluster: true, clusterRadius: 70, clusterMaxZoom: AREA_MAX_ZOOM - 1,
        clusterProperties: { total: ["+", ["get", "count"]] },
      });
      map.addSource("countries", { type: "geojson", data: countryGeoJSON(aggs) });
      void ensureCountryImages(map, aggs, () => (map.getSource("countries") as GeoJSONSource | undefined)?.setData(countryGeoJSON(countryAggregates(scopeRef.current ? [] : placesRef.current))));
      // Category pins (images are dropped whenever the style changes, so add them each time).
      for (const c of Object.keys(GROUPS)) {
        if (!map.hasImage(`pin-${c}`)) map.addImage(`pin-${c}`, pinImage(c), { pixelRatio: 2 });
        if (!map.hasImage(`pin-sel-${c}`)) map.addImage(`pin-sel-${c}`, pinImage(c, true), { pixelRatio: 2 });
      }
      // Clusters: size by count (1–4 · 5–14 · 15–29 · 30+) with a soft translucent ring.
      const size = ["step", ["get", "point_count"], 15, 5, 19, 15, 24, 30, 29] as unknown as number;
      map.addLayer({
        id: "cluster-ring", type: "circle", source: "places", filter: ["has", "point_count"],
        paint: { "circle-color": CLUSTER, "circle-opacity": 0.2, "circle-radius": ["+", size, 7] as unknown as number },
      });
      map.addLayer({
        id: "clusters", type: "circle", source: "places", filter: ["has", "point_count"],
        paint: { "circle-color": CLUSTER, "circle-radius": size, "circle-stroke-width": 2, "circle-stroke-color": "#ffffff" },
      });
      map.addLayer({
        id: "cluster-count", type: "symbol", source: "places", filter: ["has", "point_count"],
        layout: { "text-field": ["get", "point_count_abbreviated"], "text-size": 13, "text-font": ["Noto Sans Bold"], "text-allow-overlap": true },
        paint: { "text-color": "#ffffff" },
      });
      // Single places: a category icon badge. The selected one is larger.
      map.addLayer({
        id: "place-points", type: "symbol", source: "places", filter: ["!", ["has", "point_count"]],
        layout: { "icon-image": ["concat", "pin-", ["get", "category"]], "icon-allow-overlap": true, "icon-ignore-placement": true },
        paint: { "icon-opacity": ["case", ["==", ["get", "status"], "notInterested"], 0.4, 1] },
      });
      map.addLayer({
        id: "place-selected", type: "symbol", source: "places", filter: ["==", ["get", "id"], ""],
        layout: { "icon-image": ["concat", "pin-sel-", ["get", "category"]], "icon-allow-overlap": true, "icon-ignore-placement": true },
      });
      map.addLayer({
        id: "place-labels", type: "symbol", source: "places", filter: ["!", ["has", "point_count"]], minzoom: 11,
        layout: { "text-field": ["get", "name"], "text-size": 12, "text-offset": [0, 1.3], "text-anchor": "top", "text-font": ["Noto Sans Bold"], "text-max-width": 10 },
        paint: { "text-color": dark ? "#f2f2f7" : "#1c1c1e", "text-halo-color": dark ? "#1c1c1e" : "#ffffff", "text-halo-width": 1.6 },
      });
      const sel = ["all", ["!", ["has", "point_count"]], ["==", ["get", "id"], selectedRef.current]] as maplibregl.FilterSpecification;
      map.setFilter("place-selected", sel);
      // Inside a picked country: one bubble per city/region. Neighbouring cities merge into a round total bubble.
      const areaSize = ["step", ["get", "total"], 16, 5, 20, 15, 25, 30, 30] as unknown as number;
      map.addLayer({
        id: "area-cluster-ring", type: "circle", source: "areas", filter: ["has", "point_count"], 
        paint: { "circle-color": CLUSTER, "circle-opacity": 0.2, "circle-radius": ["+", areaSize, 7] as unknown as number },
      });
      map.addLayer({
        id: "area-clusters", type: "circle", source: "areas", filter: ["has", "point_count"],
        paint: { "circle-color": CLUSTER, "circle-radius": areaSize, "circle-stroke-width": 2, "circle-stroke-color": "#ffffff" },
      });
      map.addLayer({
        id: "area-cluster-count", type: "symbol", source: "areas", filter: ["has", "point_count"],
        layout: { "text-field": ["to-string", ["get", "total"]], "text-size": 13, "text-font": ["Noto Sans Bold"], "text-allow-overlap": true },
        paint: { "text-color": "#ffffff" },
      });
      map.addLayer({
        id: "area-bubbles", type: "symbol", source: "areas", filter: ["!", ["has", "point_count"]],
        layout: { "icon-image": ["get", "image"], "icon-allow-overlap": true, "icon-ignore-placement": true, "symbol-sort-key": ["-", 0, ["get", "count"]] },
      });
      // One flag bubble per country.
      map.addLayer({
        id: "country-bubbles", type: "symbol", source: "countries",
        layout: { "icon-image": ["get", "image"], "icon-allow-overlap": true, "icon-ignore-placement": true, "symbol-sort-key": ["-", 0, ["get", "count"]] },
      });
      applyPlan(map, planRef.current);
    });

    map.on("load", () => {

      map.on("click", "clusters", async (e) => {
        const feature = map.queryRenderedFeatures(e.point, { layers: ["clusters"] })[0];
        const source = map.getSource("places") as GeoJSONSource;
        const zoom = await source.getClusterExpansionZoom(feature.properties.cluster_id);
        map.easeTo({ center: (feature.geometry as GeoJSON.Point).coordinates as [number, number], zoom });
      });
      map.on("click", "place-points", (e) => {
        // Several pins on (almost) the same spot: offer a short list instead of guessing which one.
        const box: [maplibregl.PointLike, maplibregl.PointLike] = [[e.point.x - 14, e.point.y - 14], [e.point.x + 14, e.point.y + 14]];
        const ids = [...new Set(map.queryRenderedFeatures(box, { layers: ["place-points"] }).map((f) => f.properties?.id as string))];
        const here = ids.map((id) => placesRef.current.find((p) => p.id === id)).filter((p): p is Place => !!p);
        if (here.length > 1) showChooser(map, e.lngLat, here, (p) => setSelected(p));
        else setSelected(here[0]);
      });
      map.on("mousemove", "clusters", (e) => {
        const props = e.features?.[0]?.properties ?? {};
        const at = (e.features?.[0]?.geometry as GeoJSON.Point | undefined)?.coordinates as [number, number] | undefined;
        const counts = Object.fromEntries(Object.keys(GROUPS).map((g) => [g, Number(props[g] ?? 0)]));
        if (props.point_count && at) hover.setLngLat(at).setHTML(hoverCard(`${props.point_count} saved places`, "Click to zoom in", counts)).addTo(map);
      });
      map.on("mouseleave", "clusters", () => hover.remove());
      map.on("mousemove", "country-bubbles", (e) => {
        const p = e.features?.[0]?.properties;
        const at = (e.features?.[0]?.geometry as GeoJSON.Point | undefined)?.coordinates as [number, number] | undefined;
        if (p && at) hover.setLngLat(at).setHTML(hoverCard(p.name, `${p.count} saved place${p.count === 1 ? "" : "s"}`, countsFor(placesRef.current.filter((x) => x.countryCode === p.cc)))).addTo(map);
      });
      map.on("mouseleave", "country-bubbles", () => hover.remove());
      map.on("mousemove", "area-bubbles", (e) => {
        const p = e.features?.[0]?.properties;
        const at = (e.features?.[0]?.geometry as GeoJSON.Point | undefined)?.coordinates as [number, number] | undefined;
        if (p && at) hover.setLngLat(at).setHTML(hoverCard(p.key, `${p.count} saved places · click to open`, countsFor(placesRef.current.filter((x) => areaKey(x) === p.key)))).addTo(map);
      });
      map.on("mouseleave", "area-bubbles", () => hover.remove());
      map.on("mousemove", "area-clusters", async (e) => {
        const f = e.features?.[0];
        const at = (f?.geometry as GeoJSON.Point | undefined)?.coordinates as [number, number] | undefined;
        if (!f || !at) return;
        const leaves = await (map.getSource("areas") as GeoJSONSource).getClusterLeaves(f.properties.cluster_id, 50, 0);
        const names = leaves.sort((a, b) => (b.properties?.count ?? 0) - (a.properties?.count ?? 0)).map((l) => String(l.properties?.key));
        const shown = names.slice(0, 3).join(", ") + (names.length > 3 ? ` +${names.length - 3}` : "");
        const members = new Set(names);
        hover.setLngLat(at).setHTML(hoverCard(shown, `${f.properties.total} saved places · click to zoom in`,
          countsFor(placesRef.current.filter((x) => members.has(areaKey(x)))))).addTo(map);
      });
      map.on("mouseleave", "area-clusters", () => hover.remove());
      map.on("click", "area-clusters", async (e) => {
        const f = map.queryRenderedFeatures(e.point, { layers: ["area-clusters"] })[0];
        hover.remove();
        const zoom = await (map.getSource("areas") as GeoJSONSource).getClusterExpansionZoom(f.properties.cluster_id);
        map.easeTo({ center: (f.geometry as GeoJSON.Point).coordinates as [number, number], zoom });
      });
      map.on("click", "area-bubbles", (e) => {
        const p = e.features?.[0]?.properties;
        hover.remove();
        const parent = scopeRef.current;
        if (!p) return;
        const first = placesRef.current.find((x) => areaKey(x) === p.key);
        const countryScope: Scope | undefined = parent?.kind === "country" ? parent
          : first?.countryCode ? { kind: "country", key: first.countryCode, label: first.country ?? first.countryCode, code: first.countryCode } : undefined;
        enterScopeRef.current({ kind: "city", key: p.key, label: p.key, code: first?.countryCode ?? parent?.code ?? null, parent: countryScope });
      });
      map.on("click", "country-bubbles", (e) => {
        const p = e.features?.[0]?.properties;
        hover.remove();
        if (!p) return;
        if (viewRef.current === "countries") setView("smart"); // next step: that country's cities
        enterScopeRef.current({ kind: "country", key: p.cc, label: p.name, code: p.cc });
      });
      for (const layer of ["clusters", "place-points", "country-bubbles", "area-bubbles", "area-clusters"]) {
        map.on("mouseenter", layer, () => { map.getCanvas().style.cursor = "pointer"; });
        map.on("mouseleave", layer, () => { map.getCanvas().style.cursor = ""; });
      }
      map.on("moveend", () => setBounds(map.getBounds()));
      fit(map, placesRef.current);
      setBounds(map.getBounds());
    });
    return () => map.remove();
  }, []);

  enterScopeRef.current = enterScope;
  scopeRef.current = scope;

  // Push data into the map and show the layers the current view needs (and only those).
  const planKey = JSON.stringify(plan);
  useEffect(() => {
    placesRef.current = filtered;
    planRef.current = plan;
    const map = mapRef.current;
    if (!map?.getSource("places")) return;
    const pins = map.getSource("places") as GeoJSONSource;
    pins.setClusterOptions(plan.clustering);
    pins.setData(toGeoJSON(filtered));
    const areaData = plan.areas ? areaAggregates(filtered).areas : [];
    ensureAreaImages(map, areaData);
    (map.getSource("areas") as GeoJSONSource).setData(areaGeoJSON(areaData));
    const countries = map.getSource("countries") as GeoJSONSource;
    const aggs = plan.countries ? countryAggregates(filtered) : [];
    countries.setData(countryGeoJSON(aggs));
    void ensureCountryImages(map, aggs, () => countries.setData(countryGeoJSON(aggs)));
    applyPlan(map, plan);
  }, [filtered, planKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Switch base map; the choice is remembered for next time.
  useEffect(() => {
    setMapStyleId(styleId);
    const url = resolveMapStyle(styleId).url;
    if (mapRef.current && appliedStyle.current !== url) mapRef.current.setStyle(url, { diff: false });
    appliedStyle.current = url;
  }, [styleId]);

  // Highlight the selected pin.
  useEffect(() => {
    selectedRef.current = selected?.id ?? "";
    const map = mapRef.current;
    if (!map?.getLayer("place-selected")) return;
    const filter = ["all", ["!", ["has", "point_count"]], ["==", ["get", "id"], selected?.id ?? ""]] as maplibregl.FilterSpecification;
    map.setFilter("place-selected", filter);
  }, [selected]);

  const filterCount = categories.length + statuses.length;

  return (
    <div className="map-wrap">
      <div ref={containerRef} className="map" />
      <div className="map-overlay">
        <div className="map-toolbar">
          <div className="map-search-wrap">
            <label className="map-search">
              <SearchIcon />
              {scope && (
                <span className="search-token">
                  {scope.code && <Flag code={scope.code} name={scope.label} />} {scope.label}
                  <button aria-label={`Show all places, not only ${scope.label}`} onMouseDown={(e) => e.preventDefault()}
                          onClick={() => enterScope(scope.kind === "city" && scope.parent ? scope.parent : undefined)}>✕</button>
                </span>
              )}
              <input placeholder={scope ? `Search in ${scope.label}` : "Search your saved places"} value={search}
                     onChange={(e) => { setSearch(e.target.value); setSuggestOpen(true); setActive(0); }}
                     onFocus={() => setSuggestOpen(true)}
                     onBlur={() => setTimeout(() => setSuggestOpen(false), 120)}
                     onKeyDown={(e) => {
                       if (e.key === "Backspace" && !search && scope) { enterScope(scope.kind === "city" && scope.parent ? scope.parent : undefined); return; }
                       if (!suggestions?.length) return;
                       if (e.key === "ArrowDown") { e.preventDefault(); setActive((i) => Math.min(i + 1, suggestions.length - 1)); }
                       else if (e.key === "ArrowUp") { e.preventDefault(); setActive((i) => Math.max(i - 1, 0)); }
                       else if (e.key === "Enter") { e.preventDefault(); pick(suggestions[active]); }
                       else if (e.key === "Escape") setSuggestOpen(false);
                     }} />
              {search && <button className="map-search-clear" aria-label="Clear search" onMouseDown={(e) => e.preventDefault()} onClick={() => setSearch("")}>✕</button>}
            </label>
            {suggestOpen && suggestions && (
              <div className="search-suggest" role="listbox">
                {!search.trim() && suggestions.length > 0 && (
                  <div className="search-suggest-title">{scope?.kind === "country" ? `Cities and places in ${scope.label}` : scope ? `Places in ${scope.label}` : "Your countries"}</div>
                )}
                {suggestions.length === 0 && search.trim() && <div className="search-suggest-empty">No saved places {scope ? `in ${scope.label} ` : ""}match “{search.trim()}”</div>}
                {suggestions.map((s, i) => (
                  <button key={`${s.kind}-${s.key}`} role="option" aria-selected={i === active}
                          className={`suggest-row ${i === active ? "active" : ""}`}
                          onMouseDown={(e) => e.preventDefault()} onMouseEnter={() => setActive(i)} onClick={() => pick(s)}>
                    {s.kind === "place" ? (
                      <CategoryBadge category={s.place.category} size={30} />
                    ) : s.kind === "country" ? <span className="result-icon"><Flag code={s.key} name={s.label} /></span>
                      : <CategoryBadge category="city" size={30} />}
                    <span className="suggest-text">
                      <span className="suggest-name"><Highlight text={s.label} query={search} /></span>
                      <span className="suggest-sub">{s.sub}</span>
                    </span>
                    <span className="suggest-kind">{s.kind === "place" ? "" : s.kind === "city" ? "City" : "Country"}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className="map-filter-wrap">
            <button className={`btn small ${filterCount ? "is-on" : ""}`} onClick={() => setFiltersOpen((o) => !o)}>
              Filters{filterCount ? ` · ${filterCount}` : ""}
            </button>
            {filtersOpen && (
              <FiltersPopover places={scoped} categories={categories} statuses={statuses}
                              setCategories={setCategories} setStatuses={setStatuses} onClose={() => setFiltersOpen(false)} />
            )}
          </div>
          <button className={`btn small ${areaOpen ? "is-on" : ""}`} onClick={() => setAreaOpen((o) => !o)} title="Saved places in the visible map area">
            ☰ {inView.length === filtered.length ? `${filtered.length} places` : `${inView.length} of ${filtered.length} in view`}
          </button>
          <button className="btn primary small" onClick={() => setAdding(true)}>＋ Add place</button>
        </div>

        {areaOpen && <AreaPanel places={inView} selectedId={selected?.id} map={mapRef.current}
          onPick={(p) => { setSelected(p); mapRef.current?.easeTo({ center: [p.longitude, p.latitude], zoom: Math.max(mapRef.current.getZoom(), 12) }); }}
          onClose={() => setAreaOpen(false)} onFitAll={() => enterScope(undefined)}
          onCountry={(code, label) => enterScope({ kind: "country", key: code, label, code })}
          onCity={(key) => enterScope({ kind: "city", key, label: key, code: scope?.code ?? null, parent: scope?.kind === "country" ? scope : scope?.parent })} />}
      </div>

      <div className="map-style-wrap">
        <div className="map-view-switch" role="tablist" aria-label="Map view">
          {VIEWS.map((v) => (
            <button key={v.key} role="tab" aria-selected={view === v.key} className={view === v.key ? "active" : ""} title={v.hint}
                    onClick={() => {
                      setView(v.key);
                      if (v.key === "countries") { if (scope) enterScope(undefined); }        // back out to the world
                      else if (scope?.kind === "city" && v.key !== "smart") enterScope(scope.parent); // a city already shows its pins
                    }}>
              {v.label}
            </button>
          ))}
        </div>
        <button className={`map-style-btn ${styleMenu ? "is-on" : ""}`} onClick={() => setStyleMenu((o) => !o)} aria-haspopup="menu" aria-expanded={styleMenu} title="Map style">
          <LayersIcon /> {MAP_STYLES.find((s) => s.id === styleId)?.label}
        </button>
        {styleMenu && (
          <MapStyleMenu value={styleId} onClose={() => setStyleMenu(false)} onPick={(id) => { setStyleId(id); setStyleMenu(false); }} />
        )}
      </div>

      {/* Only for a genuinely empty library — not while places are still loading. */}
      {loaded && loaded.length === 0 && <Onboarding />}

      {selected && <MapPreview place={selected} onClose={() => setSelected(undefined)} onOpen={() => nav.openPlace(selected.id)} />}
      {adding && <QuickAddPlace onClose={() => setAdding(false)} />}
    </div>
  );
}

function LayersIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
      <path d="M8 1.8 1.6 5.2 8 8.6l6.4-3.4L8 1.8Z" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
      <path d="M1.6 8.2 8 11.6l6.4-3.4M1.6 11 8 14.4l6.4-3.4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinejoin="round" />
    </svg>
  );
}

/** Map style menu (anchored to its button, top-right). */
function MapStyleMenu({ value, onPick, onClose }: { value: MapStyleId; onPick: (id: MapStyleId) => void; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const away = (e: MouseEvent) => { if (ref.current && !ref.current.parentElement?.contains(e.target as Node)) onClose(); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", away); document.removeEventListener("keydown", esc); };
  }, [onClose]);
  return (
    <div className="map-style-menu" role="menu" ref={ref}>
      {MAP_STYLES.map((s) => (
        <button key={s.id} role="menuitemradio" aria-checked={value === s.id} className={`map-style-item ${value === s.id ? "active" : ""}`} onClick={() => onPick(s.id)}>
          <span className={`style-swatch swatch-${s.id}`} />
          <span className="grow">
            <span className="map-style-name">{s.label}</span>
            <span className="map-style-hint">{s.hint}</span>
          </span>
          <span className="map-style-check">{value === s.id ? "✓" : ""}</span>
        </button>
      ))}
    </div>
  );
}

/** "Fit all places" button, grouped with the zoom buttons. */
class FitAllControl implements maplibregl.IControl {
  private el?: HTMLDivElement;
  constructor(private onFit: () => void) {}
  onAdd() {
    this.el = document.createElement("div");
    this.el.className = "maplibregl-ctrl maplibregl-ctrl-group";
    const b = document.createElement("button");
    b.type = "button";
    b.title = "Fit all places";
    b.setAttribute("aria-label", "Fit all places");
    b.className = "fit-all-btn";
    b.innerHTML = '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
    b.onclick = this.onFit;
    this.el.appendChild(b);
    return this.el;
  }
  onRemove() { this.el?.remove(); }
}

/** Status and kind of place, as toggles. Applies instantly. */
function FiltersPopover({ places, categories, statuses, setCategories, setStatuses, onClose }: {
  places: Place[]; categories: CategoryGroup[]; statuses: PersonalStatus[];
  setCategories: (c: CategoryGroup[]) => void; setStatuses: (s: PersonalStatus[]) => void; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const away = (e: MouseEvent) => { if (ref.current && !ref.current.parentElement?.contains(e.target as Node)) onClose(); };
    const esc = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", away); document.removeEventListener("keydown", esc); };
  }, [onClose]);
  const count = <T extends string>(key: (p: Place) => T) => {
    const m = new Map<T, number>();
    places.forEach((p) => m.set(key(p), (m.get(key(p)) ?? 0) + 1));
    return m;
  };
  const byCat = count((p) => groupOf(p.category));
  const byStatus = count((p) => p.personalStatus);
  const toggle = <T,>(list: T[], v: T) => (list.includes(v) ? list.filter((x) => x !== v) : [...list, v]);
  return (
    <div className="filters-popover" ref={ref}>
      <div className="filters-section">
        <h5>Status</h5>
        <div className="filters-chips">
          {(Object.keys(STATUS) as PersonalStatus[]).filter((s) => byStatus.has(s)).map((s) => (
            <button key={s} className={`chip-btn ${statuses.includes(s) ? "active" : ""}`} onClick={() => setStatuses(toggle(statuses, s))}>
              {STATUS[s].emoji} {STATUS[s].label} <span className="chip-count">{byStatus.get(s)}</span>
            </button>
          ))}
        </div>
      </div>
      <div className="filters-section">
        <h5>Kind of place</h5>
        <div className="filters-chips">
          {[...byCat.entries()].sort((a, b) => b[1] - a[1]).map(([c, n]) => (
            <button key={c} className={`chip-btn ${categories.includes(c) ? "active" : ""}`} onClick={() => setCategories(toggle(categories, c))}>
              <CategoryBadge category={c} size={18} /> {GROUPS[c].label} <span className="chip-count">{n}</span>
            </button>
          ))}
        </div>
      </div>
      <div className="filters-foot">
        <button className="btn ghost small" disabled={categories.length + statuses.length === 0} onClick={() => { setCategories([]); setStatuses([]); }}>Clear</button>
        <button className="btn small" onClick={onClose}>Done</button>
      </div>
    </div>
  );
}

/**
 * Saved places in the visible map area. Grouped by country when several are in view, by city once you've
 * zoomed into one country — so the list follows the map.
 */
function AreaPanel({ places, selectedId, map, onPick, onClose, onFitAll, onCountry, onCity }: {
  places: Place[]; selectedId?: string; map?: MLMap; onPick: (p: Place) => void; onClose: () => void; onFitAll: () => void;
  onCountry?: (code: string, name: string) => void;
  onCity?: (key: string) => void;
}) {
  const countries = new Set(places.map((p) => p.countryCode ?? "?"));
  const byCountry = countries.size > 1;
  const groups = useMemo(() => {
    const g = new Map<string, { label: string; code: string | null; items: Place[] }>();
    for (const p of places) {
      const key = byCountry ? p.countryCode ?? "?" : areaKey(p);
      const label = byCountry ? p.country ?? "Unknown country" : key;
      const e = g.get(key) ?? { label, code: byCountry ? p.countryCode : null, items: [] };
      e.items.push(p);
      g.set(key, e);
    }
    return [...g.values()].sort((a, b) => b.items.length - a.items.length || a.label.localeCompare(b.label));
  }, [places, byCountry]);

  return (
    <div className="area-panel">
      <div className="area-head">
        <div className="grow">
          <div className="strong">{places.length} saved place{places.length === 1 ? "" : "s"} in view</div>
          <div className="muted small">{byCountry ? `${groups.length} countries — pick one to zoom in` : "Grouped by city"}</div>
        </div>
        <button className="close-btn small" title="Fit all places" onClick={onFitAll}>⌖</button>
        <button className="close-btn small" aria-label="Close" onClick={onClose}>✕</button>
      </div>
      <div className="area-list">
        {places.length === 0 && <p className="muted small area-empty">No saved places here. Zoom out or press ⌖ to see them all.</p>}
        {groups.map((g) => byCountry ? (
          <button key={g.label} className="area-group-row" onClick={() => (g.code && onCountry ? onCountry(g.code, g.label) : map && fit(map, g.items, "compact"))}>
            <Flag code={g.code} name={g.label} />
            <span className="grow">{g.label}</span>
            <span className="area-count">{g.items.length}</span>
            <span className="area-chevron">›</span>
          </button>
        ) : (
          <section key={g.label}>
            <button className="area-city" onClick={() => (onCity ? onCity(g.label) : map && fit(map, g.items, CITY_ZOOM))}>{g.label} <span>{g.items.length}</span></button>
            {g.items.map((p) => {
              const cat = CATEGORY[p.category] ?? CATEGORY.other;
              return (
                <button key={p.id} className={`area-place ${selectedId === p.id ? "active" : ""}`} onClick={() => onPick(p)}>
                  <CategoryBadge category={p.category} size={24} />
                  <span className="grow area-place-text">
                    <span className="area-place-name">{p.canonicalName}</span>
                    <span className="area-place-sub">{cat.label} · {p.sourceCount} source{p.sourceCount === 1 ? "" : "s"}{p.memoryCount ? ` · 📷 ${p.memoryCount}` : ""}</span>
                  </span>
                  <span>{STATUS[p.personalStatus].emoji}</span>
                </button>
              );
            })}
          </section>
        ))}
      </div>
    </div>
  );
}

/** The card shown when you tap a pin: photo with the name over it, where, what kind, and your status. */
function MapPreview({ place, onClose, onOpen }: { place: Place; onClose: () => void; onOpen: () => void }) {
  const cat = CATEGORY[place.category] ?? CATEGORY.other;
  const action = useAction();
  const hero = place.heroImagePath ?? place.thumbnailPath;
  return (
    <div className="map-preview" key={place.id}>
      <div className="map-preview-hero">
        <Thumb path={hero} fallback={<CategoryBadge category={place.category} size={64} />} />
        <div className="map-preview-shade" />
        <button className="map-preview-close" onClick={onClose} aria-label="Close">✕</button>
        <div className="map-preview-title">
          <span className="map-preview-kind" style={{ background: colorOf(place.category) }}><CategoryGlyph category={place.category} size={12} strokeWidth={2.4} /> {cat.label}</span>
          <h3>{place.canonicalName}</h3>
        </div>
      </div>
      <div className="map-preview-body">
        <div className="map-preview-where">
          <Flag code={place.countryCode} name={place.country} />
          <span>{[place.city !== place.canonicalName ? place.city : null, place.country].filter(Boolean).join(", ") || "Unknown location"}</span>
        </div>
        {place.summaryText && <p className="map-preview-summary">{place.summaryText}</p>}
        <div className="map-preview-meta">
          <select className={`status-pill status-${place.personalStatus}`} value={place.personalStatus} title="Change status"
                  disabled={action.busy} onChange={(e) => action.run(() => PlaceService.update(place.id, "personalStatus", e.target.value))}>
            {Object.entries(STATUS).map(([k, s]) => <option key={k} value={k}>{s.emoji} {s.label}</option>)}
          </select>
          <span className="meta-pill">🗂 {place.sourceCount} source{place.sourceCount === 1 ? "" : "s"}</span>
          {place.memoryCount > 0 && <span className="meta-pill">📷 {place.memoryCount}</span>}
        </div>
        <div className="map-preview-actions">
          <button className="btn primary grow" onClick={onOpen}>Open details</button>
          <button className="btn" title="Open in Apple Maps"
                  onClick={() => openUrl(`https://maps.apple.com/?ll=${place.latitude},${place.longitude}&q=${encodeURIComponent(place.canonicalName)}`)}>
            🧭 Maps
          </button>
        </div>
      </div>
    </div>
  );
}

type Scope = { kind: "country" | "city"; key: string; label: string; code: string | null; parent?: Scope; compact?: boolean };

type MapViewKind = "smart" | "countries" | "cities" | "places";

const VIEWS: { key: MapViewKind; label: string; hint: string }[] = [
  { key: "smart", label: "Smart", hint: "Country → city → place as you go" },
  { key: "countries", label: "Countries", hint: "One bubble per country" },
  { key: "cities", label: "Cities", hint: "One bubble per city (nearby cities merge)" },
  { key: "places", label: "Places", hint: "Every place as a pin" },
];

type Range = [number, number] | null;
type Clustering = { cluster: boolean; clusterRadius: number; clusterMaxZoom: number };
/** Which markers show at which zooms. Each zoom shows one kind of marker, never a mix. */
interface Plan { countries: Range; areas: Range; pins: Range; clustering: Clustering }

const CLUSTERED: Clustering = { cluster: true, clusterRadius: 44, clusterMaxZoom: 10 };
const FLAT: Clustering = { cluster: false, clusterRadius: 44, clusterMaxZoom: 10 };

function planFor(view: MapViewKind, scope: Scope | undefined): Plan {
  // A picked city: every pin in it, no bubbles, no clusters — whatever the view.
  if (scope?.kind === "city") return { countries: null, areas: null, pins: [0, 24], clustering: FLAT };
  switch (view) {
    case "countries": return { countries: [0, 24], areas: null, pins: null, clustering: CLUSTERED };
    case "cities": return { countries: null, areas: [0, 24], pins: null, clustering: CLUSTERED };
    case "places": return { countries: null, areas: null, pins: [0, 24], clustering: scope?.compact ? FLAT : CLUSTERED };
    default:
      if (!scope) return { countries: [0, COUNTRY_MAX_ZOOM], areas: null, pins: [COUNTRY_MAX_ZOOM, 24], clustering: CLUSTERED };
      if (scope.compact) return { countries: null, areas: null, pins: [0, 24], clustering: FLAT };
      return { countries: null, areas: [0, AREA_MAX_ZOOM], pins: [AREA_MAX_ZOOM, 24], clustering: FLAT };
  }
}

const LAYER_GROUPS: Record<"countries" | "areas" | "pins", string[]> = {
  countries: ["country-bubbles"],
  areas: ["area-cluster-ring", "area-clusters", "area-cluster-count", "area-bubbles"],
  pins: ["cluster-ring", "clusters", "cluster-count", "place-points", "place-selected", "place-labels"],
};

/** Shows/hides each group of layers and sets the zooms it's visible at. */
function applyPlan(map: MLMap, plan: Plan) {
  (Object.keys(LAYER_GROUPS) as (keyof typeof LAYER_GROUPS)[]).forEach((group) => {
    const range = plan[group];
    for (const id of LAYER_GROUPS[group]) {
      if (!map.getLayer(id)) continue;
      map.setLayoutProperty(id, "visibility", range ? "visible" : "none");
      if (range) map.setLayerZoomRange(id, id === "place-labels" ? Math.max(range[0], 11) : range[0], range[1]);
    }
  });
}

type Suggestion =
  | { kind: "place"; key: string; label: string; sub: string; place: Place }
  | { kind: "city" | "country"; key: string; label: string; sub: string; code: string | null };

/**
 * Suggestions for the search field, from the places in scope only. With nothing typed: your countries, or
 * — once a country is picked — its cities and places. While typing: names starting with the text first,
 * then words starting with it, then anywhere.
 */
function suggest(places: Place[], query: string, scope?: Scope): Suggestion[] {
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
    if (p.city && p.city !== p.canonicalName) cities.set(p.city, { country: p.country, code: p.countryCode, count: (cities.get(p.city)?.count ?? 0) + 1 });
    const r = Math.min(rank(p.canonicalName), ...p.alternativeNames.map(rank));
    if (r < 9) {
      const cat = CATEGORY[p.category] ?? CATEGORY.other;
      scored.push([r, { kind: "place", key: p.id, label: p.canonicalName, sub: [cat.label, p.city !== p.canonicalName ? p.city : null, p.country].filter(Boolean).join(" · "), place: p }]);
    }
  }
  for (const [code, c] of scope ? [] : countries) {
    const r = Math.min(rank(c.name), code.toLowerCase() === q ? 0 : 9);
    if (r < 9) scored.push([r - 0.5, { kind: "country", key: code, label: c.name, sub: `${c.count} saved place${c.count === 1 ? "" : "s"}`, code }]);
  }
  for (const [city, c] of scope?.kind === "city" ? [] : cities) {
    const r = rank(city);
    if (r < 9) scored.push([r - 0.25, { kind: "city", key: city, label: city, sub: [c.country, `${c.count} place${c.count === 1 ? "" : "s"}`].filter(Boolean).join(" · "), code: c.code }]);
  }
  return scored.sort((a, b) => a[0] - b[0] || a[1].label.localeCompare(b[1].label)).slice(0, 8).map(([, s]) => s);
}

/** Nothing typed yet: offer what's available in scope, biggest first. */
function browse(places: Place[], scope?: Scope): Suggestion[] {
  const count = <K,>(key: (p: Place) => K | null | undefined) => {
    const m = new Map<K, Place[]>();
    places.forEach((p) => { const k = key(p); if (k != null) m.set(k, [...(m.get(k) ?? []), p]); });
    return [...m.entries()].sort((a, b) => b[1].length - a[1].length);
  };
  const placeRow = (p: Place): Suggestion => {
    const cat = CATEGORY[p.category] ?? CATEGORY.other;
    return { kind: "place", key: p.id, label: p.canonicalName, sub: [cat.label, p.city !== p.canonicalName ? p.city : null].filter(Boolean).join(" · "), place: p };
  };
  const n = (k: number) => `${k} saved place${k === 1 ? "" : "s"}`;
  if (!scope) {
    return count((p) => p.countryCode).slice(0, 10).map(([code, ps]) => ({ kind: "country", key: code, label: ps[0].country ?? code, sub: n(ps.length), code }));
  }
  if (scope.kind === "country") {
    const cities: Suggestion[] = count((p) => (p.city && p.city !== p.canonicalName ? p.city : null)).slice(0, 6)
      .map(([city, ps]) => ({ kind: "city", key: city, label: city, sub: n(ps.length), code: scope.code }));
    const shown = new Set(cities.map((c) => c.key));
    const places2 = places.filter((p) => !p.city || !shown.has(p.city)).sort((a, b) => b.sourceCount - a.sourceCount).slice(0, 10 - cities.length).map(placeRow);
    return [...cities, ...places2];
  }
  return [...places].sort((a, b) => b.sourceCount - a.sourceCount).slice(0, 10).map(placeRow);
}

/** Bold the typed part of a suggestion. */
function Highlight({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  const i = q ? text.toLowerCase().indexOf(q.toLowerCase()) : -1;
  if (i < 0) return <>{text}</>;
  return <>{text.slice(0, i)}<mark>{text.slice(i, i + q.length)}</mark>{text.slice(i + q.length)}</>;
}

function countsFor(places: Place[]): Record<string, number> {
  const c: Record<string, number> = {};
  places.forEach((p) => { const g = groupOf(p.category); c[g] = (c[g] ?? 0) + 1; });
  return c;
}

/** Hover card: a title, a short line, and the kinds of place inside as coloured chips. */
function hoverCard(title: string, subtitle: string, counts: Record<string, number>): string {
  const chips = Object.entries(counts).filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1]).slice(0, 4)
    .map(([g, n]) => `<span class="hc-chip"><i style="background:${colorOf(g)}"></i>${n} ${escapeHtml(GROUPS[g as CategoryGroup].label)}</span>`).join("");
  return `<div class="hc"><div class="hc-title">${escapeHtml(title)}</div><div class="hc-sub">${escapeHtml(subtitle)}</div>${chips ? `<div class="hc-chips">${chips}</div>` : ""}</div>`;
}

/** Would these places fit on screen at a "small country" zoom? */
function isCompact(map: MLMap | undefined, places: Place[]): boolean {
  if (!map || places.length < 2) return places.length > 0;
  const bounds = new maplibregl.LngLatBounds();
  places.forEach((p) => bounds.extend([p.longitude, p.latitude]));
  return (map.cameraForBounds(bounds, { padding: 80 })?.zoom ?? 0) >= COMPACT_ZOOM;
}

function escapeHtml(s: string): string {
  return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
}

/** A small list for pins that sit on (almost) the same spot. */
function showChooser(map: MLMap, at: maplibregl.LngLat, places: Place[], onPick: (p: Place) => void) {
  const list = document.createElement("div");
  list.className = "pin-chooser";
  const title = document.createElement("div");
  title.className = "pin-chooser-title";
  title.textContent = `${places.length} places here`;
  list.appendChild(title);
  const popup = new maplibregl.Popup({ closeButton: false, offset: 16, className: "map-chooser", maxWidth: "280px" });
  places.slice(0, 12).forEach((p) => {
    const b = document.createElement("button");
    b.className = "pin-chooser-row";
    const dot = document.createElement("span");
    dot.className = "pin-chooser-dot";
    dot.style.background = colorOf(p.category);
    const name = document.createElement("span");
    name.textContent = p.canonicalName;
    b.append(dot, name);
    b.onclick = () => { popup.remove(); onPick(p); };
    list.appendChild(b);
  });
  popup.setLngLat(at).setDOMContent(list).addTo(map);
}

function SearchIcon() {
  return (
    <svg className="map-search-icon" width="15" height="15" viewBox="0 0 16 16" aria-hidden="true">
      <circle cx="7" cy="7" r="5.25" fill="none" stroke="currentColor" strokeWidth="1.8" />
      <path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    </svg>
  );
}

function toGeoJSON(places: Place[]): GeoJSON.FeatureCollection {
  return {
    type: "FeatureCollection",
    features: places.map((p) => ({
      type: "Feature",
      geometry: { type: "Point", coordinates: [p.longitude, p.latitude] },
      properties: { id: p.id, name: p.canonicalName, category: groupOf(p.category), status: p.personalStatus },
    })),
  };
}

/** World view: zoom 0–3 country bubbles · 3.5–10 clusters (see CLUSTERING) · 10+ individual places. */
const COUNTRY_MAX_ZOOM = 3.5;
/** Inside a picked country, city/region bubbles give way to individual pins at this zoom. */
const AREA_MAX_ZOOM = 11;
/** Picking a city zooms to about street/neighbourhood level. */
const CITY_ZOOM = 11;

/** A picked country whose places fit at ≥ this zoom (e.g. Sri Lanka) shows every place — no clusters. */
const COMPACT_ZOOM = 5;

/** Fits the map to places. `minZoom` = zoom at least this close; "compact" = past clustering if the places are close together. */
function fit(map: MLMap, places: Place[], minZoom?: number | "compact") {
  if (places.length === 0) return;
  if (places.length === 1) {
    map.easeTo({ center: [places[0].longitude, places[0].latitude], zoom: Math.max(12, typeof minZoom === "number" ? minZoom : 0) });
    return;
  }
  const bounds = new maplibregl.LngLatBounds();
  places.forEach((p) => bounds.extend([p.longitude, p.latitude]));
  const camera = map.cameraForBounds(bounds, { padding: 80, maxZoom: 14 });
  if (!camera) return;
  const zoom = camera.zoom ?? 0;
  // A city: zoom to ~11–12, unless that would cut off some of its places.
  const floor = minZoom === "compact" ? 0 : minZoom !== undefined && zoom >= minZoom - 2 ? minZoom : 0;
  map.easeTo({ ...camera, zoom: Math.min(Math.max(zoom, floor), 12.5), duration: 600 });
}

/** First-run guidance: permission → AI key → scan. */
function Onboarding() {
  const nav = useNav();
  const { data: permission, reload } = useLoad(() => PhotoLibraryService.permissionStatus(), []);
  const { data: overview } = useLoad(() => AppService.overview(), []);
  const granted = permission === "authorized" || permission === "limited";
  const scanning = overview?.queue.running;

  return (
    <div className="onboarding card">
      <h3>Turn your screenshots into a travel map</h3>
      <p className="muted small">Text is read on your Mac with Apple Vision. Only likely-travel text is sent to DeepSeek.</p>
      <div className={`step ${granted ? "done" : ""}`}>
        <span className="step-num">{granted ? "✓" : 1}</span>
        <div className="grow">
          <div className="strong">Allow access to Photos</div>
          <div className="muted small">Only screenshots are read. Status: {permission ?? "checking…"}</div>
        </div>
        {!granted && <button className="btn" onClick={async () => { await PhotoLibraryService.requestPermission(); reload(); }}>Allow</button>}
      </div>
      <div className={`step ${overview?.aiConfigured ? "done" : ""}`}>
        <span className="step-num">{overview?.aiConfigured ? "✓" : 2}</span>
        <div className="grow">
          <div className="strong">DeepSeek API key</div>
          <div className="muted small">{overview?.aiConfigured ? `Found (${overview.apiKeySource})` : "Add DEEPSEEK_API_KEY to .env.local or Settings"}</div>
        </div>
        {!overview?.aiConfigured && <button className="btn" onClick={() => nav.go("settings")}>Settings</button>}
      </div>
      <div className="step">
        <span className="step-num">3</span>
        <div className="grow">
          <div className="strong">Scan screenshots</div>
          <div className="muted small">Only screenshots you haven't processed yet. Runs in the background; pause any time. Tip: try a small test run first (Settings → Diagnostics).</div>
          <div className="row wrap" style={{ marginTop: 8 }}>{!scanning && <ScanControls />}{scanning && <span className="muted">Scanning…</span>}</div>
        </div>
      </div>
    </div>
  );
}
