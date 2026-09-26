import { useState } from "react";
import { PlaceService } from "../../api/services";
import type { PersonalStatus, PlaceCandidate, PlaceCategory } from "../../api/types";
import { ErrorNote, Flag, Modal } from "../../components/common";
import { PlacePickList } from "../../components/PlacePicker";
import { CategoryPicker } from "../../components/CategoryPicker";
import { STATUS } from "../../lib/labels";
import { useAction, useNav } from "../../lib/nav";

/**
 * "+ Add Place": a place you already know, without a screenshot or Reel. Saved places are suggested first
 * (picking one just opens it); otherwise pick an Apple Maps result, choose a status and save.
 */
export function QuickAddPlace({ onClose }: { onClose: () => void }) {
  const nav = useNav();
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<PlaceCandidate>();
  const [status, setStatus] = useState<PersonalStatus>("wantToVisit");
  const [category, setCategory] = useState<PlaceCategory | "">("");
  const [notes, setNotes] = useState("");
  const save = useAction();

  const submit = async () => {
    if (!picked) return;
    const r = await save.run(() => PlaceService.addManual(picked, status, category || null, notes.trim() || null));
    if (!r) return;
    onClose();
    nav.openPlace(r.id);
  };

  return (
    <Modal title={picked ? "Add place" : "Add a place you know"} onClose={onClose}>
      {!picked ? (
        <>
          <p className="muted small">Type a place — places you've already saved come first; the location always comes from Apple Maps.</p>
          <input autoFocus value={query} onChange={(e) => setQuery(e.target.value)} placeholder="e.g. Fushimi Inari, Kyoto" />
          <PlacePickList query={query} action="Select"
            onPickSaved={(p) => { onClose(); nav.openPlace(p.id); }}
            onPickMap={(c, existing) => { if (existing) { onClose(); nav.openPlace(existing.id); } else setPicked(c); }} />
        </>
      ) : (
        <div className="stack">
          <div className="quick-add-picked">
            <div className="grow">
              <div className="strong"><Flag code={picked.countryCode} name={picked.country} /> {picked.name}</div>
              <div className="muted small">{[picked.address, picked.country].filter(Boolean).join(" · ")}</div>
            </div>
            <button className="btn small" onClick={() => setPicked(undefined)}>Change</button>
          </div>
          <div className="field"><label>Status</label>
            <div className="segmented wrap">
              {(["wantToVisit", "maybe", "visited", "favourite"] as PersonalStatus[]).map((s) => (
                <button key={s} type="button" className={status === s ? "active" : ""} onClick={() => setStatus(s)}>{STATUS[s].emoji} {STATUS[s].label}</button>
              ))}
            </div>
          </div>
          <div className="field"><label>Kind of place</label>
            <CategoryPicker value={category} onChange={setCategory} allowAuto autoLabel="Automatic (from Apple Maps)" />
          </div>
          <div className="field"><label>Notes (optional)</label>
            <textarea rows={3} value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="Why you want to go, who told you about it…" />
          </div>
          <ErrorNote error={save.error} onClose={save.clearError} />
          <div className="row">
            <span className="grow" />
            <button className="btn" onClick={onClose}>Cancel</button>
            <button className="btn primary" disabled={save.busy} onClick={submit}>{save.busy ? "Saving…" : "Save place"}</button>
          </div>
        </div>
      )}
    </Modal>
  );
}
