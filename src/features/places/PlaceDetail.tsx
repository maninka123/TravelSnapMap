import maplibregl from "maplibre-gl";
import { useEffect, useMemo, useRef, useState } from "react";
import { PlaceService, TripService } from "../../api/services";
import type { Fact, Place, PlaceDetail as Detail, PlaceWarning } from "../../api/types";
import { ErrorNote, Modal, PlaceLine, Pill, Thumb } from "../../components/common";
import { AddFact, FactRow } from "../../components/FactsEditor";
import { PlaceSearchDialog } from "../../components/PlacePicker";
import { CATEGORY, FACT, formatDate, formatKm, formatTime, INFO_CARDS, SOURCE, SOURCE_KIND, STATUS, TIME_SENSITIVE } from "../../lib/labels";
import { CategoryBadge } from "../../lib/categoryIcons";
import { useAction, useLoad, useNav } from "../../lib/nav";
import { quietBasemap } from "../map/MapView";
import { resolveMapStyle } from "../../lib/mapStyle";
import { MyVisit } from "./MyVisit";
import { openUrl } from "../../lib/open";

export function PlaceDetail({ id }: { id: string }) {
  const nav = useNav();
  const { data, error } = useLoad(() => PlaceService.detail(id), [id]);
  const action = useAction();
  const [dialog, setDialog] = useState<"location" | "merge" | "split" | "trip" | null>(null);

  if (error) return <p className="bad">{error}</p>;
  if (!data) return <p className="muted">Loading…</p>;
  const { place } = data;
  const hero = place.heroImagePath ?? place.thumbnailPath;

  return (
    <div>
      <div className="hero"><Thumb path={hero} fallback={<CategoryBadge category={place.category} size={72} />} /></div>

      <div className="detail-head">
        <EditableName place={place} onSave={(v) => action.run(() => PlaceService.update(place.id, "canonicalName", v))} />
        <PlaceLine place={place} />
        {place.alternativeNames.length > 0 && <span className="muted small">Also: {place.alternativeNames.join(" · ")}</span>}
        <div className="row wrap">
          <select value={place.personalStatus} onChange={(e) => action.run(() => PlaceService.update(place.id, "personalStatus", e.target.value))}>
            {Object.entries(STATUS).map(([k, s]) => <option key={k} value={k}>{s.emoji} {s.label}</option>)}
          </select>
          <select value={place.category} onChange={(e) => action.run(() => PlaceService.update(place.id, "category", e.target.value))}>
            {Object.entries(CATEGORY).map(([k, c]) => <option key={k} value={k}>{c.label}</option>)}
          </select>
          {place.verification === "needsReview" && <Pill tone="warn">Location not confirmed</Pill>}
          {place.isUserVerified && <Pill tone="ok">Verified by you</Pill>}
          <span className="spacer" />
          <button className="btn" onClick={() => setDialog("trip")}>✈️ Add to trip</button>
          <button className="btn" title="Open in Apple Maps"
                  onClick={() => openUrl(`https://maps.apple.com/?ll=${place.latitude},${place.longitude}&q=${encodeURIComponent(place.canonicalName)}`)}>🧭 Apple Maps</button>
          <details className="menu">
            <summary className="btn">More ▾</summary>
            <div className="menu-items card">
              <button className="btn ghost" onClick={() => setDialog("location")}>Correct location</button>
              <button className="btn ghost" onClick={() => setDialog("merge")}>Merge into another place…</button>
              <button className="btn ghost" disabled={data.screenshots.length + data.reels.length < 2} onClick={() => setDialog("split")}>Split sources into a new place…</button>
              <button className="btn ghost danger" onClick={async () => {
                if (await action.run(() => PlaceService.remove(place.id)) !== undefined) nav.back();
              }}>Delete place</button>
            </div>
          </details>
        </div>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />

      <Warnings warnings={data.warnings} />

      <MiniMap place={place} />

      <WhySaved detail={data} />

      <div className="section"><h3>My visit</h3>{place.visitedAt && <span className="muted small">{formatDate(place.visitedAt)}</span>}</div>
      <MyVisit place={place} memories={data.memories} />

      <div className="section"><h3>Saved information</h3></div>
      <div className="disclaimer">Saved from your screenshots and Reels — it may be out of date, and nothing here comes from an external database.</div>
      <div className="add-fact-bar"><AddFact places={[place]} /></div>
      <InfoCards facts={data.facts} nearby={data.nearby} warned={new Set(data.warnings.flatMap((w) => w.factIds))} />

      {data.images.length > 0 && (
        <>
          <div className="section"><h3>Photos from your sources</h3><span className="muted small">{data.images.filter((i) => i.isAccepted).length}</span></div>
          <div className="grid small-tiles">
            {data.images.filter((i) => i.isAccepted).map((img) => (
              <div key={img.id} className="tile">
                <Thumb path={img.imagePath} />
                <div className="tile-body row">
                  {place.heroImageId === img.id ? <Pill tone="ok">Cover</Pill> :
                    <button className="btn small" onClick={() => action.run(() => PlaceService.update(place.id, "heroImageId", img.id))}>Set cover</button>}
                  <button className="btn small danger" onClick={() => action.run(() => PlaceService.removeImage(img.id))}>Remove</button>
                </div>
              </div>
            ))}
          </div>
        </>
      )}

      <div className="section"><h3>Sources</h3><span className="muted small">{data.screenshots.length + data.reels.length}</span></div>
      <div className="grid small-tiles">
        {data.reels.map((r) => (
          <div key={r.id} className="tile portrait" onClick={() => nav.openReel(r.id)}>
            <Thumb path={r.thumbnailPath} fallback="🎬" />
            <div className="tile-body">
              <span className="small strong">🎬 Reel {r.creator ?? ""}</span>
              <span className="muted small">{formatDate(r.createdAt)}</span>
            </div>
          </div>
        ))}
        {data.screenshots.map((s) => (
          <div key={s.id} className="tile portrait" onClick={() => nav.openScreenshot(s.id)}>
            <Thumb path={s.thumbnailPath} />
            <div className="tile-body">
              <span className="small">{formatDate(s.creationDate)}</span>
              <div className="row">
                <span className="muted small grow">{SOURCE[s.sourceType]}</span>
                <button className="icon-btn small" title="Remove this screenshot from the place"
                        onClick={(e) => { e.stopPropagation(); void action.run(() => PlaceService.removeScreenshot(place.id, s.id)); }}>✕</button>
              </div>
            </div>
          </div>
        ))}
      </div>

      {data.trips.length > 0 && (
        <>
          <div className="section"><h3>In trips</h3></div>
          <div className="row wrap">{data.trips.map((t) => <span key={t.id} className="chip-btn">✈️ {t.name}</span>)}</div>
        </>
      )}

      <div className="section"><h3>Your notes</h3></div>
      <Notes place={place} onSave={(v) => action.run(() => PlaceService.update(place.id, "notes", v))} />

      {dialog === "location" && (
        <PlaceSearchDialog title="Correct location" includeSaved={false} initialQuery={[place.canonicalName, place.city, place.country].filter(Boolean).join(", ")}
          onClose={() => setDialog(null)}
          onPick={async (c) => { setDialog(null); await action.run(() => PlaceService.setLocation(place.id, c)); }} />
      )}
      {dialog === "merge" && <MergeDialog place={place} onClose={() => setDialog(null)} onMerged={(target) => { setDialog(null); nav.back(); nav.openPlace(target); }} />}
      {dialog === "split" && <SplitDialog detail={data} onClose={() => setDialog(null)} />}
      {dialog === "trip" && <AddToTripDialog placeId={place.id} onClose={() => setDialog(null)} />}
    </div>
  );
}

/** Freshness and disagreement warnings, shown before the details so they're seen when planning. */
function Warnings({ warnings }: { warnings: PlaceWarning[] }) {
  if (warnings.length === 0) return null;
  return (
    <div className="warnings-panel">
      <div className="warnings-head">⚠️ Before you go <span className="muted small">— check these, your saved info may be out of date</span></div>
      {warnings.map((w, i) => (
        <div key={i} className={`warning-row warning-${w.kind}`}>
          <span className="warning-icon">{w.kind === "stale" ? "🕰️" : "↔️"}</span>
          <div>
            <div className="strong">{w.title}</div>
            <div className="muted small">{w.detail}</div>
          </div>
        </div>
      ))}
    </div>
  );
}

function EditableName({ place, onSave }: { place: Place; onSave: (v: string) => void }) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(place.canonicalName);
  if (!editing) return <h2 onDoubleClick={() => setEditing(true)} title="Double-click to rename">{place.canonicalName}</h2>;
  return (
    <form className="row" onSubmit={(e) => { e.preventDefault(); setEditing(false); if (value.trim()) onSave(value.trim()); }}>
      <input autoFocus className="grow" value={value} onChange={(e) => setValue(e.target.value)} onBlur={() => setEditing(false)} />
    </form>
  );
}

function Notes({ place, onSave }: { place: Place; onSave: (v: string) => void }) {
  const [value, setValue] = useState(place.notes);
  return <textarea value={value} placeholder="Your own notes…" onChange={(e) => setValue(e.target.value)} onBlur={() => value !== place.notes && onSave(value)} />;
}

function MiniMap({ place }: { place: Place }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    const map = new maplibregl.Map({ container: ref.current, style: resolveMapStyle().url, center: [place.longitude, place.latitude], zoom: 13, interactive: true, attributionControl: false });
    new maplibregl.Marker({ color: CATEGORY[place.category]?.color }).setLngLat([place.longitude, place.latitude]).addTo(map);
    map.on("load", () => quietBasemap(map));
    return () => map.remove();
  }, [place.latitude, place.longitude, place.category]);
  return (
    <div>
      <div ref={ref} className="mini-map" />
      {place.address && <div className="muted small" style={{ marginTop: 4 }}>{place.address}</div>}
    </div>
  );
}

function WhySaved({ detail }: { detail: Detail }) {
  const action = useAction();
  const { place } = detail;
  if (detail.facts.length === 0) return null;
  return (
    <>
      <div className="section">
        <h3>Why did I save this?</h3>
        <button className="btn small" disabled={action.busy} onClick={() => action.run(() => PlaceService.summarize(place.id))}>
          {action.busy ? "Summarising…" : place.summaryText ? "Refresh" : "Summarise"}
        </button>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />
      {place.summaryText ? (
        <div className="summary">
          {place.summaryText}
          <div className="muted small" style={{ marginTop: 6 }}>Based only on {place.summaryFactIds.length || detail.facts.length} saved facts · {formatDate(place.summaryGeneratedAt)}</div>
        </div>
      ) : <p className="muted small">A short summary written only from your saved screenshots and Reels.</p>}
    </>
  );
}

/** Groups facts into cards; contradictions stay side by side, each with its own source. */
function InfoCards({ facts, nearby, warned }: { facts: Fact[]; nearby: Detail["nearby"]; warned: Set<string> }) {
  const nav = useNav();
  const cards = useMemo(() => INFO_CARDS.map((card) => ({
    ...card,
    facts: facts.filter((f) => card.types.includes(f.type))
      .sort((a, b) => (b.validFrom ?? b.createdAt).localeCompare(a.validFrom ?? a.createdAt)),
  })), [facts]);

  const visible = cards.filter((c) => c.facts.length > 0 || (c.key === "nearby" && nearby.length > 0));
  if (visible.length === 0) return <p className="muted">No travel information was found in the sources yet.</p>;

  return (
    <div className="info-cards">
      {visible.map((card) => (
        <div key={card.key} className="info-card">
          <h4 className="row"><span className="grow">{card.title}</span>
            {sourceCount(card.facts) > 1 && <span className="muted small" title="Each source is kept separately; different advice is not merged">from {sourceCount(card.facts)} sources</span>}</h4>
          {card.facts.map((f, i) => {
            // For values that change (prices, hours), label the newest one.
            const sameTypeEarlier = TIME_SENSITIVE.has(f.type) && card.facts.slice(0, i).some((o) => o.type === f.type);
            return (
              <FactRow key={f.id} fact={f} source={<FactSource fact={f} />} extra={<>
                {TIME_SENSITIVE.has(f.type) && card.facts.filter((o) => o.type === f.type).length > 1 &&
                  <><Pill tone={sameTypeEarlier ? "muted" : "ok"}>{sameTypeEarlier ? "Earlier" : "Latest saved"}</Pill>{" "}</>}
                {warned.has(f.id) && <span title="See “Before you go” above">⚠️ </span>}
              </>} />
            );
          })}
          {card.key === "nearby" && nearby.map((n) => (
            <div key={n.place.id} className="fact">
              <button className="btn ghost small" onClick={() => nav.openPlace(n.place.id)}>
                <CategoryBadge category={n.place.category} size={20} /> {n.place.canonicalName}
              </button>
              <span className="muted small">{formatKm(n.distanceKm)} · saved</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

export function FactSource({ fact }: { fact: Fact }) {
  const nav = useNav();
  const kind = SOURCE_KIND[fact.sourceKind] ?? "Source";
  if (fact.reelId) {
    return (
      <div className="fact-source">
        <span>🎬 Instagram Reel</span>
        {fact.creator && <span>{fact.creator}</span>}
        <span>· {kind}{fact.sourceTimeSec != null ? ` · ${formatTime(fact.sourceTimeSec)}` : ""}</span>
        <button onClick={() => nav.openReel(fact.reelId!, fact.sourceTimeSec ?? undefined)}>Source 🔎</button>
      </div>
    );
  }
  return (
    <div className="fact-source">
      <span>{FACT[fact.type]?.emoji} {kind}</span>
      <span>· {formatDate(fact.validFrom)}</span>
      {fact.sourceType && fact.sourceType !== "unknown" && <span>· {SOURCE[fact.sourceType]}</span>}
      {fact.creator && <span>· {fact.creator}</span>}
      {fact.screenshotId && <button onClick={() => nav.openScreenshot(fact.screenshotId!, fact.sourceBlockIds)}>Source 🔎</button>}
    </div>
  );
}

function MergeDialog({ place, onClose, onMerged }: { place: Place; onClose: () => void; onMerged: (targetId: string) => void }) {
  const [q, setQ] = useState("");
  const { data: places = [] } = useLoad(() => PlaceService.list({ search: q || undefined }), [q]);
  const action = useAction();
  return (
    <Modal title={`Merge "${place.canonicalName}" into…`} onClose={onClose}>
      <p className="muted small">All sources, facts and photos move to the chosen place. This place is then removed.</p>
      <input autoFocus placeholder="Search saved places…" value={q} onChange={(e) => setQ(e.target.value)} />
      <ErrorNote error={action.error} />
      <div className="candidate-list">
        {places.filter((p) => p.id !== place.id).slice(0, 30).map((p) => (
          <div key={p.id} className="candidate">
            <div><div className="strong">{p.canonicalName}</div><PlaceLine place={p} /></div>
            <button className="btn" onClick={async () => {
              if (await action.run(() => PlaceService.merge(place.id, p.id)) !== undefined) onMerged(p.id);
            }}>Merge</button>
          </div>
        ))}
      </div>
    </Modal>
  );
}

function SplitDialog({ detail, onClose }: { detail: Detail; onClose: () => void }) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [pick, setPick] = useState(false);
  const action = useAction();
  const toggle = (id: string) => setSelected((s) => { const n = new Set(s); n.has(id) ? n.delete(id) : n.add(id); return n; });
  if (pick) {
    return <PlaceSearchDialog title="Where are the selected sources?" onClose={onClose}
      onPick={async (c) => { await action.run(() => PlaceService.split(detail.place.id, [...selected], c)); onClose(); }} />;
  }
  return (
    <Modal title="Split into a new place" onClose={onClose} wide>
      <p className="muted small">Select the screenshots that actually show a different place.</p>
      <div className="grid small-tiles">
        {detail.screenshots.map((s) => (
          <div key={s.id} className={`tile portrait ${selected.has(s.id) ? "selected" : ""}`} onClick={() => toggle(s.id)}
               style={selected.has(s.id) ? { outline: "3px solid var(--accent)" } : undefined}>
            <Thumb path={s.thumbnailPath} />
            <div className="tile-body small">{formatDate(s.creationDate)}</div>
          </div>
        ))}
      </div>
      <button className="btn primary" disabled={selected.size === 0} onClick={() => setPick(true)}>Choose new location ({selected.size})</button>
    </Modal>
  );
}

function AddToTripDialog({ placeId, onClose }: { placeId: string; onClose: () => void }) {
  const { data: trips = [], reload } = useLoad(() => TripService.list(), []);
  const [name, setName] = useState("");
  const action = useAction();
  return (
    <Modal title="Add to trip" onClose={onClose}>
      {trips.map((t) => (
        <div key={t.id} className="candidate">
          <span>✈️ {t.name} <span className="muted small">· {t.placeCount} places</span></span>
          <button className="btn" onClick={async () => { await action.run(() => TripService.addPlace(t.id, placeId)); onClose(); }}>Add</button>
        </div>
      ))}
      <form className="row" onSubmit={async (e) => {
        e.preventDefault();
        if (!name.trim()) return;
        const id = await action.run(() => TripService.create(name.trim()));
        if (id) { await action.run(() => TripService.addPlace(id, placeId)); reload(); onClose(); }
      }}>
        <input className="grow" placeholder="New trip, e.g. Japan 2027" value={name} onChange={(e) => setName(e.target.value)} />
        <button className="btn primary">Create & add</button>
      </form>
    </Modal>
  );
}

function sourceCount(facts: Fact[]): number {
  return new Set(facts.map((f) => f.reelId ?? f.screenshotId ?? f.id)).size;
}
