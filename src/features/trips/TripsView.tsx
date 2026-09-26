import { useMemo, useState } from "react";
import { PlaceService, TripService } from "../../api/services";
import type { Trip, TripEntry } from "../../api/types";
import { CategoryChip, Empty, ErrorNote, Modal, PlaceLine, Thumb } from "../../components/common";
import { useAction, useLoad, useNav } from "../../lib/nav";

export function TripsView() {
  const { data: trips = [] } = useLoad(() => TripService.list(), []);
  const [openId, setOpenId] = useState<string>();
  const [name, setName] = useState("");
  const action = useAction();
  const open = trips.find((t) => t.id === openId);

  if (open) return <TripDetail trip={open} onBack={() => setOpenId(undefined)} />;
  return (
    <div className="page">
      <div className="page-head"><h2>Trips</h2></div>
      <form className="row" style={{ marginBottom: 16 }} onSubmit={async (e) => {
        e.preventDefault();
        if (!name.trim()) return;
        const id = await action.run(() => TripService.create(name.trim()));
        setName("");
        if (id) setOpenId(id);
      }}>
        <input placeholder="New trip, e.g. Japan 2027" value={name} onChange={(e) => setName(e.target.value)} style={{ width: 280 }} />
        <button className="btn primary">Create trip</button>
      </form>
      {trips.length === 0 ? <Empty icon="✈️" title="No trips yet">Group saved places into trips.</Empty> : (
        <div className="grid">
          {trips.map((t) => (
            <div key={t.id} className="tile" onClick={() => setOpenId(t.id)}>
              <div className="tile-body">
                <div className="tile-title">✈️ {t.name}</div>
                <span className="muted small">{t.placeCount} places{t.startDate ? ` · ${t.startDate}` : ""}</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function TripDetail({ trip, onBack }: { trip: Trip; onBack: () => void }) {
  const nav = useNav();
  const { data: entries = [] } = useLoad(() => TripService.entries(trip.id), [trip.id]);
  const { data: allWarnings = {} } = useLoad(() => PlaceService.warnings(), []);
  const tripWarnings = entries.filter((e) => allWarnings[e.place.id]).map((e) => ({ entry: e, warnings: allWarnings[e.place.id] }));
  const action = useAction();
  const [adding, setAdding] = useState(false);
  const [form, setForm] = useState({ name: trip.name, start: trip.startDate ?? "", end: trip.endDate ?? "", notes: trip.notes });

  const byDay = useMemo(() => {
    const g = new Map<string, TripEntry[]>();
    entries.forEach((e) => {
      const key = e.day == null ? "Unscheduled" : `Day ${e.day}`;
      g.set(key, [...(g.get(key) ?? []), e]);
    });
    return [...g.entries()];
  }, [entries]);
  const byCity = useMemo(() => {
    const m = new Map<string, number>();
    entries.forEach((e) => m.set(e.place.city ?? e.place.country ?? "Other", (m.get(e.place.city ?? e.place.country ?? "Other") ?? 0) + 1));
    return [...m.entries()].sort((a, b) => b[1] - a[1]);
  }, [entries]);

  const save = () => action.run(() => TripService.update(trip.id, form.name, form.start || null, form.end || null, form.notes));

  return (
    <div className="page">
      <div className="page-head">
        <button className="btn ghost" onClick={onBack}>← Trips</button>
        <input className="grow" style={{ fontSize: 20, fontWeight: 700, maxWidth: 420 }} value={form.name}
               onChange={(e) => setForm({ ...form, name: e.target.value })} onBlur={save} />
        <button className="btn primary" onClick={() => setAdding(true)}>＋ Add places</button>
        <button className="btn danger" onClick={async () => { await action.run(() => TripService.remove(trip.id)); onBack(); }}>Delete trip</button>
      </div>
      <div className="row wrap" style={{ marginBottom: 12 }}>
        <label className="row small">From <input type="date" value={form.start} onChange={(e) => setForm({ ...form, start: e.target.value })} onBlur={save} /></label>
        <label className="row small">To <input type="date" value={form.end} onChange={(e) => setForm({ ...form, end: e.target.value })} onBlur={save} /></label>
        {byCity.map(([city, n]) => <span key={city} className="chip-btn">{city} · {n}</span>)}
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />
      {tripWarnings.length > 0 && (
        <details className="warnings-panel trip-warnings" open={tripWarnings.length <= 3 || undefined}>
          <summary className="warnings-head">
            ⚠️ Check before you go <span className="muted small">— {tripWarnings.length} place{tripWarnings.length === 1 ? " has" : "s have"} out-of-date or conflicting saved info</span>
          </summary>
          {tripWarnings.map(({ entry, warnings }) => (
            <div key={entry.id} className="warning-row" onClick={() => nav.openPlace(entry.place.id)} role="button">
              <span className="warning-icon">{warnings.some((w) => w.kind === "conflict") ? "↔️" : "🕰️"}</span>
              <div>
                <div className="strong">{entry.place.canonicalName}</div>
                <div className="muted small">{warnings.map((w) => w.title).join(" · ")}</div>
              </div>
            </div>
          ))}
        </details>
      )}
      {entries.length === 0 ? <Empty icon="🧳" title="No places in this trip yet" /> : byDay.map(([day, items]) => (
        <div key={day} style={{ marginBottom: 16 }}>
          <h4 style={{ marginBottom: 6 }}>{day}</h4>
          <div className="list">
            {items.map((e) => (
              <div key={e.id} className="list-row">
                <Thumb path={e.place.heroImagePath ?? e.place.thumbnailPath} />
                <div className="grow" onClick={() => nav.openPlace(e.place.id)}>
                  <div className="strong">{e.place.canonicalName}{allWarnings[e.place.id] && <span className="warn-dot" title={allWarnings[e.place.id].map((w) => w.title).join("\n")}> ⚠️</span>}</div>
                  <PlaceLine place={e.place} />
                </div>
                <CategoryChip category={e.place.category} />
                <select value={e.day ?? ""} onChange={(ev) => action.run(() => TripService.updateEntry(e.id, ev.target.value ? Number(ev.target.value) : null, null))}>
                  <option value="">No day</option>
                  {Array.from({ length: 21 }, (_, i) => <option key={i + 1} value={i + 1}>Day {i + 1}</option>)}
                </select>
                <button className="icon-btn" title="Remove from trip" onClick={() => action.run(() => TripService.removeEntry(e.id))}>✕</button>
              </div>
            ))}
          </div>
        </div>
      ))}
      <textarea placeholder="Trip notes…" value={form.notes} onChange={(e) => setForm({ ...form, notes: e.target.value })} onBlur={save} />
      {adding && <AddPlaces trip={trip} existing={new Set(entries.map((e) => e.place.id))} onClose={() => setAdding(false)} />}
    </div>
  );
}

function AddPlaces({ trip, existing, onClose }: { trip: Trip; existing: Set<string>; onClose: () => void }) {
  const [q, setQ] = useState("");
  const { data: places = [] } = useLoad(() => PlaceService.list({ search: q || undefined, sort: "name" }), [q]);
  const action = useAction();
  return (
    <Modal title={`Add places to ${trip.name}`} onClose={onClose} wide>
      <input autoFocus placeholder="Search saved places (e.g. Kyoto)…" value={q} onChange={(e) => setQ(e.target.value)} />
      <div className="candidate-list">
        {places.filter((p) => !existing.has(p.id)).slice(0, 80).map((p) => (
          <div key={p.id} className="candidate">
            <div><div className="strong">{p.canonicalName}</div><PlaceLine place={p} /></div>
            <button className="btn" onClick={() => action.run(() => TripService.addPlace(trip.id, p.id))}>Add</button>
          </div>
        ))}
      </div>
    </Modal>
  );
}
