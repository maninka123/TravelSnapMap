import { useEffect, useMemo, useState } from "react";
import { fileUrl, MemoryService, PlaceService } from "../../api/services";
import type { Memory, OwnPhoto, Place } from "../../api/types";
import { ErrorNote, Modal, Pill, Thumb } from "../../components/common";
import { formatDate } from "../../lib/labels";
import { useAction } from "../../lib/nav";

/**
 * After the trip: when you went, your own notes and your own photos from Photos (originals stay in Photos;
 * a copy is kept for the app). Any of your photos can become the place's cover.
 */
export function MyVisit({ place, memories }: { place: Place; memories: Memory[] }) {
  const action = useAction();
  const [picking, setPicking] = useState(false);
  const [notes, setNotes] = useState(place.visitNotes);
  useEffect(() => setNotes(place.visitNotes), [place.id, place.visitNotes]);
  const visited = place.personalStatus === "visited" || place.personalStatus === "favourite" || memories.length > 0 || !!place.visitedAt;

  if (!visited) {
    return (
      <div className="visit-invite">
        <div className="grow">
          <div className="strong">Been here?</div>
          <div className="muted small">Mark it visited, add the date, your own notes and your photos from Photos — your wishlist becomes a travel memory map.</div>
        </div>
        <button className="btn" onClick={() => action.run(() => PlaceService.update(place.id, "personalStatus", "visited"))}>✅ I've been here</button>
        <button className="btn primary" onClick={() => setPicking(true)}>📷 Add my photos…</button>
        {picking && <MemoryPicker place={place} onClose={() => setPicking(false)} />}
      </div>
    );
  }

  return (
    <div className="my-visit">
      <div className="row wrap">
        <label className="row small">Visited on
          <input type="date" value={place.visitedAt ?? ""} onChange={(e) => action.run(() => PlaceService.update(place.id, "visitedAt", e.target.value || null))} />
        </label>
        <span className="grow" />
        {place.coverMemoryId && (
          <button className="btn small" onClick={() => action.run(() => PlaceService.update(place.id, "coverMemoryId", null))}>Use saved photo as cover</button>
        )}
        <button className="btn primary small" onClick={() => setPicking(true)}>📷 Add my photos…</button>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />
      {memories.length > 0 ? (
        <div className="memory-grid">
          {memories.map((m) => (
            <figure key={m.id} className={`memory ${place.coverMemoryId === m.id ? "is-cover" : ""}`}>
              <Thumb path={m.thumbnailPath ?? m.imagePath} />
              <figcaption>
                <span className="small">{formatDate(m.takenAt)}</span>
                <span className="memory-actions">
                  {place.coverMemoryId === m.id ? <Pill tone="ok">Cover</Pill> : (
                    <button className="btn small" onClick={() => action.run(() => PlaceService.update(place.id, "coverMemoryId", m.id))}>Set cover</button>
                  )}
                  <button className="icon-btn small" title="Remove from this place (stays in Photos)" onClick={() => action.run(() => MemoryService.remove(m.id))}>✕</button>
                </span>
              </figcaption>
            </figure>
          ))}
        </div>
      ) : <p className="muted small">No photos of your own yet. Add the ones you took here from Photos.</p>}
      <textarea placeholder="Your memories — what it was like, what you'd tell a friend…" value={notes}
                onChange={(e) => setNotes(e.target.value)}
                onBlur={() => notes !== place.visitNotes && action.run(() => PlaceService.update(place.id, "visitNotes", notes))} />
      {picking && <MemoryPicker place={place} onClose={() => setPicking(false)} />}
    </div>
  );
}

type Mode = "near" | "dates";

/** Choose your own photos: those taken near the place (by GPS) or on the days you were there. */
function MemoryPicker({ place, onClose }: { place: Place; onClose: () => void }) {
  const [mode, setMode] = useState<Mode>("near");
  // A whole city/region needs a wider circle than a single spot.
  const [radius, setRadius] = useState(place.category === "city" || place.category === "region" ? 25 : place.category === "nature" || place.category === "hiking" ? 5 : 1);
  const today = new Date().toISOString().slice(0, 10);
  const [from, setFrom] = useState(place.visitedAt ?? today);
  const [to, setTo] = useState(place.visitedAt ?? today);
  const [photos, setPhotos] = useState<OwnPhoto[]>();
  const [previews, setPreviews] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [shown, setShown] = useState(48);
  const load = useAction();
  const attach = useAction();

  useEffect(() => {
    let alive = true;
    setPhotos(undefined);
    setSelected(new Set());
    setShown(48);
    void load.run(() => (mode === "near" ? MemoryService.near(place.id, radius) : MemoryService.between(from, to)))
      .then((p) => { if (alive) setPhotos(p ?? []); });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode, radius, from, to, place.id]);

  // Previews for the visible photos, in small batches.
  useEffect(() => {
    if (!photos) return;
    const missing = photos.slice(0, shown).map((p) => p.id).filter((id) => !(id in previews));
    if (missing.length === 0) return;
    let alive = true;
    (async () => {
      for (let i = 0; i < missing.length && alive; i += 12) {
        const got = await MemoryService.previews(missing.slice(i, i + 12)).catch(() => ({}));
        if (alive) setPreviews((p) => ({ ...p, ...got }));
      }
    })();
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [photos, shown]);

  const byDay = useMemo(() => {
    const g = new Map<string, OwnPhoto[]>();
    photos?.slice(0, shown).forEach((p) => {
      const day = p.creationDate?.slice(0, 10) ?? "Unknown date";
      g.set(day, [...(g.get(day) ?? []), p]);
    });
    return [...g.entries()];
  }, [photos, shown]);

  const toggle = (id: string) => setSelected((s) => { const n = new Set(s); if (n.has(id)) n.delete(id); else n.add(id); return n; });
  const toggleDay = (items: OwnPhoto[]) => setSelected((s) => {
    const n = new Set(s);
    const all = items.every((p) => n.has(p.id));
    items.forEach((p) => (all ? n.delete(p.id) : n.add(p.id)));
    return n;
  });

  const submit = async () => {
    const chosen = (photos ?? []).filter((p) => selected.has(p.id));
    if (await attach.run(() => MemoryService.attach(place.id, chosen)) !== undefined) onClose();
  };

  return (
    <Modal title={`Your photos from ${place.canonicalName}`} onClose={onClose} wide>
      <div className="row wrap">
        <div className="segmented">
          <button className={mode === "near" ? "active" : ""} onClick={() => setMode("near")}>📍 Taken here</button>
          <button className={mode === "dates" ? "active" : ""} onClick={() => setMode("dates")}>📅 On certain days</button>
        </div>
        {mode === "near" ? (
          <select value={radius} onChange={(e) => setRadius(Number(e.target.value))}>
            <option value={0.3}>Within 300 m</option>
            <option value={1}>Within 1 km</option>
            <option value={5}>Within 5 km</option>
            <option value={25}>Within 25 km</option>
          </select>
        ) : (
          <>
            <input type="date" value={from} max={to} onChange={(e) => setFrom(e.target.value)} />
            <span className="muted small">to</span>
            <input type="date" value={to} min={from} onChange={(e) => setTo(e.target.value)} />
          </>
        )}
        <span className="grow" />
        <span className="muted small">{photos ? `${photos.length} photo${photos.length === 1 ? "" : "s"}` : "Looking in Photos…"}</span>
      </div>
      <p className="muted small">Only photos you choose are copied into TravelSnapMap; the originals stay in Photos. Screenshots are never shown here.</p>
      <ErrorNote error={load.error ?? attach.error} />
      {photos?.length === 0 && (
        <p className="muted">{mode === "near" ? "No photos with a location near this place. Try a wider distance, or pick the days you were there." : "No photos on those days."}</p>
      )}
      <div className="memory-picker">
        {byDay.map(([day, items]) => (
          <section key={day}>
            <header className="row">
              <h4 className="grow">{day === "Unknown date" ? day : formatDate(day)}</h4>
              <button className="btn ghost small" onClick={() => toggleDay(items)}>
                {items.every((p) => selected.has(p.id)) ? "Deselect day" : "Select day"}
              </button>
            </header>
            <div className="memory-picker-grid">
              {items.map((p) => (
                <button key={p.id} className={`memory-pick ${selected.has(p.id) ? "selected" : ""}`} onClick={() => toggle(p.id)}>
                  {previews[p.id] ? <img src={fileUrl(previews[p.id])} alt="" loading="lazy" /> : <span className="memory-pick-loading" />}
                  {selected.has(p.id) && <span className="memory-check">✓</span>}
                </button>
              ))}
            </div>
          </section>
        ))}
        {photos && photos.length > shown && <button className="btn" onClick={() => setShown((n) => n + 48)}>Show more</button>}
      </div>
      <div className="row">
        <span className="grow" />
        <button className="btn" onClick={onClose}>Cancel</button>
        <button className="btn primary" disabled={selected.size === 0 || attach.busy} onClick={submit}>
          {attach.busy ? "Copying from Photos…" : `Add ${selected.size || ""} photo${selected.size === 1 ? "" : "s"}`}
        </button>
      </div>
    </Modal>
  );
}
