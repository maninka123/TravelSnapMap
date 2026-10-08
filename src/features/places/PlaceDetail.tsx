import maplibregl from "maplibre-gl";
import {
  AlertTriangle, ArrowLeftRight, BadgeCheck, Clapperboard, CopyPlus, Crosshair, ExternalLink, History, Luggage, Map as MapIcon, MoreHorizontal,
  Navigation, Plus, Scissors, Search, Smartphone, Trash2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { PlaceService, ReelService, ScreenshotService, TripService } from "../../api/services";
import type { Fact, Place, PlaceDetail as Detail, PlaceWarning } from "../../api/types";
import { ErrorNote, Modal, PlaceLine, Pill, Thumb } from "../../components/common";
import { Menu, MenuItem, MenuSeparator } from "../../components/ui";
import { FACT_ICON, INFO_CARD_ICON, StatusIcon } from "../../lib/icons";
import { AddFact, FactRow } from "../../components/FactsEditor";
import { PlaceSearchDialog } from "../../components/PlacePicker";
import { CategoryPicker } from "../../components/CategoryPicker";
import { colorOf } from "../../lib/categoryIcons";
import { formatDate, formatKm, formatTime, INFO_CARDS, SOURCE, SOURCE_KIND, STATUS, TIME_SENSITIVE } from "../../lib/labels";
import { CategoryBadge } from "../../lib/categoryIcons";
import { useAction, useLoad, useNav } from "../../lib/nav";
import { quietBasemap } from "../map/MapView";
import { resolveMapStyle } from "../../lib/mapStyle";
import { MyVisit } from "./MyVisit";
import { PlacePhotos } from "./PlacePhotos";
import { HeroPhoto } from "./HeroPhoto";
import { openUrl } from "../../lib/open";

export function PlaceDetail({ id }: { id: string }) {
  const nav = useNav();
  const { data, error } = useLoad(() => PlaceService.detail(id), [id]);
  const action = useAction();
  const [dialog, setDialog] = useState<"location" | "merge" | "split" | "trip" | "remove" | null>(null);

  // Move one source to Not travel; if that was the place's only support, the place is gone — go back.
  const notTravel = async (run: () => Promise<void>) => {
    if (await action.run(run) === undefined) return;
    const stillThere = await PlaceService.detail(id).then(() => true, () => false);
    if (!stillThere) nav.advanceAfterRemoval();
  };

  // The place was removed (here or elsewhere): move on instead of showing "Place not found".
  const gone = !!error && /not found/i.test(error);
  useEffect(() => { if (gone) nav.advanceAfterRemoval(); }, [gone]); // eslint-disable-line react-hooks/exhaustive-deps
  if (gone) return null;
  if (error) return <p className="bad">{error}</p>;
  if (!data) return <p className="muted">Loading…</p>;
  const { place } = data;
  const hero = place.heroImagePath ?? place.thumbnailPath;

  return (
    <div>
      <HeroPhoto place={place} path={hero} />

      <div className="detail-head">
        <EditableName place={place} onSave={(v) => action.run(() => PlaceService.update(place.id, "canonicalName", v))} />
        <div className="row wrap" style={{ gap: 10 }}>
          <PlaceLine place={place} />
          {place.verification === "needsReview" ? <Pill tone="warn">Location not confirmed</Pill>
            : place.isUserVerified ? <span className="verify-badge ok-text"><BadgeCheck size={14} /> Checked by you</span>
            : <span className="verify-badge muted" title="Located with Apple Maps"><BadgeCheck size={14} /> Apple Maps match</span>}
        </div>
        {place.alternativeNames.length > 0 && <span className="muted small">Also known as {place.alternativeNames.join(" · ")}</span>}
        <div className="detail-actions">
          <label className={`status-tag status-${place.personalStatus}`} style={{ paddingRight: 2, fontSize: 13 }}>
            <StatusIcon status={place.personalStatus} size={13} />
            <select className="status-select" aria-label="Status" value={place.personalStatus} style={{ background: "transparent", color: "inherit" }}
                    onChange={(e) => action.run(() => PlaceService.update(place.id, "personalStatus", e.target.value))}>
              {Object.entries(STATUS).map(([k, s]) => <option key={k} value={k}>{s.label}</option>)}
            </select>
          </label>
          <CategoryPicker value={place.category} onChange={(c) => c && action.run(() => PlaceService.update(place.id, "category", c))} />
          <span className="spacer" />
          <button className="btn" onClick={() => setDialog("trip")}><Luggage size={14} /> Add to trip</button>
          <button className="btn" onClick={() => nav.go("explore", { explore: "map", focusPlaceId: place.id })}><MapIcon size={14} /> Show on map</button>
          <Menu trigger={<button className="btn icon-only" aria-label="More actions"><MoreHorizontal size={16} /></button>}>
            <MenuItem icon={<Navigation size={14} />} onSelect={() => openUrl(`https://maps.apple.com/?ll=${place.latitude},${place.longitude}&q=${encodeURIComponent(place.canonicalName)}`)}>Open in Apple Maps</MenuItem>
            <MenuSeparator />
            <MenuItem icon={<Crosshair size={14} />} onSelect={() => setDialog("location")}>Correct location…</MenuItem>
            <MenuItem icon={<CopyPlus size={14} />} onSelect={() => setDialog("merge")}>Merge into another place…</MenuItem>
            <MenuItem icon={<Scissors size={14} />} disabled={data.screenshots.length + data.reels.length < 2} onSelect={() => setDialog("split")}>Split sources into a new place…</MenuItem>
            <MenuSeparator />
            <MenuItem icon={<Trash2 size={14} />} danger onSelect={() => setDialog("remove")}>Remove pin…</MenuItem>
          </Menu>
        </div>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />

      <div className="place-layout">
        <div>
          <Warnings warnings={data.warnings} />
          <WhySaved detail={data} />

          <div className="section"><h3>Saved information</h3><AddFact places={[place]} /></div>
          <p className="disclaimer" style={{ marginBottom: 12 }}>From your screenshots and Reels — it may be out of date. Nothing here comes from an outside database, and different advice is kept side by side.</p>
          <InfoCards facts={data.facts} nearby={data.nearby} warned={new Set(data.warnings.flatMap((w) => w.factIds))} />

          <PlacePhotos place={place} images={data.images} screenshots={data.screenshots} reels={data.reels}
            onNotTravel={(kind, sid) => void notTravel(() => (kind === "reel" ? ReelService.action(sid, "markNotTravel") : ScreenshotService.action(sid, "markNotTravel")))} />

          <div className="section"><h3>My visit</h3>{place.visitedAt && <span className="muted small">{formatDate(place.visitedAt)}</span>}</div>
          <MyVisit place={place} memories={data.memories} />
        </div>
        <aside className="place-side">
          <MiniMap place={place} />
          <div className="side-card">
            <h5>Your notes</h5>
            <Notes place={place} onSave={(v) => action.run(() => PlaceService.update(place.id, "notes", v))} />
          </div>
          {data.trips.length > 0 && (
            <div className="side-card">
              <h5>In trips</h5>
              <div className="row wrap" style={{ gap: 6 }}>{data.trips.map((t) => (
                <button key={t.id} className="chip-btn" onClick={() => nav.go("trips", { tripId: t.id })}><Luggage size={12} /> {t.name}</button>
              ))}</div>
            </div>
          )}
          <div className="side-card">
            <h5>Sources</h5>
            <span className="small">{data.screenshots.length} screenshot{data.screenshots.length === 1 ? "" : "s"} · {data.reels.length} Reel{data.reels.length === 1 ? "" : "s"}</span>
            <span className="muted small">Saved {formatDate(place.createdAt)}</span>
          </div>
        </aside>
      </div>

      {dialog === "location" && (
        <PlaceSearchDialog title="Correct location" includeSaved={false} initialQuery={[place.canonicalName, place.city, place.country].filter(Boolean).join(", ")}
          onClose={() => setDialog(null)}
          onPick={async (c) => {
            setDialog(null);
            const shown = await action.run(() => PlaceService.setLocation(place.id, c));
            // Merged into a place you already had there: show that one instead.
            if (shown && shown !== place.id) { nav.back(); nav.openPlace(shown); }
          }} />
      )}
      {dialog === "merge" && <MergeDialog place={place} onClose={() => setDialog(null)} onMerged={(target) => { setDialog(null); nav.back(); nav.openPlace(target); }} />}
      {dialog === "split" && <SplitDialog detail={data} onClose={() => setDialog(null)} />}
      {dialog === "trip" && <AddToTripDialog placeId={place.id} onClose={() => setDialog(null)} />}
      {dialog === "remove" && <RemovePinDialog detail={data} onClose={() => setDialog(null)} onRemoved={() => { setDialog(null); nav.advanceAfterRemoval(); }} />}
    </div>
  );
}

/** Freshness and disagreement warnings, shown before the details so they're seen when planning. */
function Warnings({ warnings }: { warnings: PlaceWarning[] }) {
  if (warnings.length === 0) return null;
  return (
    <div className="warnings-panel">
      <div className="warnings-head"><AlertTriangle size={15} /> Before you go <span className="muted small" style={{ fontWeight: 400 }}>— your saved info may be out of date</span></div>
      {warnings.map((w, i) => (
        <div key={i} className={`warning-row warning-${w.kind}`}>
          {w.kind === "stale" ? <History size={15} /> : <ArrowLeftRight size={15} />}
          <div>
            <div className="strong">{w.title}</div>
            <div className="muted small">{w.detail}</div>
          </div>
        </div>
      ))}
    </div>
  );
}

/** Remove a place from the map, saying plainly what happens to its screenshots and Reels. */
function RemovePinDialog({ detail, onClose, onRemoved }: { detail: Detail; onClose: () => void; onRemoved: () => void }) {
  const { place, screenshots, reels, facts } = detail;
  const sources = screenshots.length + reels.length;
  const [alsoNotTravel, setAlsoNotTravel] = useState(false);
  const action = useAction();
  const what = [screenshots.length && `${screenshots.length} screenshot${screenshots.length === 1 ? "" : "s"}`, reels.length && `${reels.length} Reel${reels.length === 1 ? "" : "s"}`].filter(Boolean).join(" and ");

  const remove = async () => {
    const done = await action.run(async () => {
      if (alsoNotTravel) {
        for (const s of screenshots) await ScreenshotService.action(s.id, "markNotTravel");
        for (const r of reels) await ReelService.action(r.id, "markNotTravel");
      }
      // Marking the sources may already have removed it (it was their only place); remove it if it's still there.
      if (await PlaceService.detail(place.id).then(() => true, () => false)) await PlaceService.remove(place.id);
    });
    if (done !== undefined) onRemoved();
  };

  return (
    <Modal title={`Remove “${place.canonicalName}” from your map?`} onClose={onClose}>
      <p className="muted">
        The pin, its {facts.length} tip{facts.length === 1 ? "" : "s"}, photos and your notes are removed.
        {sources > 0 ? ` Its ${what} stay in Sources.` : ""}
      </p>
      {sources > 0 && (
        <label className="check">
          <input type="checkbox" checked={alsoNotTravel} onChange={(e) => setAlsoNotTravel(e.target.checked)} />
          <span>Also move {sources === 1 ? "it" : `those ${what}`} to Not travel
            <br /><span className="muted small">So {sources === 1 ? "it isn't" : "they aren't"} used again. Leave unticked if {sources === 1 ? "it shows" : "they show"} other places you want to keep.</span></span>
        </label>
      )}
      <ErrorNote error={action.error} onClose={action.clearError} />
      <div className="row">
        <span className="grow" />
        <button className="btn" onClick={onClose}>Cancel</button>
        <button className="btn danger-solid" disabled={action.busy} onClick={remove}>{action.busy ? "Removing…" : "Remove pin"}</button>
      </div>
    </Modal>
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
    new maplibregl.Marker({ color: colorOf(place.category) }).setLngLat([place.longitude, place.latitude]).addTo(map);
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
          <h4>{(() => { const Icon = INFO_CARD_ICON[card.key]; return Icon ? <Icon size={15} /> : null; })()}<span className="grow">{card.title}</span>
            {sourceCount(card.facts) > 1 && <span className="muted small" title="Each source is kept separately; different advice is not merged">from {sourceCount(card.facts)} sources</span>}</h4>
          {card.facts.map((f, i) => {
            // For values that change (prices, hours), label the newest one.
            const sameTypeEarlier = TIME_SENSITIVE.has(f.type) && card.facts.slice(0, i).some((o) => o.type === f.type);
            return (
              <FactRow key={f.id} fact={f} source={<FactSource fact={f} />} extra={<>
                {TIME_SENSITIVE.has(f.type) && card.facts.filter((o) => o.type === f.type).length > 1 &&
                  <><Pill tone={sameTypeEarlier ? "muted" : "ok"}>{sameTypeEarlier ? "Earlier" : "Latest saved"}</Pill>{" "}</>}
                {warned.has(f.id) && <span title="See “Before you go” above" className="warn-text"><AlertTriangle size={12} /> </span>}
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
        <span className="row" style={{ gap: 4 }}><Clapperboard size={12} /> Instagram Reel</span>
        {fact.creator && <span>{fact.creator}</span>}
        <span>· {kind}{fact.sourceTimeSec != null ? ` · ${formatTime(fact.sourceTimeSec)}` : ""}</span>
        <button onClick={() => nav.openReel(fact.reelId!, fact.sourceTimeSec ?? undefined)}>View source <ExternalLink size={11} /></button>
      </div>
    );
  }
  return (
    <div className="fact-source">
      <span className="row" style={{ gap: 4 }}>{(() => { const Icon = FACT_ICON[fact.type] ?? Smartphone; return <Icon size={12} />; })()} {kind}</span>
      <span>· {formatDate(fact.validFrom)}</span>
      {fact.sourceType && fact.sourceType !== "unknown" && <span>· {SOURCE[fact.sourceType]}</span>}
      {fact.creator && <span>· {fact.creator}</span>}
      {fact.screenshotId && <button onClick={() => nav.openScreenshot(fact.screenshotId!, fact.sourceBlockIds)}>View source <Search size={11} /></button>}
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
          <span className="row"><Luggage size={14} className="muted" /> {t.name} <span className="muted small">· {t.placeCount} places</span></span>
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
        <button className="btn primary"><Plus size={14} /> Create &amp; add</button>
      </form>
    </Modal>
  );
}

function sourceCount(facts: Fact[]): number {
  return new Set(facts.map((f) => f.reelId ?? f.screenshotId ?? f.id)).size;
}
