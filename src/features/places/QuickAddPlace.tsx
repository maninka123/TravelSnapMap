import { useState } from "react";
import { PlaceService } from "../../api/services";
import type { PersonalStatus, PlaceCandidate } from "../../api/types";
import { CandidateRow, ErrorNote, Flag, Modal } from "../../components/common";
import { CATEGORY, STATUS } from "../../lib/labels";
import { useAction, useNav } from "../../lib/nav";

/**
 * "+ Add Place": a place you already know, without a screenshot or Reel. Search Apple Maps, pick the
 * result, choose a status and save. If it's already in your library you're taken to it instead.
 */
export function QuickAddPlace({ onClose }: { onClose: () => void }) {
  const nav = useNav();
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<PlaceCandidate[]>();
  const [picked, setPicked] = useState<PlaceCandidate>();
  const [status, setStatus] = useState<PersonalStatus>("wantToVisit");
  const [category, setCategory] = useState("");
  const [notes, setNotes] = useState("");
  const search = useAction();
  const save = useAction();

  const runSearch = async () => {
    if (!query.trim()) return;
    setResults(await search.run(() => PlaceService.searchMap(query.trim())) ?? []);
  };

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
          <p className="muted small">Search Apple Maps — the location always comes from the map, so it lands exactly on the right spot.</p>
          <form className="row" onSubmit={(e) => { e.preventDefault(); void runSearch(); }}>
            <input autoFocus className="grow" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="e.g. Fushimi Inari, Kyoto" />
            <button className="btn primary" disabled={search.busy || !query.trim()}>{search.busy ? "Searching…" : "Search"}</button>
          </form>
          <ErrorNote error={search.error} onClose={search.clearError} />
          <div className="candidate-list">
            {results?.length === 0 && <p className="muted small">No results. Try adding the city or country.</p>}
            {results?.map((c, i) => <CandidateRow key={i} candidate={c} action="Select" onPick={() => setPicked(c)} />)}
          </div>
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
            <select value={category} onChange={(e) => setCategory(e.target.value)}>
              <option value="">Automatic (from Apple Maps)</option>
              {Object.entries(CATEGORY).map(([k, c]) => <option key={k} value={k}>{c.emoji} {c.label}</option>)}
            </select>
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
