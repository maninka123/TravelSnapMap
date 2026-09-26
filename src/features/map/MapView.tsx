import maplibregl, { type GeoJSONSource, type Map as MLMap } from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import { AppService, PhotoLibraryService } from "../../api/services";
import { ScanControls } from "../../components/ScanControls";
import { PlaceService } from "../../api/services";
import type { Place, PlaceCategory, PersonalStatus } from "../../api/types";
import { Flag, Thumb } from "../../components/common";
import { MatchSnippet, SEARCH_EXAMPLES, SearchChips, useSmartSearch } from "../../components/SmartSearch";
import { CATEGORY, STATUS } from "../../lib/labels";
import { openUrl } from "../../lib/open";
import { QuickAddPlace } from "../places/QuickAddPlace";
import { useLoad, useNav } from "../../lib/nav";

/** Free vector basemap (no API key). Swap the style URL to change providers. */
export const MAP_STYLE = "https://tiles.openfreemap.org/styles/liberty";

export function MapView() {
  const nav = useNav();
  // The map shows verified places only; provisional ones wait in the Review inbox.
  const { data: places = [] } = useLoad(() => PlaceService.list({ verifiedOnly: true }), []);
  const [category, setCategory] = useState<PlaceCategory | "">("");
  const [status, setStatus] = useState<PersonalStatus | "">("");
  const [country, setCountry] = useState("");
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Place>();
  const [adding, setAdding] = useState(false);
  const [showResults, setShowResults] = useState(true);
  const { result: smart, busy: searching } = useSmartSearch(search, true);
  const hitIds = useMemo(() => (smart ? new Set(smart.hits.map((h) => h.place.id)) : undefined), [smart]);

  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MLMap | undefined>(undefined);
  const placesRef = useRef<Place[]>([]);

  const filtered = useMemo(() => places.filter((p) =>
    (!category || p.category === category) &&
    (!status || p.personalStatus === status) &&
    (!country || p.countryCode === country) &&
    (!hitIds || hitIds.has(p.id))), [places, category, status, country, hitIds]);

  // Zoom to the results of each new search.
  useEffect(() => {
    const map = mapRef.current;
    if (map && smart && smart.hits.length > 0) fit(map, smart.hits.map((h) => h.place));
  }, [smart]);

  const countries = useMemo(() => {
    const counts = new Map<string, { code: string; name: string; count: number }>();
    for (const p of places) {
      if (!p.countryCode) continue;
      const c = counts.get(p.countryCode) ?? { code: p.countryCode, name: p.country ?? p.countryCode, count: 0 };
      c.count++;
      counts.set(p.countryCode, c);
    }
    return [...counts.values()].sort((a, b) => b.count - a.count);
  }, [places]);

  // Create the map once.
  useEffect(() => {
    if (!containerRef.current) return;
    const map = new maplibregl.Map({ container: containerRef.current, style: MAP_STYLE, center: [20, 25], zoom: 1.4, attributionControl: { compact: true } });
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "bottom-left");
    mapRef.current = map;

    map.on("load", () => {
      map.addSource("places", { type: "geojson", data: toGeoJSON(placesRef.current), cluster: true, clusterRadius: 48, clusterMaxZoom: 13 });
      map.addLayer({
        id: "clusters", type: "circle", source: "places", filter: ["has", "point_count"],
        paint: {
          "circle-color": "#0f8a7e", "circle-opacity": 0.85, "circle-stroke-width": 3, "circle-stroke-color": "#ffffff",
          "circle-radius": ["step", ["get", "point_count"], 16, 10, 22, 50, 30],
        },
      });
      map.addLayer({
        id: "cluster-count", type: "symbol", source: "places", filter: ["has", "point_count"],
        layout: { "text-field": ["get", "point_count_abbreviated"], "text-size": 13, "text-font": ["Noto Sans Bold"] },
        paint: { "text-color": "#ffffff" },
      });
      map.addLayer({
        id: "place-points", type: "circle", source: "places", filter: ["!", ["has", "point_count"]],
        paint: {
          "circle-color": ["get", "color"], "circle-radius": 8, "circle-stroke-width": 2.5, "circle-stroke-color": "#ffffff",
          "circle-opacity": ["case", ["==", ["get", "status"], "notInterested"], 0.35, 1],
        },
      });
      map.addLayer({
        id: "place-labels", type: "symbol", source: "places", filter: ["!", ["has", "point_count"]], minzoom: 9,
        layout: { "text-field": ["get", "name"], "text-size": 12, "text-offset": [0, 1.3], "text-anchor": "top", "text-font": ["Noto Sans Regular"] },
        paint: { "text-color": "#1c2128", "text-halo-color": "#ffffff", "text-halo-width": 1.5 },
      });

      map.on("click", "clusters", async (e) => {
        const feature = map.queryRenderedFeatures(e.point, { layers: ["clusters"] })[0];
        const source = map.getSource("places") as GeoJSONSource;
        const zoom = await source.getClusterExpansionZoom(feature.properties.cluster_id);
        map.easeTo({ center: (feature.geometry as GeoJSON.Point).coordinates as [number, number], zoom });
      });
      map.on("click", "place-points", (e) => {
        const id = e.features?.[0]?.properties?.id;
        setSelected(placesRef.current.find((p) => p.id === id));
      });
      for (const layer of ["clusters", "place-points"]) {
        map.on("mouseenter", layer, () => { map.getCanvas().style.cursor = "pointer"; });
        map.on("mouseleave", layer, () => { map.getCanvas().style.cursor = ""; });
      }
      fit(map, placesRef.current);
    });
    return () => map.remove();
  }, []);

  // Push filtered data into the clustered source.
  useEffect(() => {
    placesRef.current = filtered;
    const source = mapRef.current?.getSource("places") as GeoJSONSource | undefined;
    source?.setData(toGeoJSON(filtered));
  }, [filtered]);

  const focusCountry = (code: string) => {
    const next = country === code ? "" : code;
    setCountry(next);
    const map = mapRef.current;
    if (map) fit(map, places.filter((p) => !next || p.countryCode === next));
  };

  return (
    <div className="map-wrap">
      <div ref={containerRef} className="map" />
      <div className="map-overlay">
        <div className="map-filters">
          <div className="smart-search-field">
            <span className="smart-search-icon">{searching ? "…" : "🔎"}</span>
            <input placeholder="Ask your map — e.g. “sunrise spots in Japan”" value={search}
                   onChange={(e) => { setSearch(e.target.value); setShowResults(true); }} onFocus={() => setShowResults(true)} />
            {search && <button className="icon-btn small" aria-label="Clear search" onClick={() => setSearch("")}>✕</button>}
          </div>
          <select value={category} onChange={(e) => setCategory(e.target.value as PlaceCategory | "")}>
            <option value="">All categories</option>
            {Object.entries(CATEGORY).map(([k, c]) => <option key={k} value={k}>{c.emoji} {c.label}</option>)}
          </select>
          <select value={status} onChange={(e) => setStatus(e.target.value as PersonalStatus | "")}>
            <option value="">Any status</option>
            {Object.entries(STATUS).map(([k, s]) => <option key={k} value={k}>{s.emoji} {s.label}</option>)}
          </select>
          <span className="muted small" style={{ alignSelf: "center" }}>{filtered.length} places</span>
          <button className="btn small" onClick={() => setAdding(true)}>＋ Add place</button>
        </div>
        {!search && places.length > 0 && (
          <div className="search-examples">
            {SEARCH_EXAMPLES.map((ex) => <button key={ex} className="chip-btn" onClick={() => setSearch(ex)}>{ex}</button>)}
          </div>
        )}
        {smart && showResults && (
          <div className="smart-results material">
            <div className="row">
              <SearchChips chips={smart.chips} count={smart.hits.length} />
              <span className="grow" />
              <button className="icon-btn small" title="Hide list" onClick={() => setShowResults(false)}>▾</button>
            </div>
            <div className="smart-results-list">
              {smart.hits.slice(0, 40).map((h) => (
                <button key={h.place.id} className="smart-result" onClick={() => {
                  setSelected(h.place);
                  mapRef.current?.easeTo({ center: [h.place.longitude, h.place.latitude], zoom: Math.max(mapRef.current.getZoom(), 12) });
                }}>
                  <Thumb path={h.place.heroImagePath ?? h.place.thumbnailPath} fallback={CATEGORY[h.place.category]?.emoji} />
                  <div className="grow">
                    <div className="strong">{h.place.canonicalName}</div>
                    <div className="muted small"><Flag code={h.place.countryCode} name={h.place.country} /> {[h.place.city, h.place.country].filter(Boolean).join(", ")}</div>
                    {h.matches[0] && <MatchSnippet fact={h.matches[0]} />}
                  </div>
                </button>
              ))}
              {smart.hits.length === 0 && <p className="muted small">Nothing saved matches that yet. Try fewer words, or a country or kind of place.</p>}
            </div>
          </div>
        )}
        {countries.length > 0 && (
          <div className="country-chips">
            {countries.slice(0, 14).map((c) => (
              <button key={c.code} className={`chip-btn ${country === c.code ? "active" : ""}`} onClick={() => focusCountry(c.code)} title={c.name}>
                <Flag code={c.code} name={c.name} /> {c.count}
              </button>
            ))}
          </div>
        )}
      </div>

      {places.length === 0 && <Onboarding />}

      {selected && <MapPreview place={selected} onClose={() => setSelected(undefined)} onOpen={() => nav.openPlace(selected.id)} />}
      {adding && <QuickAddPlace onClose={() => setAdding(false)} />}
    </div>
  );
}

/** The card shown when you tap a pin: photo with the name over it, where, what kind, and your status. */
function MapPreview({ place, onClose, onOpen }: { place: Place; onClose: () => void; onOpen: () => void }) {
  const cat = CATEGORY[place.category] ?? CATEGORY.other;
  const status = STATUS[place.personalStatus];
  const hero = place.heroImagePath ?? place.thumbnailPath;
  return (
    <div className="map-preview" key={place.id}>
      <div className="map-preview-hero">
        <Thumb path={hero} fallback={<span className="map-preview-fallback">{cat.emoji}</span>} />
        <div className="map-preview-shade" />
        <button className="map-preview-close" onClick={onClose} aria-label="Close">✕</button>
        <div className="map-preview-title">
          <span className="map-preview-kind" style={{ background: cat.color }}>{cat.emoji} {cat.label}</span>
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
          <span className={`status-pill status-${place.personalStatus}`}>{status.emoji} {status.label}</span>
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

function toGeoJSON(places: Place[]): GeoJSON.FeatureCollection {
  return {
    type: "FeatureCollection",
    features: places.map((p) => ({
      type: "Feature",
      geometry: { type: "Point", coordinates: [p.longitude, p.latitude] },
      properties: { id: p.id, name: p.canonicalName, color: (CATEGORY[p.category] ?? CATEGORY.other).color, status: p.personalStatus },
    })),
  };
}

function fit(map: MLMap, places: Place[]) {
  if (places.length === 0) return;
  if (places.length === 1) {
    map.easeTo({ center: [places[0].longitude, places[0].latitude], zoom: 12 });
    return;
  }
  const bounds = new maplibregl.LngLatBounds();
  places.forEach((p) => bounds.extend([p.longitude, p.latitude]));
  map.fitBounds(bounds, { padding: 80, maxZoom: 13, duration: 600 });
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
          <div className="muted small">Only screenshots you haven't processed yet. Runs in the background; pause any time. Tip: try a small test run from Sources first.</div>
          <div className="row wrap" style={{ marginTop: 8 }}>{!scanning && <ScanControls compact />}{scanning && <span className="muted">Scanning…</span>}</div>
        </div>
      </div>
    </div>
  );
}
