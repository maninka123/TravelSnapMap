import maplibregl, { type GeoJSONSource, type Map as MLMap } from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import { AppService, PhotoLibraryService, ProcessingService } from "../../api/services";
import { PlaceService } from "../../api/services";
import type { Place, PlaceCategory, PersonalStatus } from "../../api/types";
import { CategoryChip, PlaceLine, StatusBadge, Thumb } from "../../components/common";
import { CATEGORY, flag, STATUS } from "../../lib/labels";
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

  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MLMap | undefined>(undefined);
  const placesRef = useRef<Place[]>([]);

  const filtered = useMemo(() => {
    const q = search.toLowerCase().trim();
    return places.filter((p) =>
      (!category || p.category === category) &&
      (!status || p.personalStatus === status) &&
      (!country || p.countryCode === country) &&
      (!q || [p.canonicalName, ...p.alternativeNames, p.city, p.country].join(" ").toLowerCase().includes(q)));
  }, [places, category, status, country, search]);

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
          <input placeholder="Search places…" value={search} onChange={(e) => setSearch(e.target.value)} style={{ width: 200 }} />
          <select value={category} onChange={(e) => setCategory(e.target.value as PlaceCategory | "")}>
            <option value="">All categories</option>
            {Object.entries(CATEGORY).map(([k, c]) => <option key={k} value={k}>{c.emoji} {c.label}</option>)}
          </select>
          <select value={status} onChange={(e) => setStatus(e.target.value as PersonalStatus | "")}>
            <option value="">Any status</option>
            {Object.entries(STATUS).map(([k, s]) => <option key={k} value={k}>{s.emoji} {s.label}</option>)}
          </select>
          <span className="muted small" style={{ alignSelf: "center" }}>{filtered.length} places</span>
        </div>
        {countries.length > 0 && (
          <div className="country-chips">
            {countries.slice(0, 14).map((c) => (
              <button key={c.code} className={`chip-btn ${country === c.code ? "active" : ""}`} onClick={() => focusCountry(c.code)} title={c.name}>
                {flag(c.code)} {c.count}
              </button>
            ))}
          </div>
        )}
      </div>

      {places.length === 0 && <Onboarding />}

      {selected && (
        <div className="map-preview">
          <Thumb path={selected.heroImagePath ?? selected.thumbnailPath} />
          <div className="map-preview-body">
            <div className="row">
              <h3 className="grow">{selected.canonicalName}</h3>
              <StatusBadge place={selected} />
              <button className="icon-btn" onClick={() => setSelected(undefined)}>✕</button>
            </div>
            <PlaceLine place={selected} />
            <div className="row wrap">
              <CategoryChip category={selected.category} />
              <span className="muted small">{selected.sourceCount} source{selected.sourceCount === 1 ? "" : "s"}</span>
            </div>
            <button className="btn primary" onClick={() => nav.openPlace(selected.id)}>Open details</button>
          </div>
        </div>
      )}
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
          <div className="muted small">Runs in the background; you can pause any time. Or import an Instagram Reel from Sources.</div>
        </div>
        <button className="btn primary" disabled={!granted || scanning} onClick={() => ProcessingService.start(true)}>
          {scanning ? "Scanning…" : "Start"}
        </button>
      </div>
    </div>
  );
}
