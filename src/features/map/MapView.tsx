import maplibregl, { type GeoJSONSource, type LngLatBounds, type Map as MLMap } from "maplibre-gl";
import { Check, ChevronRight, ExternalLink, Layers, Maximize2, Navigation, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { PlaceService } from "../../api/services";
import type { Place } from "../../api/types";
import { Flag, Thumb } from "../../components/common";
import { Menu, MenuItem } from "../../components/ui";
import { CATEGORY, STATUS } from "../../lib/labels";
import { StatusIcon } from "../../lib/icons";
import { openUrl } from "../../lib/open";
import { useAction } from "../../lib/nav";
import { CategoryBadge, CategoryGlyph, colorOf, GROUPS, groupOf, pinImage, type CategoryGroup } from "../../lib/categoryIcons";
import { areaAggregates, areaGeoJSON, areaKey, PIN_CITY_MAX, rankAreas, support, countryAggregates, countryGeoJSON, ensureAreaImages, ensureCountryImages } from "../../lib/countryBubbles";
import { getMapStyleId, MAP_STYLES, resolveMapStyle, setMapStyleId, type MapStyleId } from "../../lib/mapStyle";
import { inScope, type Scope } from "../explore/model";

/** Base-map labels that compete with your places: road names/shields, POIs, water lines, villages. */
const NOISY_LABELS = /^(highway-name|highway-shield|road_shield|road-shield|waterway|water_name_line|airport|poi|label_other|label_village)/;

/** Hides the busiest base-map labels; call once the style has loaded. */
export function quietBasemap(map: MLMap) {
  for (const layer of map.getStyle().layers ?? []) {
    if (layer.type === "symbol" && NOISY_LABELS.test(layer.id)) map.setLayoutProperty(layer.id, "visibility", "none");
  }
}

const CLUSTER = "#0b7a83";
const CAMERA_KEY = "map.camera";

export interface MapViewProps {
  /** Places after kind/status/text filters (all countries; the scope is applied here). */
  places: Place[];
  scope?: Scope;
  onScope: (scope: Scope | undefined) => void;
  selected?: Place;
  onSelect: (place: Place | undefined) => void;
  /** Bumped when the camera should move to the selected place (search, list), not on map clicks. */
  flyKey?: number;
  hoverId?: string;
  onBounds?: (bounds: LngLatBounds) => void;
  onOpen: (place: Place) => void;
}

/**
 * The map engine. One kind of marker at a time: country bubbles → (a country) its cities → (a city) its places, with
 * small countries going straight to pins. Filters, scope and selection are owned by Explore.
 */
export function MapView({ places, scope, onScope, selected, onSelect, flyKey, hoverId, onBounds, onOpen }: MapViewProps) {
  const scoped = places.filter((p) => inScope(p, scope));
  const plan = planFor(scope, scoped);
  // In a city the map also shows the other pins in that country, so zooming out reveals the places around it.
  const cityCode = scope?.kind === "city" ? scope.code ?? scoped[0]?.countryCode ?? null : null;
  const mapPins = cityCode ? places.filter((p) => p.countryCode === cityCode) : scoped;

  const [styleId, setStyleId] = useState<MapStyleId>(getMapStyleId);
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<MLMap | undefined>(undefined);
  const placesRef = useRef<Place[]>([]);
  const planRef = useRef<Plan>(plan);
  const scopeRef = useRef<Scope | undefined>(scope);
  const selectedRef = useRef<string>("");
  const hoverRef = useRef<string>("");
  const appliedStyle = useRef(resolveMapStyle(getMapStyleId()).url);
  const clusterKeyRef = useRef(JSON.stringify(plan.clustering));
  const callbacks = useRef({ onScope, onSelect, onBounds });
  callbacks.current = { onScope, onSelect, onBounds };
  scopeRef.current = scope;

  // Create the map once; reopen where you left it (or fit all places the first time).
  useEffect(() => {
    if (!containerRef.current) return;
    const saved = readCamera();
    const map = new maplibregl.Map({
      container: containerRef.current, style: resolveMapStyle().url, center: saved?.center ?? [20, 25], zoom: saved?.zoom ?? 1.4,
      attributionControl: { compact: true }, dragRotate: false, pitchWithRotate: false, touchPitch: false,
    });
    map.touchZoomRotate.disableRotation();
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "bottom-left");
    map.addControl(new FitAllControl(() => fit(map, placesRef.current)), "bottom-left");
    mapRef.current = map;
    const hover = new maplibregl.Popup({ closeButton: false, closeOnClick: false, offset: 18, className: "map-hover" });

    // Runs for the first style and again whenever you pick another map style (setStyle drops custom layers).
    map.on("style.load", () => {
      const dark = resolveMapStyle(getMapStyleId()).dark;
      quietBasemap(map);
      map.addSource("places", {
        type: "geojson", data: toGeoJSON(placesRef.current, scopeRef.current?.kind === "city" ? scopeRef.current.key : null), ...planRef.current.clustering,
        clusterProperties: Object.fromEntries(Object.keys(GROUPS).map((g) => [g, ["+", ["case", ["==", ["get", "category"], g], 1, 0]]])),
      });
      const aggs = planRef.current.countries ? countryAggregates(placesRef.current) : [];
      const areaData = planRef.current.areas ? areaAggregates(placesRef.current).areas : [];
      ensureAreaImages(map, areaData);
      map.addSource("areas", { type: "geojson", data: areaGeoJSON(areaData), cluster: false, clusterRadius: 70, clusterMaxZoom: AREA_MAX_ZOOM - 1, clusterProperties: { total: ["+", ["get", "count"]] } });
      map.addSource("countries", { type: "geojson", data: countryGeoJSON(aggs) });
      void ensureCountryImages(map, aggs, () => (map.getSource("countries") as GeoJSONSource | undefined)?.setData(countryGeoJSON(countryAggregates(scopeRef.current ? [] : placesRef.current))));
      for (const c of Object.keys(GROUPS)) {
        if (!map.hasImage(`pin-${c}`)) map.addImage(`pin-${c}`, pinImage(c), { pixelRatio: 2 });
        if (!map.hasImage(`pin-sel-${c}`)) map.addImage(`pin-sel-${c}`, pinImage(c, true), { pixelRatio: 2 });
      }
      // Clusters: size by count (1–4 · 5–14 · 15–29 · 30+) with a soft translucent ring.
      const size = ["step", ["get", "point_count"], 14, 5, 18, 15, 22, 30, 27] as unknown as number;
      map.addLayer({ id: "cluster-ring", type: "circle", source: "places", filter: ["has", "point_count"], paint: { "circle-color": CLUSTER, "circle-opacity": 0.18, "circle-radius": ["+", size, 6] as unknown as number } });
      map.addLayer({ id: "clusters", type: "circle", source: "places", filter: ["has", "point_count"], paint: { "circle-color": CLUSTER, "circle-radius": size, "circle-stroke-width": 2, "circle-stroke-color": "#ffffff" } });
      map.addLayer({ id: "cluster-count", type: "symbol", source: "places", filter: ["has", "point_count"], layout: { "text-field": ["get", "point_count_abbreviated"], "text-size": 12.5, "text-font": ["Noto Sans Bold"], "text-allow-overlap": true }, paint: { "text-color": "#ffffff" } });
      map.addLayer({
        id: "place-points", type: "symbol", source: "places", filter: ["!", ["has", "point_count"]],
        layout: {
          "icon-image": ["concat", "pin-", ["get", "category"]], "icon-allow-overlap": true, "icon-ignore-placement": true,
          "icon-size": ["case", ["get", "cityPlace"], 1.3, ["get", "focus"], 0.9, 0.65],
          "symbol-sort-key": ["case", ["get", "cityPlace"], 2, ["get", "focus"], 1, 0],
        },
        paint: { "icon-opacity": ["case", ["==", ["get", "status"], "notInterested"], 0.4, ["get", "focus"], 1, 0.55] },
      });
      map.addLayer({ id: "place-hover", type: "symbol", source: "places", filter: ["==", ["get", "id"], hoverRef.current], layout: { "icon-image": ["concat", "pin-sel-", ["get", "category"]], "icon-size": 0.85, "icon-allow-overlap": true, "icon-ignore-placement": true } });
      map.addLayer({ id: "place-selected", type: "symbol", source: "places", filter: ["==", ["get", "id"], selectedRef.current], layout: { "icon-image": ["concat", "pin-sel-", ["get", "category"]], "icon-allow-overlap": true, "icon-ignore-placement": true } });
      map.addLayer({
        id: "place-labels", type: "symbol", source: "places", filter: ["all", ["!", ["has", "point_count"]], ["get", "focus"]], minzoom: 11,
        layout: {
          "text-field": ["get", "name"], "text-size": ["case", ["get", "cityPlace"], 14, 12], "text-offset": ["case", ["get", "cityPlace"], ["literal", [0, 1.6]], ["literal", [0, 1.2]]],
          "text-anchor": "top", "text-font": ["Noto Sans Bold"], "text-max-width": 10, "symbol-sort-key": ["case", ["get", "cityPlace"], 0, 1],
        },
        paint: { "text-color": dark ? "#f2f2f4" : "#1c1c1e", "text-halo-color": dark ? "#1c1c1e" : "#ffffff", "text-halo-width": 1.6 },
      });
      // Inside a picked country: one bubble per city/region. Neighbouring cities merge into a round total bubble.
      const areaSize = ["step", ["get", "total"], 16, 5, 20, 15, 25, 30, 30] as unknown as number;
      map.addLayer({ id: "area-cluster-ring", type: "circle", source: "areas", filter: ["has", "point_count"], paint: { "circle-color": CLUSTER, "circle-opacity": 0.18, "circle-radius": ["+", areaSize, 6] as unknown as number } });
      map.addLayer({ id: "area-clusters", type: "circle", source: "areas", filter: ["has", "point_count"], paint: { "circle-color": CLUSTER, "circle-radius": areaSize, "circle-stroke-width": 2, "circle-stroke-color": "#ffffff" } });
      map.addLayer({ id: "area-cluster-count", type: "symbol", source: "areas", filter: ["has", "point_count"], layout: { "text-field": ["to-string", ["get", "total"]], "text-size": 13, "text-font": ["Noto Sans Bold"], "text-allow-overlap": true }, paint: { "text-color": "#ffffff" } });
      map.addLayer({ id: "area-minor", type: "circle", source: "areas", filter: ["all", ["!", ["has", "point_count"]], ["!", ["get", "major"]]], paint: { "circle-color": CLUSTER, "circle-radius": 6, "circle-opacity": 0.85, "circle-stroke-width": 2, "circle-stroke-color": "#ffffff" } });
      map.addLayer({ id: "area-bubbles", type: "symbol", source: "areas", filter: ["all", ["!", ["has", "point_count"]], ["get", "major"]], layout: { "icon-image": ["get", "image"], "icon-allow-overlap": true, "icon-ignore-placement": true, "symbol-sort-key": ["-", 0, ["get", "count"]] } });
      map.addLayer({ id: "country-bubbles", type: "symbol", source: "countries", layout: { "icon-image": ["get", "image"], "icon-allow-overlap": true, "icon-ignore-placement": true, "symbol-sort-key": ["-", 0, ["get", "count"]] } });
      applyPlan(map, planRef.current);
    });

    const cityScope = (key: string): Scope => {
      const parent = scopeRef.current;
      const first = placesRef.current.find((x) => areaKey(x) === key);
      const country: Scope | undefined = parent?.kind === "country" ? parent
        : first?.countryCode ? { kind: "country", key: first.countryCode, label: first.country ?? first.countryCode, code: first.countryCode } : undefined;
      return { kind: "city", key, label: key, code: first?.countryCode ?? parent?.code ?? null, parent: country };
    };
    const at = (e: maplibregl.MapLayerMouseEvent) => (e.features?.[0]?.geometry as GeoJSON.Point | undefined)?.coordinates as [number, number] | undefined;

    map.on("load", () => {
      map.on("click", "clusters", async (e) => {
        const feature = map.queryRenderedFeatures(e.point, { layers: ["clusters"] })[0];
        const zoom = await (map.getSource("places") as GeoJSONSource).getClusterExpansionZoom(feature.properties.cluster_id);
        map.easeTo({ center: (feature.geometry as GeoJSON.Point).coordinates as [number, number], zoom });
      });
      map.on("click", "place-points", (e) => {
        // Several pins on (almost) the same spot: offer a short list instead of guessing which one.
        const box: [maplibregl.PointLike, maplibregl.PointLike] = [[e.point.x - 14, e.point.y - 14], [e.point.x + 14, e.point.y + 14]];
        const hit = map.queryRenderedFeatures(e.point, { layers: ["place-points"] });
        const city = hit.find((f) => f.properties?.cityPlace);
        if (city) { callbacks.current.onSelect(placesRef.current.find((p) => p.id === city.properties?.id)); return; }
        const near = map.queryRenderedFeatures(box, { layers: ["place-points"] }).filter((f) => !f.properties?.cityPlace);
        const ids = [...new Set(near.map((f) => f.properties?.id as string))];
        const here = ids.map((id) => placesRef.current.find((p) => p.id === id)).filter((p): p is Place => !!p);
        if (here.length > 1) showChooser(map, e.lngLat, here, (p) => callbacks.current.onSelect(p));
        else callbacks.current.onSelect(here[0]);
      });
      map.on("mousemove", "clusters", (e) => {
        const props = e.features?.[0]?.properties ?? {};
        const counts = Object.fromEntries(Object.keys(GROUPS).map((g) => [g, Number(props[g] ?? 0)]));
        const p = at(e);
        if (props.point_count && p) hover.setLngLat(p).setHTML(hoverCard(`${props.point_count} saved places`, "Click to zoom in", counts)).addTo(map);
      });
      map.on("mousemove", "country-bubbles", (e) => {
        const p = e.features?.[0]?.properties;
        const c = at(e);
        if (p && c) hover.setLngLat(c).setHTML(hoverCard(p.name, `${p.count} saved place${p.count === 1 ? "" : "s"} · click to open`, countsFor(placesRef.current.filter((x) => x.countryCode === p.cc)))).addTo(map);
      });
      for (const layer of ["area-bubbles", "area-minor"]) {
        map.on("mousemove", layer, (e) => {
          const p = e.features?.[0]?.properties;
          const c = at(e);
          if (p && c) hover.setLngLat(c).setHTML(hoverCard(p.key, `${p.count} saved places · click to open`, countsFor(placesRef.current.filter((x) => areaKey(x) === p.key)))).addTo(map);
        });
        map.on("click", layer, (e) => {
          const p = e.features?.[0]?.properties;
          hover.remove();
          if (p) callbacks.current.onScope(cityScope(String(p.key)));
        });
      }
      map.on("mousemove", "area-clusters", async (e) => {
        const f = e.features?.[0];
        const c = at(e);
        if (!f || !c) return;
        const leaves = await (map.getSource("areas") as GeoJSONSource).getClusterLeaves(f.properties.cluster_id, 50, 0);
        const names = leaves.sort((a, b) => (b.properties?.count ?? 0) - (a.properties?.count ?? 0)).map((l) => String(l.properties?.key));
        const members = new Set(names);
        hover.setLngLat(c).setHTML(hoverCard(names.slice(0, 3).join(", ") + (names.length > 3 ? ` +${names.length - 3}` : ""), `${f.properties.total} saved places · click to zoom in`,
          countsFor(placesRef.current.filter((x) => members.has(areaKey(x)))))).addTo(map);
      });
      map.on("click", "area-clusters", async (e) => {
        const f = map.queryRenderedFeatures(e.point, { layers: ["area-clusters"] })[0];
        hover.remove();
        const zoom = await (map.getSource("areas") as GeoJSONSource).getClusterExpansionZoom(f.properties.cluster_id);
        map.easeTo({ center: (f.geometry as GeoJSON.Point).coordinates as [number, number], zoom });
      });
      map.on("click", "country-bubbles", (e) => {
        const p = e.features?.[0]?.properties;
        hover.remove();
        if (p) callbacks.current.onScope({ kind: "country", key: p.cc, label: p.name, code: p.cc });
      });
      for (const layer of ["clusters", "place-points", "country-bubbles", "area-bubbles", "area-clusters", "area-minor"]) {
        map.on("mouseenter", layer, () => { map.getCanvas().style.cursor = "pointer"; });
        map.on("mouseleave", layer, () => { map.getCanvas().style.cursor = ""; hover.remove(); });
      }
      map.on("moveend", () => {
        callbacks.current.onBounds?.(map.getBounds());
        writeCamera(map);
      });
      if (!saved) fit(map, placesRef.current);
      callbacks.current.onBounds?.(map.getBounds());
    });
    return () => map.remove();
  }, []);

  // Picking a country or city (from search, the list, a bubble or the breadcrumbs) moves the camera to it.
  const scopeKey = scope ? `${scope.kind}:${scope.key}` : "";
  const firstScope = useRef(true);
  useEffect(() => {
    if (firstScope.current) { firstScope.current = false; return; }
    const map = mapRef.current;
    if (!map) return;
    const target = places.filter((p) => inScope(p, scope));
    fit(map, target.length ? target : places, scope?.kind === "city" ? CITY_ZOOM : scope ? "compact" : undefined);
  }, [scopeKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Push data into the map and show only the layers the current view needs.
  const planKey = JSON.stringify(plan);
  useEffect(() => {
    placesRef.current = mapPins;
    planRef.current = plan;
    const map = mapRef.current;
    if (!map?.getSource("places")) return;
    const pins = map.getSource("places") as GeoJSONSource;
    // Only touch the clustering setting when it really changes (re-setting it mid-animation can leave pins undrawn).
    const clusterKey = JSON.stringify(plan.clustering);
    if (clusterKeyRef.current !== clusterKey) { pins.setClusterOptions(plan.clustering); clusterKeyRef.current = clusterKey; }
    const areaData = plan.areas ? areaAggregates(scoped).areas : [];
    // In a country's city view, small cities (1–2 places) show their pins instead of a bubble.
    const pinCities = plan.areas ? rankAreas(areaData).pinCities : null;
    const focusCity = scope?.kind === "city" ? scope.key : null;
    const pinData = toGeoJSON(pinCities ? scoped.filter((p) => pinCities.has(areaKey(p))) : mapPins, focusCity);
    pins.setData(pinData);
    ensureAreaImages(map, areaData);
    (map.getSource("areas") as GeoJSONSource).setData(areaGeoJSON(areaData));
    const countries = map.getSource("countries") as GeoJSONSource;
    const aggs = plan.countries ? countryAggregates(scoped) : [];
    countries.setData(countryGeoJSON(aggs));
    void ensureCountryImages(map, aggs, () => countries.setData(countryGeoJSON(aggs)));
    applyPlan(map, plan);
    // Make sure the new markers are drawn as soon as a zoom-in finishes, without needing to move the mouse.
    const redraw = () => { pins.setData(pinData); map.triggerRepaint(); };
    map.triggerRepaint();
    map.once("moveend", redraw);
    const t = window.setTimeout(redraw, 700);
    return () => { map.off("moveend", redraw); window.clearTimeout(t); };
  }, [places, planKey, scopeKey]); // eslint-disable-line react-hooks/exhaustive-deps

  // Switch base map (remembered); follow the app's light/dark appearance for "Match system".
  useEffect(() => {
    setMapStyleId(styleId);
    const apply = () => {
      const url = resolveMapStyle(styleId).url;
      if (mapRef.current && appliedStyle.current !== url) mapRef.current.setStyle(url, { diff: false });
      appliedStyle.current = url;
    };
    apply();
    window.addEventListener("themechange", apply);
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    mq?.addEventListener?.("change", apply);
    return () => { window.removeEventListener("themechange", apply); mq?.removeEventListener?.("change", apply); };
  }, [styleId]);

  // Selected and hovered pins.
  useEffect(() => {
    selectedRef.current = selected?.id ?? "";
    const map = mapRef.current;
    if (map?.getLayer("place-selected")) map.setFilter("place-selected", ["==", ["get", "id"], selected?.id ?? ""]);
  }, [selected?.id]);
  useEffect(() => {
    hoverRef.current = hoverId ?? "";
    const map = mapRef.current;
    if (map?.getLayer("place-hover")) map.setFilter("place-hover", ["==", ["get", "id"], hoverId ?? ""]);
  }, [hoverId]);
  // Fly to a place chosen in search or the list.
  useEffect(() => {
    const map = mapRef.current;
    if (!map || !flyKey || !selected) return;
    map.easeTo({ center: [selected.longitude, selected.latitude], zoom: Math.max(map.getZoom(), 13), duration: 700 });
  }, [flyKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const crumbs: Scope[] = [];
  for (let s = scope; s; s = s.parent) crumbs.unshift(s);

  return (
    <div className="map-wrap">
      <div ref={containerRef} className="map" role="region" aria-label="Map of saved places" />
      <nav className="map-scope" aria-label="Map area">
        {scope && (
          <>
            <button className="map-crumb" onClick={() => onScope(undefined)}>All countries</button>
            {crumbs.map((c, i) => (
              <span key={c.key} className="row" style={{ gap: 4 }}>
                <ChevronRight size={14} className="faint" aria-hidden="true" />
                <button className={`map-crumb ${i === crumbs.length - 1 ? "current" : ""}`} onClick={() => i < crumbs.length - 1 && onScope(c)}
                        aria-current={i === crumbs.length - 1 ? "location" : undefined}>
                  {c.kind === "country" && c.code && <Flag code={c.code} name={c.label} />} {c.kind === "country" && c.code ? countryName(c, places) : c.label}
                </button>
              </span>
            ))}
          </>
        )}
      </nav>
      <div className="map-float top-right">
        <Menu trigger={<button className="btn ghost small" aria-label="Map style"><Layers size={15} /> {MAP_STYLES.find((s) => s.id === styleId)?.label}</button>}>
          {MAP_STYLES.map((s) => (
            <MenuItem key={s.id} icon={<span className={`style-swatch swatch-${s.id}`} />} onSelect={() => setStyleId(s.id)}
                      hint={styleId === s.id ? <Check size={14} /> : undefined}>
              <span className="col"><span>{s.label}</span><span className="menu-hint-line">{s.hint}</span></span>
            </MenuItem>
          ))}
        </Menu>
      </div>
      {selected && <MapPreview place={selected} onClose={() => onSelect(undefined)} onOpen={() => onOpen(selected)} />}
    </div>
  );
}

function countryName(scope: Scope, places: Place[]): string {
  return places.find((p) => p.countryCode === scope.key)?.country ?? scope.label;
}

function readCamera(): { center: [number, number]; zoom: number } | undefined {
  try {
    const v = JSON.parse(localStorage.getItem(CAMERA_KEY) ?? "null");
    if (v && Array.isArray(v.center) && typeof v.zoom === "number") return v;
  } catch { /* first launch */ }
  return undefined;
}

function writeCamera(map: MLMap) {
  try {
    const c = map.getCenter();
    localStorage.setItem(CAMERA_KEY, JSON.stringify({ center: [c.lng, c.lat], zoom: map.getZoom() }));
  } catch { /* not remembered */ }
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
    b.title = "Show all places";
    b.setAttribute("aria-label", "Show all places");
    b.className = "fit-all-btn";
    b.innerHTML = '<svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true"><path d="M2 6V2h4M10 2h4v4M14 10v4h-4M6 14H2v-4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>';
    b.onclick = this.onFit;
    this.el.appendChild(b);
    return this.el;
  }
  onRemove() { this.el?.remove(); }
}

/** The card shown when you pick a pin: photo with the name over it, where, status, and what to do next. */
function MapPreview({ place, onClose, onOpen }: { place: Place; onClose: () => void; onOpen: () => void }) {
  const cat = CATEGORY[place.category] ?? CATEGORY.other;
  const action = useAction();
  return (
    <div className="map-preview" key={place.id} role="dialog" aria-label={place.canonicalName}>
      <div className="map-preview-hero">
        <Thumb path={place.heroImagePath ?? place.thumbnailPath} focus={place.heroFocus} fallback={<CategoryBadge category={place.category} size={56} />} />
        <div className="map-preview-shade" />
        <button className="map-preview-close" onClick={onClose} aria-label="Close"><X size={15} /></button>
        <div className="map-preview-title">
          <span className="map-preview-kind" style={{ background: colorOf(place.category) }}><CategoryGlyph category={place.category} size={12} strokeWidth={2.4} /> {cat.label}</span>
          <h3>{place.canonicalName}</h3>
        </div>
      </div>
      <div className="map-preview-body">
        <div className="map-preview-where">
          <Flag code={place.countryCode} name={place.country} />
          <span className="truncate">{[place.city !== place.canonicalName ? place.city : null, place.country].filter(Boolean).join(", ") || "Unknown location"}</span>
        </div>
        {place.summaryText && <p className="small clamp-3" style={{ color: "var(--text-2)" }}>{place.summaryText}</p>}
        <div className="map-preview-meta">
          <label className={`status-tag status-${place.personalStatus}`} style={{ paddingRight: 2 }}>
            <StatusIcon status={place.personalStatus} size={12} />
            <select className="status-select" aria-label="Status" value={place.personalStatus} disabled={action.busy}
                    style={{ background: "transparent", color: "inherit" }}
                    onChange={(e) => action.run(() => PlaceService.update(place.id, "personalStatus", e.target.value))}>
              {Object.entries(STATUS).map(([k, s]) => <option key={k} value={k}>{s.label}</option>)}
            </select>
          </label>
          <span className="meta-pill">{place.sourceCount} source{place.sourceCount === 1 ? "" : "s"}</span>
          {place.memoryCount > 0 && <span className="meta-pill">{place.memoryCount} of your photos</span>}
        </div>
        <div className="map-preview-actions">
          <button className="btn primary grow" onClick={onOpen}><Maximize2 size={14} /> Details</button>
          <button className="btn" title="Open in Apple Maps"
                  onClick={() => openUrl(`https://maps.apple.com/?ll=${place.latitude},${place.longitude}&q=${encodeURIComponent(place.canonicalName)}`)}>
            <Navigation size={14} /> Maps <ExternalLink size={11} className="faint" />
          </button>
        </div>
      </div>
    </div>
  );
}

type Range = [number, number] | null;
type Clustering = { cluster: boolean; clusterRadius: number; clusterMaxZoom: number };
/** Which markers show. One kind at a time: countries, or a country's cities, or a city's places. */
interface Plan { countries: Range; areas: Range; pins: Range; clustering: Clustering; labelsFrom: number }

/** A country this small goes straight to its pins (no city step). */
const FEW_PLACES = 5;
const FLAT: Clustering = { cluster: false, clusterRadius: 44, clusterMaxZoom: 10 };

export function planFor(scope: Scope | undefined, inScopePlaces: Place[]): Plan {
  const pins: Plan = { countries: null, areas: null, pins: [0, 24], clustering: FLAT, labelsFrom: 11 };
  if (scope?.kind === "city") return pins;
  if (scope?.kind === "country") {
    const oneCity = new Set(inScopePlaces.map(areaKey)).size <= 1;
    if (inScopePlaces.length <= FEW_PLACES || oneCity) return { ...pins, labelsFrom: 0 };
    return { countries: null, areas: [0, 24], pins: [0, 24], clustering: FLAT, labelsFrom: 11 };
  }
  return { countries: [0, 24], areas: null, pins: null, clustering: FLAT, labelsFrom: 11 };
}

const LAYER_GROUPS: Record<"countries" | "areas" | "pins", string[]> = {
  countries: ["country-bubbles"],
  areas: ["area-cluster-ring", "area-clusters", "area-cluster-count", "area-minor", "area-bubbles"],
  pins: ["cluster-ring", "clusters", "cluster-count", "place-points", "place-hover", "place-selected", "place-labels"],
};

function applyPlan(map: MLMap, plan: Plan) {
  (Object.keys(LAYER_GROUPS) as (keyof typeof LAYER_GROUPS)[]).forEach((group) => {
    const range = plan[group];
    for (const id of LAYER_GROUPS[group]) {
      if (!map.getLayer(id)) continue;
      map.setLayoutProperty(id, "visibility", range ? "visible" : "none");
      if (range) map.setLayerZoomRange(id, id === "place-labels" ? Math.max(range[0], plan.labelsFrom) : range[0], range[1]);
    }
  });
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

/** "Xi'an" = "Xian" = "xi an": compare names ignoring case, accents and punctuation. */
function sameName(a: string, b: string): boolean {
  const n = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase().replace(/[^\p{L}\p{N}]/gu, "");
  return n(a) === n(b);
}

/**
 * Pins as GeoJSON. Inside a picked city (`focusCity`), that city's own pins are "focus" (full size, named) and the
 * neighbours around it are smaller and faded; the place that *is* the city (e.g. "Beijing") is "cityPlace".
 */
export function toGeoJSON(places: Place[], focusCity?: string | null): GeoJSON.FeatureCollection {
  const labelled = !!focusCity && places.filter((p) => areaKey(p) === focusCity).reduce((n, p) => n + support(p), 0) > PIN_CITY_MAX;
  return {
    type: "FeatureCollection",
    features: places.map((p) => {
      const inFocus = !focusCity || areaKey(p) === focusCity;
      const isCity = labelled && inFocus && [p.canonicalName, ...p.alternativeNames].some((n) => sameName(n, focusCity!));
      return {
        type: "Feature",
        geometry: { type: "Point", coordinates: [p.longitude, p.latitude] },
        properties: { id: p.id, name: p.canonicalName, category: groupOf(p.category), status: p.personalStatus, focus: inFocus, cityPlace: isCity },
      };
    }),
  };
}

/** Inside a country, nearby city bubbles merge until this zoom so they never pile up. */
const AREA_MAX_ZOOM = 11;
/** Picking a city zooms to about neighbourhood level. */
const CITY_ZOOM = 11;

/** Fits the map to places. `minZoom` = zoom at least this close; "compact" = past clustering if they're close together. */
export function fit(map: MLMap, places: Place[], minZoom?: number | "compact"): number | undefined {
  if (places.length === 0) return undefined;
  const padding = { top: 70, bottom: 60, left: 60, right: 60 };
  if (places.length === 1) {
    const z = Math.max(12, typeof minZoom === "number" ? minZoom : 0);
    map.easeTo({ center: [places[0].longitude, places[0].latitude], zoom: z });
    return z;
  }
  const bounds = new maplibregl.LngLatBounds();
  places.forEach((p) => bounds.extend([p.longitude, p.latitude]));
  const camera = map.cameraForBounds(bounds, { padding, maxZoom: 14 });
  if (!camera) return undefined;
  const zoom = camera.zoom ?? 0;
  const floor = minZoom === "compact" ? 0 : minZoom !== undefined && zoom >= minZoom - 2 ? minZoom : 0;
  const target = Math.min(Math.max(zoom, floor), 12.5);
  map.easeTo({ ...camera, zoom: target, duration: 650 });
  return target;
}
