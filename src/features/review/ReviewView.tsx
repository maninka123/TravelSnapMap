import { useState } from "react";
import { ReviewService } from "../../api/services";
import type { PlaceCandidate, ReviewEntry } from "../../api/types";
import { CandidateRow, Empty, ErrorNote, PlaceLine, PlaceSearchDialog, Thumb } from "../../components/common";
import { formatDate } from "../../lib/labels";
import { useAction, useLoad, useNav } from "../../lib/nav";

const TITLES: Record<string, string> = {
  travelClassification: "Is this travel related?",
  placeResolution: "Which place is this?",
  duplicatePlace: "Are these the same place?",
  photoCrop: "Use this image as a place photo?",
  processingFailure: "Processing failed",
};

/** Everything the app wasn't sure about. Asking beats silently guessing. */
export function ReviewView() {
  const { data: entries = [], error } = useLoad(() => ReviewService.list(), []);
  const [filter, setFilter] = useState("");
  const counts = entries.reduce<Record<string, number>>((m, e) => ({ ...m, [e.review.kind]: (m[e.review.kind] ?? 0) + 1 }), {});
  const shown = entries.filter((e) => !filter || e.review.kind === filter);

  return (
    <div className="page">
      <div className="page-head"><h2>Review</h2><span className="muted">{entries.length} open</span></div>
      <div className="toolbar">
        <button className={`chip-btn ${!filter ? "active" : ""}`} onClick={() => setFilter("")}>All</button>
        {Object.entries(counts).map(([k, n]) => (
          <button key={k} className={`chip-btn ${filter === k ? "active" : ""}`} onClick={() => setFilter(k)}>{TITLES[k]} {n}</button>
        ))}
      </div>
      {error && <p className="bad">{error}</p>}
      {shown.length === 0 ? <Empty icon="✅" title="All caught up">Uncertain results will appear here.</Empty> : (
        <div className="stack">{shown.map((e) => <ReviewCard key={e.review.id} entry={e} />)}</div>
      )}
    </div>
  );
}

function ReviewCard({ entry }: { entry: ReviewEntry }) {
  const nav = useNav();
  const action = useAction();
  const [search, setSearch] = useState(false);
  const { review, placeA, placeB, image } = entry;
  const resolve = (act: string, candidate?: PlaceCandidate) => action.run(() => ReviewService.resolve(review.id, act, candidate));
  const openSource = () => {
    if (review.reelId) nav.openReel(review.reelId);
    else if (review.screenshotId) nav.openScreenshot(review.screenshotId);
  };

  return (
    <div className="card review-card">
      <Thumb path={image?.imagePath ?? review.screenshotThumbnail} fallback={review.reelId ? "🎬" : "📸"} />
      <div className="stack">
        <div className="row">
          <h3 className="grow">{TITLES[review.kind]}</h3>
          <button className="btn small ghost" onClick={openSource}>Open source</button>
        </div>
        <div className="muted">{review.message}</div>
        <ErrorNote error={action.error} onClose={action.clearError} />

        {review.kind === "travelClassification" && (
          <div className="row">
            <button className="btn primary" onClick={() => resolve("yes")}>Yes, travel</button>
            <button className="btn" onClick={() => resolve("no")}>No</button>
          </div>
        )}

        {review.kind === "placeResolution" && (
          <>
            {placeA && (
              <div className="candidate">
                <div><div className="strong">Best guess: {placeA.canonicalName}</div><PlaceLine place={placeA} /></div>
                <button className="btn primary" onClick={() => resolve("confirm")}>Confirm</button>
              </div>
            )}
            <div className="candidate-list">
              {review.candidates.filter((c) => !placeA || c.mapIdentifier !== placeA.mapIdentifier).map((c, i) => (
                <CandidateRow key={i} candidate={c} onPick={() => resolve("choose", c)} />
              ))}
            </div>
            <div className="row">
              <button className="btn" onClick={() => setSearch(true)}>Search another place…</button>
              <button className="btn ghost" onClick={() => resolve("dismiss")}>Not a place</button>
            </div>
          </>
        )}

        {review.kind === "duplicatePlace" && placeA && placeB && (
          <>
            <div className="compare">
              {[placeA, placeB].map((p) => (
                <div key={p.id} className="candidate" style={{ cursor: "pointer" }} onClick={() => nav.openPlace(p.id)}>
                  <div><div className="strong">{p.canonicalName}</div><PlaceLine place={p} /><div className="muted small">{p.sourceCount} sources · added {formatDate(p.createdAt)}</div></div>
                </div>
              ))}
            </div>
            <div className="row">
              <button className="btn primary" onClick={() => resolve("merge")}>Merge</button>
              <button className="btn" onClick={() => resolve("separate")}>Keep separate</button>
            </div>
          </>
        )}

        {review.kind === "photoCrop" && (
          <div className="row">
            <button className="btn primary" onClick={() => resolve("accept")}>Accept</button>
            <button className="btn" onClick={openSource}>Adjust</button>
            <button className="btn" onClick={() => resolve("reject")}>Reject</button>
          </div>
        )}

        {review.kind === "processingFailure" && (
          <div className="row">
            <button className="btn primary" onClick={() => resolve("retry")}>Retry</button>
            <button className="btn" onClick={openSource}>Review manually</button>
            <button className="btn" onClick={() => resolve("ignore")}>Ignore</button>
          </div>
        )}
      </div>
      {search && (
        <PlaceSearchDialog initialQuery={[review.extractedPlace?.display_name, review.extractedPlace?.country].filter(Boolean).join(", ")}
          onClose={() => setSearch(false)} onPick={(c) => { setSearch(false); void resolve("choose", c); }} />
      )}
    </div>
  );
}
