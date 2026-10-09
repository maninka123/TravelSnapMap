import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import maplibregl from "maplibre-gl";
import {
  AlertTriangle, ArrowDown, ArrowUp, CalendarDays, Check, ChevronLeft, Download, GripVertical, Luggage, Map as MapIcon, MoreHorizontal,
  Navigation, Plus, Search, Trash2, X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { PlaceService, TripService } from "../../api/services";
import type { Place, Trip, TripEntry } from "../../api/types";
import { CategoryChip, Empty, ErrorNote, Modal, PlaceLine, Thumb, plural } from "../../components/common";
import { Menu, MenuItem, MenuSeparator, useToast } from "../../components/ui";
import { CategoryBadge, colorOf } from "../../lib/categoryIcons";
import { StatusIcon } from "../../lib/icons";
import { withMetro } from "../../lib/metro";
import { openUrl } from "../../lib/open";
import { resolveMapStyle } from "../../lib/mapStyle";
import { useAction, useLoad, useNav } from "../../lib/nav";
import { quietBasemap } from "../map/MapView";
import { byDay, citySummary, dayDate, dayList, formatRange, moveStop, nudge, type Stop } from "./itinerary";

/** My Trips: plan where to go, day by day, from the places you saved. */
export function TripsView() {
  const nav = useNav();
  const { data: trips } = useLoad(() => TripService.list(), []);
  const open = trips?.find((t) => t.id === nav.tripId);
  if (open) return <TripDetail trip={open} onBack={() => nav.setTripId(undefined)} />;
  return <TripList trips={trips} onOpen={(id) => nav.setTripId(id)} />;
}

function TripList({ trips, onOpen }: { trips?: Trip[]; onOpen: (id: string) => void }) {
  const [creating, setCreating] = useState(false);
  return (
    <div className="page">
      <div className="page-head">
        <h1>My Trips</h1>
        <button className="btn primary" onClick={() => setCreating(true)}><Plus size={15} /> New trip</button>
      </div>
      {!trips ? null : trips.length === 0 ? (
        <Empty icon={<Luggage size={26} />} title="Plan your first trip"
               actions={<button className="btn primary" onClick={() => setCreating(true)}><Plus size={14} /> New trip</button>}>
          Group the places you saved into a trip, arrange them day by day, and take the plan with you.
        </Empty>
      ) : (
        <div className="trip-grid">
          {trips.map((t) => <TripCard key={t.id} trip={t} onOpen={() => onOpen(t.id)} />)}
          <button className="trip-card trip-new" onClick={() => setCreating(true)}><Plus size={22} /><span className="strong">New trip</span></button>
        </div>
      )}
      {creating && <NewTripDialog onClose={() => setCreating(false)} onCreated={(id) => { setCreating(false); onOpen(id); }} />}
    </div>
  );
}

function TripCard({ trip, onOpen }: { trip: Trip; onOpen: () => void }) {
  const { data: entries = [] } = useLoad(() => TripService.entries(trip.id), [trip.id]);
  const covers = entries.map((e) => e.place.heroImagePath ?? e.place.thumbnailPath).filter(Boolean).slice(0, 3) as string[];
  const cities = citySummary(entries).slice(0, 3).map(([c]) => c);
  const range = formatRange(trip);
  return (
    <button className="trip-card" onClick={onOpen}>
      <div className={`trip-cover ${covers.length < 2 ? "single" : ""}`}>
        {covers.length === 0 ? <div className="thumb thumb-empty"><MapIcon size={28} /></div> : covers.map((c, i) => <Thumb key={i} path={c} />)}
      </div>
      <div className="trip-card-body">
        <span className="trip-card-title">{trip.name}</span>
        <span className="muted small">{[range, plural(trip.placeCount, "place")].filter(Boolean).join(" · ")}</span>
        {cities.length > 0 && <span className="small truncate" style={{ color: "var(--text-2)" }}>{cities.join(" · ")}</span>}
      </div>
    </button>
  );
}

function NewTripDialog({ onClose, onCreated }: { onClose: () => void; onCreated: (id: string) => void }) {
  const [name, setName] = useState("");
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");
  const action = useAction();
  const create = async () => {
    if (!name.trim()) return;
    const id = await action.run(async () => {
      const id = await TripService.create(name.trim());
      if (start || end) await TripService.update(id, name.trim(), start || null, end || null, "");
      return id;
    });
    if (id) onCreated(id);
  };
  return (
    <Modal title="New trip" onClose={onClose}>
      <form className="stack" onSubmit={(e) => { e.preventDefault(); void create(); }}>
        <div className="field"><label htmlFor="trip-name">Name</label><input id="trip-name" autoFocus placeholder="e.g. Japan in spring" value={name} onChange={(e) => setName(e.target.value)} /></div>
        <div className="row">
          <div className="field grow"><label htmlFor="trip-start">From <span className="muted">(optional)</span></label><input id="trip-start" type="date" value={start} onChange={(e) => setStart(e.target.value)} /></div>
          <div className="field grow"><label htmlFor="trip-end">To</label><input id="trip-end" type="date" min={start || undefined} value={end} onChange={(e) => setEnd(e.target.value)} /></div>
        </div>
        <ErrorNote error={action.error} onClose={action.clearError} />
        <div className="modal-actions"><button type="button" className="btn" onClick={onClose}>Cancel</button><button className="btn primary" disabled={!name.trim() || action.busy}>Create trip</button></div>
      </form>
    </Modal>
  );
}

function TripDetail({ trip, onBack }: { trip: Trip; onBack: () => void }) {
  const nav = useNav();
  const toast = useToast();
  const action = useAction();
  const { data: loaded } = useLoad(() => TripService.entries(trip.id), [trip.id]);
  const { data: allWarnings = {} } = useLoad(() => PlaceService.warnings(), []);
  const [order, setOrder] = useState<TripEntry[]>([]);
  useEffect(() => { if (loaded) setOrder(withMetroEntries(loaded)); }, [loaded]);
  const [extraDays, setExtraDays] = useState(0);
  const [adding, setAdding] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [form, setForm] = useState({ name: trip.name, start: trip.startDate ?? "", end: trip.endDate ?? "", notes: trip.notes });
  useEffect(() => setForm({ name: trip.name, start: trip.startDate ?? "", end: trip.endDate ?? "", notes: trip.notes }), [trip.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const save = (next = form) => action.run(() => TripService.update(trip.id, next.name.trim() || trip.name, next.start || null, next.end || null, next.notes));
  const days = dayList(order, { startDate: form.start || null, endDate: form.end || null }, extraDays);
  const grouped = byDay(order);
  const warnings = order.filter((e) => allWarnings[e.place.id]).map((e) => ({ entry: e, warnings: allWarnings[e.place.id] }));
  const numbers = new Map(order.map((e, i) => [e.id, i + 1]));
  const visited = order.filter((e) => e.place.personalStatus === "visited").length;

  // Saves the new order right away (optimistic), then the whole itinerary in one transaction.
  const apply = (next: Stop[]) => {
    const byId = new Map(order.map((e) => [e.id, e]));
    const entries = next.map((s) => ({ ...byId.get(s.id)!, day: s.day }));
    setOrder(entries);
    void action.run(() => TripService.reorder(trip.id, next.map((s) => ({ id: s.id, day: s.day }))));
  };
  const exportTrip = async () => {
    const path = await chooseExportPath(`${trip.name}.md`);
    if (!path) return;
    if (await action.run(() => TripService.exportMarkdown(trip.id, path)) !== undefined) toast.ok("Itinerary exported.");
  };

  return (
    <div className="page">
      <div className="page-head">
        <button className="btn ghost small" onClick={onBack}><ChevronLeft size={16} /> Trips</button>
        <input className="trip-title-input" aria-label="Trip name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })}
               onBlur={() => form.name.trim() && form.name !== trip.name && save()} />
        <span className="spacer" />
        <button className="btn" onClick={() => setAdding(true)}><Plus size={15} /> Add places</button>
        <button className="btn" onClick={exportTrip} disabled={order.length === 0}><Download size={15} /> Export</button>
        <Menu trigger={<button className="btn icon-only" aria-label="More"><MoreHorizontal size={16} /></button>}>
          <MenuItem icon={<CalendarDays size={15} />} onSelect={() => setExtraDays((n) => n + 1)}>Add a day</MenuItem>
          <MenuSeparator />
          <MenuItem icon={<Trash2 size={15} />} danger onSelect={() => setConfirmDelete(true)}>Delete trip…</MenuItem>
        </Menu>
      </div>
      <div className="trip-dates" style={{ marginTop: -6, marginBottom: 16 }}>
        <CalendarDays size={15} />
        <input type="date" aria-label="Start date" value={form.start} onChange={(e) => { const f = { ...form, start: e.target.value }; setForm(f); void save(f); }} />
        <span>to</span>
        <input type="date" aria-label="End date" min={form.start || undefined} value={form.end} onChange={(e) => { const f = { ...form, end: e.target.value }; setForm(f); void save(f); }} />
        <span className="small">{plural(order.length, "place")}{visited ? ` · ${visited} visited` : ""}</span>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />

      {order.length === 0 ? (
        <Empty icon={<MapIcon size={26} />} title="No places in this trip yet"
               actions={<button className="btn primary" onClick={() => setAdding(true)}><Plus size={14} /> Add saved places</button>}>
          Add places you saved, then drag them into days.
        </Empty>
      ) : (
        <div className="trip-layout">
          <div>
            {warnings.length > 0 && (
              <details className="warnings-panel trip-warnings" open={warnings.length <= 3 || undefined}>
                <summary className="warnings-head"><AlertTriangle size={15} /> Check before you go
                  <span className="muted small" style={{ fontWeight: 400 }}>— {plural(warnings.length, "place has", "places have")} out-of-date or conflicting saved info</span></summary>
                {warnings.map(({ entry, warnings: w }) => (
                  <div key={entry.id} className="warning-row" onClick={() => nav.openPlace(entry.place.id)} role="button" tabIndex={0}>
                    <AlertTriangle size={14} />
                    <div><div className="strong">{entry.place.canonicalName}</div><div className="muted small">{w.map((x) => x.title).join(" · ")}</div></div>
                  </div>
                ))}
              </details>
            )}
            {days.map((d) => <DayColumn key={d} day={d} label={`Day ${d}`} date={dayDate({ startDate: form.start || null }, d)} entries={grouped.get(d) ?? []}
                                         numbers={numbers} order={order} onApply={apply} dayCount={days.length} />)}
            <DayColumn day={null} label="Not scheduled" entries={grouped.get(null) ?? []} numbers={numbers} order={order} onApply={apply} dayCount={days.length}
                       emptyText={days.length ? "Drag places here to take them out of a day." : "Add dates or a day (⋯ menu), then drag places into days."} />
            <div className="field" style={{ marginTop: 18 }}>
              <label htmlFor="trip-notes">Trip notes</label>
              <textarea id="trip-notes" placeholder="Bookings, reminders, things to pack…" value={form.notes}
                        onChange={(e) => setForm({ ...form, notes: e.target.value })} onBlur={() => form.notes !== trip.notes && save()} />
            </div>
          </div>
          <aside className="trip-side">
            <TripMap entries={order} numbers={numbers} onOpen={(p) => nav.openPlace(p.id)} />
            <div className="side-card">
              <h5>Where</h5>
              <div className="row wrap" style={{ gap: 6 }}>{citySummary(order).map(([city, n]) => <span key={city} className="chip">{city} <span className="muted">{n}</span></span>)}</div>
              <p className="muted small">Stops are numbered in itinerary order. Travel times and opening hours aren't calculated — check them before you go.</p>
            </div>
          </aside>
        </div>
      )}
      {adding && <AddPlaces trip={trip} existing={new Set(order.map((e) => e.place.id))} onClose={() => setAdding(false)} />}
      {confirmDelete && (
        <Modal title={`Delete “${trip.name}”?`} onClose={() => setConfirmDelete(false)}>
          <p className="muted">The trip and its itinerary are removed. Your saved places stay in Explore.</p>
          <div className="modal-actions">
            <button className="btn" onClick={() => setConfirmDelete(false)}>Cancel</button>
            <button className="btn danger-solid" onClick={async () => { if (await action.run(() => TripService.remove(trip.id)) !== undefined) onBack(); }}>Delete trip</button>
          </div>
        </Modal>
      )}
    </div>
  );
}

const chooseExportPath = (defaultPath: string) =>
  saveDialog({ title: "Export itinerary", defaultPath, filters: [{ name: "Markdown", extensions: ["md"] }] }).then((p) => p ?? undefined);

function withMetroEntries(entries: TripEntry[]): TripEntry[] {
  const metro = new Map(withMetro(entries.map((e) => e.place)).map((p) => [p.id, p.metro]));
  return entries.map((e) => ({ ...e, place: { ...e.place, metro: metro.get(e.place.id) } }));
}

const DRAG = "application/x-tsm-stop";

/** One day of the itinerary: a drop target, its stops in order. */
function DayColumn({ day, label, date, entries, numbers, order, onApply, emptyText, dayCount }: {
  day: number | null; label: string; date?: string | null; entries: TripEntry[]; numbers: Map<string, number>; order: TripEntry[];
  onApply: (next: Stop[]) => void; emptyText?: string; dayCount: number;
}) {
  const nav = useNav();
  const action = useAction();
  const [over, setOver] = useState(false);
  const [dropBefore, setDropBefore] = useState<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const stops = order.map((e) => ({ id: e.id, day: e.day }));
  const accept = (e: DragEvent) => e.dataTransfer.types.includes(DRAG);
  const drop = (e: DragEvent, beforeId: string | null) => {
    e.preventDefault();
    e.stopPropagation();
    setOver(false);
    setDropBefore(null);
    const id = e.dataTransfer.getData(DRAG);
    if (id) onApply(moveStop(stops, id, day, beforeId));
  };

  return (
    <section className={`day ${over ? "drag-over" : ""}`} aria-label={label}
             onDragOver={(e) => { if (accept(e)) { e.preventDefault(); setOver(true); } }}
             onDragLeave={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node)) { setOver(false); setDropBefore(null); } }}
             onDrop={(e) => drop(e, null)}>
      <div className="day-head">
        <h4>{label}{date && <span className="muted small" style={{ fontWeight: 500 }}>{date}</span>}</h4>
        <span className="muted small">{entries.length ? plural(entries.length, "place") : ""}</span>
      </div>
      {entries.length === 0 && <div className="day-empty">{emptyText ?? "Drop places here."}</div>}
      {entries.map((e, i) => {
        const p = e.place;
        const done = p.personalStatus === "visited";
        return (
          <div key={e.id} className={`stop ${dragging === e.id ? "dragging" : ""} ${dropBefore === e.id ? "drop-before" : ""}`}
               draggable onDragStart={(ev) => { ev.dataTransfer.setData(DRAG, e.id); ev.dataTransfer.effectAllowed = "move"; setDragging(e.id); }}
               onDragEnd={() => setDragging(null)}
               onDragOver={(ev) => { if (accept(ev)) { ev.preventDefault(); setDropBefore(e.id); } }}
               onDrop={(ev) => drop(ev, e.id)}>
            <span className="stop-handle" aria-hidden="true" title="Drag to move"><GripVertical size={15} /></span>
            <span className={`stop-num ${done ? "done" : ""}`} aria-label={`Stop ${numbers.get(e.id)}`}>{done ? <Check size={12} strokeWidth={3} /> : numbers.get(e.id)}</span>
            <Thumb path={p.thumbnailPath ?? p.heroImagePath} focus={p.heroFocus} fallback={<CategoryBadge category={p.category} size={26} />} />
            <div className="stop-text" role="button" tabIndex={0} onClick={() => nav.openPlace(p.id)} onKeyDown={(ev) => ev.key === "Enter" && nav.openPlace(p.id)}>
              <span className="strong truncate">{p.canonicalName}</span>
              <span className="muted small truncate"><CategoryChip category={p.category} /> {[p.city, p.country].filter(Boolean).join(", ")}</span>
            </div>
            <div className="stop-actions">
              <button className="icon-btn" aria-label="Move up" title="Move up" disabled={i === 0} onClick={() => onApply(nudge(stops, e.id, -1))}><ArrowUp size={14} /></button>
              <button className="icon-btn" aria-label="Move down" title="Move down" disabled={i === entries.length - 1} onClick={() => onApply(nudge(stops, e.id, 1))}><ArrowDown size={14} /></button>
              <Menu trigger={<button className="icon-btn" aria-label={`Options for ${p.canonicalName}`}><MoreHorizontal size={15} /></button>}>
                {Array.from({ length: dayCount }, (_, k) => k + 1).filter((d) => d !== day).slice(0, 14).map((d) => (
                  <MenuItem key={d} icon={<CalendarDays size={14} />} onSelect={() => onApply(moveStop(stops, e.id, d))}>Move to Day {d}</MenuItem>
                ))}
                {day !== null && <MenuItem icon={<CalendarDays size={14} />} onSelect={() => onApply(moveStop(stops, e.id, null))}>Not scheduled</MenuItem>}
                <MenuSeparator />
                <MenuItem icon={<StatusIcon status="visited" size={14} />} onSelect={() => action.run(() => PlaceService.update(p.id, "personalStatus", done ? "wantToVisit" : "visited"))}>
                  {done ? "Mark as not visited" : "Mark as visited"}
                </MenuItem>
                <MenuItem icon={<Navigation size={14} />} onSelect={() => openUrl(`https://maps.apple.com/?ll=${p.latitude},${p.longitude}&q=${encodeURIComponent(p.canonicalName)}`)}>Open in Apple Maps</MenuItem>
                <MenuSeparator />
                <MenuItem icon={<X size={14} />} danger onSelect={() => action.run(() => TripService.removeEntry(e.id))}>Remove from trip</MenuItem>
              </Menu>
            </div>
          </div>
        );
      })}
    </section>
  );
}

/** The trip's places on a map, numbered in itinerary order (no routes are drawn — nothing is invented). */
function TripMap({ entries, numbers, onOpen }: { entries: TripEntry[]; numbers: Map<string, number>; onOpen: (p: Place) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | undefined>(undefined);
  const markers = useRef<maplibregl.Marker[]>([]);
  useEffect(() => {
    if (!ref.current) return;
    const map = new maplibregl.Map({ container: ref.current, style: resolveMapStyle().url, center: [0, 20], zoom: 1, attributionControl: { compact: true }, dragRotate: false });
    map.addControl(new maplibregl.NavigationControl({ showCompass: false }), "top-right");
    map.on("load", () => quietBasemap(map));
    mapRef.current = map;
    return () => map.remove();
  }, []);
  const key = entries.map((e) => `${e.id}:${e.day}:${e.place.personalStatus}`).join("|");
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    markers.current.forEach((m) => m.remove());
    markers.current = entries.map((e) => {
      const el = document.createElement("button");
      el.className = `trip-marker ${e.day == null ? "unscheduled" : ""}`;
      el.textContent = String(numbers.get(e.id) ?? "");
      el.title = e.place.canonicalName;
      el.setAttribute("aria-label", `${numbers.get(e.id)}. ${e.place.canonicalName}`);
      el.style.background = e.place.personalStatus === "visited" ? "var(--status-visited)" : e.day == null ? "" : colorOf(e.place.category);
      el.onclick = () => onOpen(e.place);
      return new maplibregl.Marker({ element: el }).setLngLat([e.place.longitude, e.place.latitude]).addTo(map);
    });
    if (entries.length) {
      const b = new maplibregl.LngLatBounds();
      entries.forEach((e) => b.extend([e.place.longitude, e.place.latitude]));
      map.fitBounds(b, { padding: 40, maxZoom: 13, duration: 0 });
    }
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps
  return <div ref={ref} className="trip-map" role="region" aria-label="Trip map" />;
}

function AddPlaces({ trip, existing, onClose }: { trip: Trip; existing: Set<string>; onClose: () => void }) {
  const [q, setQ] = useState("");
  const { data: places = [] } = useLoad(() => PlaceService.list({ search: q || undefined, sort: "name" }), [q]);
  const [added, setAdded] = useState<Set<string>>(new Set());
  const action = useAction();
  const shown = useMemo(() => places.filter((p) => !existing.has(p.id)).slice(0, 80), [places, existing]);
  return (
    <Modal title={`Add places to ${trip.name}`} onClose={onClose} wide>
      <label className="search-field" style={{ width: "100%" }}><Search size={15} /><input autoFocus placeholder="Search saved places (e.g. Kyoto)…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search saved places" /></label>
      <ErrorNote error={action.error} onClose={action.clearError} />
      <div className="candidate-list" style={{ maxHeight: "55vh", overflow: "auto" }}>
        {shown.length === 0 && <p className="muted small" style={{ padding: 8 }}>{q ? "No saved places match." : "Every saved place is already in this trip."}</p>}
        {shown.map((p) => (
          <div key={p.id} className="candidate">
            <CategoryBadge category={p.category} size={28} />
            <div className="grow"><div className="strong">{p.canonicalName}</div><PlaceLine place={p} /></div>
            {added.has(p.id) ? <span className="pill pill-ok"><Check size={11} /> Added</span>
              : <button className="btn small" onClick={async () => { if (await action.run(() => TripService.addPlace(trip.id, p.id)) !== undefined) setAdded((s) => new Set(s).add(p.id)); }}><Plus size={13} /> Add</button>}
          </div>
        ))}
      </div>
      <div className="modal-actions"><span className="muted small grow">{added.size ? `${plural(added.size, "place")} added` : ""}</span><button className="btn primary" onClick={onClose}>Done</button></div>
    </Modal>
  );
}

