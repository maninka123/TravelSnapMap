import { useState } from "react";
import { ReviewService } from "../../api/services";
import type { ReviewEntry } from "../../api/types";
import { Empty } from "../../components/common";
import { useLoad, useNav, type SourceRef } from "../../lib/nav";
import { AnsweredCard, REVIEW_TITLES, ReviewCard } from "./ReviewCards";

/** Everything the app wasn't sure about — and, one click away, what you've answered recently. */
export function ReviewView() {
  const nav = useNav();
  const [tab, setTab] = useState<"open" | "recent">("open");
  const { data: openEntries = [], error } = useLoad(() => ReviewService.list(), []);
  const { data: recentEntries = [] } = useLoad(() => (tab === "recent" ? ReviewService.recent() : Promise.resolve([] as ReviewEntry[])), [tab]);
  const entries = tab === "open" ? openEntries : recentEntries;

  const [filter, setFilter] = useState("");
  const [source, setSource] = useState<"all" | "screenshots" | "reels">("all");
  const bySource = entries.filter((e) => source === "all" || (source === "reels" ? !!e.review.reelId : !e.review.reelId));
  const reelCount = entries.filter((e) => e.review.reelId).length;
  const counts = bySource.reduce<Record<string, number>>((m, e) => ({ ...m, [e.review.kind]: (m[e.review.kind] ?? 0) + 1 }), {});
  const shown = bySource.filter((e) => !filter || e.review.kind === filter);

  // Opening an item shows the same viewer as Sources; ‹ › steps through the items listed here.
  const refs: SourceRef[] = [];
  shown.forEach((e) => {
    const ref: SourceRef | null = e.review.reelId ? { type: "reel", id: e.review.reelId } : e.review.screenshotId ? { type: "screenshot", id: e.review.screenshotId } : null;
    if (ref && !refs.some((r) => r.type === ref.type && r.id === ref.id)) refs.push(ref);
  });
  const open = (e: ReviewEntry) => {
    if (e.review.reelId) nav.openReel(e.review.reelId, undefined, refs);
    else if (e.review.screenshotId) nav.openScreenshot(e.review.screenshotId, undefined, refs);
  };

  return (
    <div className="page">
      <div className="page-head">
        <h2>Review</h2>
        <span className="spacer" />
        <div className="segmented">
          <button className={tab === "open" ? "active" : ""} onClick={() => { setTab("open"); setFilter(""); }}>
            To review<span className="tab-count">{openEntries.length}</span>
          </button>
          <button className={tab === "recent" ? "active" : ""} onClick={() => { setTab("recent"); setFilter(""); }}>🕘 Recently reviewed</button>
        </div>
      </div>
      <div className="toolbar">
        <div className="segmented">
          <button className={source === "all" ? "active" : ""} onClick={() => { setSource("all"); setFilter(""); }}>All {entries.length}</button>
          <button className={source === "screenshots" ? "active" : ""} onClick={() => { setSource("screenshots"); setFilter(""); }}>📸 Screenshots {entries.length - reelCount}</button>
          <button className={source === "reels" ? "active" : ""} onClick={() => { setSource("reels"); setFilter(""); }}>🎬 Reels {reelCount}</button>
        </div>
        <button className={`chip-btn ${!filter ? "active" : ""}`} onClick={() => setFilter("")}>All kinds</button>
        {Object.entries(counts).map(([k, n]) => (
          <button key={k} className={`chip-btn ${filter === k ? "active" : ""}`} onClick={() => setFilter(k)}>{REVIEW_TITLES[k]} {n}</button>
        ))}
      </div>
      {tab === "recent" && <p className="muted small" style={{ marginTop: -4 }}>Your latest answers. If one was wrong, change it here — or open the item to see it in context.</p>}
      {error && <p className="bad">{error}</p>}
      {shown.length === 0 ? (
        tab === "open"
          ? <Empty icon="✅" title="All caught up">Uncertain results will appear here.</Empty>
          : <Empty icon="🕘" title="Nothing reviewed yet">Answers you give in Review (or in a screenshot's viewer) show up here.</Empty>
      ) : (
        <div className="stack">
          {shown.map((e) => tab === "open"
            ? <ReviewCard key={e.review.id} entry={e} onOpen={() => open(e)} />
            : <AnsweredCard key={e.review.id} entry={e} onOpen={() => open(e)} />)}
        </div>
      )}
    </div>
  );
}
