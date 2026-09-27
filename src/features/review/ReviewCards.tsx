import { useState } from "react";
import { ReviewService } from "../../api/services";
import type { PlaceCandidate, ReviewEntry } from "../../api/types";
import { CandidateRow, ErrorNote, PlaceLine, Pill, Thumb } from "../../components/common";
import { PlaceSearchDialog } from "../../components/PlacePicker";
import { formatDate } from "../../lib/labels";
import { useAction, useLoad, useNav } from "../../lib/nav";

export const REVIEW_TITLES: Record<string, string> = {
  travelClassification: "Is this travel related?",
  placeResolution: "Which place is this?",
  duplicatePlace: "Are these the same place?",
  photoCrop: "Use this image as a place photo?",
  processingFailure: "Processing failed",
};

/** Your answer in plain words. */
export function answerLabel(entry: ReviewEntry): string {
  const { review, chosen } = entry;
  switch (review.resolution) {
    case "chose place":
    case "confirmed": return chosen ? `It's ${chosen.canonicalName}` : "Chose a place";
    case "not a place": return "Not a place";
    case "travel": return "Yes, travel";
    case "not travel": return "Not travel";
    case "merged": return "Same place — merged";
    case "kept separate": return "Different places — kept separate";
    case "accepted": return "Used as a place photo";
    case "rejected": return "Photo not used";
    case "retried": return "Retried";
    case "ignored": return "Ignored";
    case "merged into another place": return "Merged into another place";
    default: return review.resolution ?? "Answered";
  }
}

/** A question waiting for you. `compact` = shown inside the screenshot/Reel viewer (no thumbnail). */
export function ReviewCard({ entry, onOpen, compact = false }: { entry: ReviewEntry; onOpen?: () => void; compact?: boolean }) {
  const nav = useNav();
  const action = useAction();
  const [search, setSearch] = useState(false);
  const { review, placeA, placeB, image } = entry;
  const resolve = (act: string, candidate?: PlaceCandidate) => action.run(() => ReviewService.resolve(review.id, act, candidate));

  return (
    <div className={`card review-card ${compact ? "compact" : ""}`}>
      {!compact && (
        <button className="review-thumb" onClick={onOpen} title="Open the screenshot or Reel">
          <Thumb path={image?.imagePath ?? review.screenshotThumbnail} fallback={review.reelId ? "🎬" : "📸"} />
        </button>
      )}
      <div className="stack">
        <div className="row">
          <h3 className="grow review-title">{REVIEW_TITLES[review.kind]}</h3>
          {!compact && onOpen && <button className="btn small ghost" onClick={onOpen}>Open ›</button>}
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
            {!compact && onOpen && <button className="btn" onClick={onOpen}>Adjust</button>}
            <button className="btn" onClick={() => resolve("reject")}>Reject</button>
          </div>
        )}

        {review.kind === "processingFailure" && (
          <div className="row">
            <button className="btn primary" onClick={() => resolve("retry")}>Retry</button>
            {!compact && onOpen && <button className="btn" onClick={onOpen}>Review manually</button>}
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

/** An answered question: what you chose, with a way to change it. */
export function AnsweredCard({ entry, onOpen, compact = false }: { entry: ReviewEntry; onOpen?: () => void; compact?: boolean }) {
  const action = useAction();
  const [search, setSearch] = useState<"changePlace" | "isPlace" | null>(null);
  const { review, placeA, placeB, image } = entry;
  const change = (act: string, candidate?: PlaceCandidate) => action.run(() => ReviewService.change(review.id, act, candidate));
  const res = review.resolution;

  return (
    <div className={`card review-card answered ${compact ? "compact" : ""}`}>
      {!compact && (
        <button className="review-thumb" onClick={onOpen} title="Open the screenshot or Reel">
          <Thumb path={image?.imagePath ?? review.screenshotThumbnail} fallback={review.reelId ? "🎬" : "📸"} />
        </button>
      )}
      <div className="stack">
        <div className="row">
          <span className="grow review-title-small">{REVIEW_TITLES[review.kind]}</span>
          <span className="muted small">{formatDate(review.resolvedAt ?? review.createdAt)}</span>
          {!compact && onOpen && <button className="btn small ghost" onClick={onOpen}>Open ›</button>}
        </div>
        <div className="answer-line">
          <span className="muted small">Your answer</span>
          <Pill tone="ok">{answerLabel(entry)}</Pill>
          {entry.chosen && <span className="muted small"><PlaceLine place={entry.chosen} /></span>}
        </div>
        <ErrorNote error={action.error} onClose={action.clearError} />
        <div className="row wrap">
          {review.kind === "placeResolution" && (res === "chose place" || res === "confirmed") &&
            <button className="btn small" disabled={action.busy} onClick={() => setSearch("changePlace")}>Change place…</button>}
          {review.kind === "placeResolution" && res === "not a place" &&
            <button className="btn small" disabled={action.busy} onClick={() => setSearch("isPlace")}>It is a place…</button>}
          {review.kind === "travelClassification" && res === "travel" &&
            <button className="btn small" disabled={action.busy} onClick={() => change("notTravel")}>Change to Not travel</button>}
          {review.kind === "travelClassification" && res === "not travel" &&
            <button className="btn small" disabled={action.busy} onClick={() => change("travel")}>Change to Travel</button>}
          {review.kind === "duplicatePlace" && res === "kept separate" && placeA && placeB &&
            <button className="btn small" disabled={action.busy} onClick={() => change("merge")}>Merge them now</button>}
          {review.kind === "photoCrop" && res === "accepted" &&
            <button className="btn small" disabled={action.busy} onClick={() => change("removePhoto")}>Remove photo</button>}
          {review.kind === "processingFailure" &&
            <button className="btn small" disabled={action.busy} onClick={() => change("retry")}>Try again</button>}
          {((review.kind === "duplicatePlace" && res === "merged") || (review.kind === "photoCrop" && res === "rejected")) &&
            <span className="muted small">This one can't be undone.</span>}
        </div>
      </div>
      {search && (
        <PlaceSearchDialog title={search === "changePlace" ? "Which place is it really?" : "Which place is it?"}
          initialQuery={entry.chosen?.canonicalName ?? [review.extractedPlace?.display_name, review.extractedPlace?.country].filter(Boolean).join(", ")}
          onClose={() => setSearch(null)} onPick={(c) => { const act = search; setSearch(null); void change(act, c); }} />
      )}
    </div>
  );
}

/**
 * Inside the screenshot/Reel viewer: what's waiting to be reviewed for it (answer right here),
 * and your earlier answers (change them here too). Answers also appear under Review → Recently reviewed.
 */
export function SourceReviews({ screenshotId, reelId }: { screenshotId?: string; reelId?: string }) {
  const { data: entries = [] } = useLoad(() => ReviewService.forSource(screenshotId ?? null, reelId ?? null), [screenshotId, reelId]);
  const open = entries.filter((e) => !e.review.isResolved);
  const answered = entries.filter((e) => e.review.isResolved)
    .sort((a, b) => (b.review.resolvedAt ?? b.review.createdAt).localeCompare(a.review.resolvedAt ?? a.review.createdAt));
  if (entries.length === 0) return null;
  return (
    <div className={`source-reviews ${open.length ? "has-open" : ""}`}>
      {open.length > 0 && (
        <>
          <div className="source-reviews-head">⚑ Needs your review <span className="muted small">{open.length}</span></div>
          <div className="stack">{open.map((e) => <ReviewCard key={e.review.id} entry={e} compact />)}</div>
        </>
      )}
      {answered.length > 0 && (
        <details className="source-answered" open={open.length === 0 || undefined}>
          <summary>Your answers <span className="muted small">{answered.length}</span></summary>
          <div className="stack">{answered.map((e) => <AnsweredCard key={e.review.id} entry={e} compact />)}</div>
        </details>
      )}
    </div>
  );
}
