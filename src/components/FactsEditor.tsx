import { Plus } from "lucide-react";
import { Pencil, Trash2 } from "lucide-react";
import { useState, type ReactNode } from "react";
import { PlaceService } from "../api/services";
import type { Fact, Place, TravelFactType } from "../api/types";
import { FACT } from "../lib/labels";
import { useAction } from "../lib/nav";
import { ErrorNote, Pill } from "./common";

const TYPES = Object.entries(FACT) as [TravelFactType, { label: string }][];

/** One saved tip that you can edit or delete. Edited tips are yours and survive reprocessing. */
export function FactRow({ fact, source, extra }: { fact: Fact; source?: ReactNode; extra?: ReactNode }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(fact.text);
  const [type, setType] = useState<TravelFactType>(fact.type);
  const action = useAction();

  const save = async () => {
    if (await action.run(() => PlaceService.updateFact(fact.id, text, type)) !== undefined) setEditing(false);
  };

  if (editing) {
    return (
      <div className="fact fact-editing">
        <div className="row">
          <select value={type} onChange={(e) => setType(e.target.value as TravelFactType)}>
            {TYPES.map(([k, t]) => <option key={k} value={k}>{t.label}</option>)}
          </select>
        </div>
        <textarea autoFocus rows={2} value={text} onChange={(e) => setText(e.target.value)}
                  onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || !e.shiftKey)) { e.preventDefault(); void save(); } if (e.key === "Escape") setEditing(false); }} />
        <ErrorNote error={action.error} onClose={action.clearError} />
        <div className="row">
          <span className="grow muted small">↩ to save · Esc to cancel</span>
          <button className="btn small" onClick={() => { setEditing(false); setText(fact.text); setType(fact.type); }}>Cancel</button>
          <button className="btn small primary" disabled={action.busy || !text.trim()} onClick={save}>Save</button>
        </div>
      </div>
    );
  }
  return (
    <div className="fact">
      <div className="row">
        <div className="fact-text grow">
          {extra}{fact.text}
          {fact.origin === "user" && <> <Pill tone="muted">edited by you</Pill></>}
        </div>
        <span className="fact-actions">
          <button className="icon-btn" title="Edit" aria-label="Edit tip" onClick={() => setEditing(true)}><Pencil size={13} /></button>
          <button className="icon-btn" title="Delete" aria-label="Delete tip" onClick={() => action.run(() => PlaceService.deleteFact(fact.id))}><Trash2 size={13} /></button>
        </span>
      </div>
      {fact.sourceQuote && fact.origin !== "user" && <div className="muted small">“{fact.sourceQuote}”</div>}
      {source}
    </div>
  );
}

/** "Add a tip" for a place (optionally tied to the screenshot or Reel being viewed). */
export function AddFact({ places, screenshotId, reelId }: { places: Place[]; screenshotId?: string; reelId?: string }) {
  const [open, setOpen] = useState(false);
  const [placeId, setPlaceId] = useState(places[0]?.id ?? "");
  const [type, setType] = useState<TravelFactType>("generalTip");
  const [text, setText] = useState("");
  const action = useAction();
  if (places.length === 0) return null;
  if (!open) return <button className="btn small ghost add-fact-btn" onClick={() => setOpen(true)}><Plus size={13} /> Add a tip</button>;

  const save = async () => {
    const id = await action.run(() => PlaceService.addFact(placeId || places[0].id, type, text, screenshotId ?? null, reelId ?? null));
    if (id !== undefined) { setText(""); setOpen(false); }
  };
  return (
    <div className="fact fact-editing">
      <div className="row wrap">
        {places.length > 1 && (
          <select value={placeId} onChange={(e) => setPlaceId(e.target.value)}>
            {places.map((p) => <option key={p.id} value={p.id}>{p.canonicalName}</option>)}
          </select>
        )}
        <select value={type} onChange={(e) => setType(e.target.value as TravelFactType)}>
          {TYPES.map(([k, t]) => <option key={k} value={k}>{t.label}</option>)}
        </select>
      </div>
      <textarea autoFocus rows={2} placeholder="e.g. Go before 8 am to avoid the crowds" value={text} onChange={(e) => setText(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); if (text.trim()) void save(); } if (e.key === "Escape") setOpen(false); }} />
      <ErrorNote error={action.error} onClose={action.clearError} />
      <div className="row">
        <span className="grow" />
        <button className="btn small" onClick={() => setOpen(false)}>Cancel</button>
        <button className="btn small primary" disabled={action.busy || !text.trim()} onClick={save}>Add tip</button>
      </div>
    </div>
  );
}
