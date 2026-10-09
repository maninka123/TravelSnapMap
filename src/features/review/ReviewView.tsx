import { AlertCircle, CheckCircle2, Copy, History, Image as ImageIcon, MapPinned, HelpCircle } from "lucide-react";
import { useState } from "react";
import { ReviewService } from "../../api/services";
import type { ReviewEntry, ReviewKind } from "../../api/types";
import { Empty } from "../../components/common";
import { Segmented } from "../../components/ui";
import { useLoad, useNav, type SourceRef } from "../../lib/nav";
import { AnsweredCard, ReviewCard } from "./ReviewCards";

/** Why each kind of question is asked — shown above the group so every decision has a reason. */
export const REVIEW_GROUPS: { kind: ReviewKind; title: string; why: string; icon: typeof AlertCircle }[] = [
  { kind: "placeResolution", title: "Which place is this?", icon: MapPinned,
    why: "Apple Maps found more than one possible match, or none close enough. Nothing goes on your map until you pick one." },
  { kind: "duplicatePlace", title: "Same place twice?", icon: Copy,
    why: "Two saved places are very close and have similar names. Merge them, or keep them apart if they're different businesses." },
  { kind: "travelClassification", title: "Is this travel?", icon: HelpCircle,
    why: "The AI wasn't sure this screenshot is about a place to visit. Your answer is kept if it's ever read again." },
  { kind: "photoCrop", title: "Use this photo?", icon: ImageIcon,
    why: "A picture was cut from a screenshot, but it may not be a photo of the place." },
  { kind: "processingFailure", title: "Couldn't be read", icon: AlertCircle,
    why: "These failed several times (for example, the file was moved). Try again, or ignore them." },
];

/** Everything the app wasn't sure about, grouped by reason — and, one click away, what you've answered recently. */
export function ReviewView() {
  const nav = useNav();
  const [tab, setTab] = useState<"open" | "recent">("open");
  const [source, setSource] = useState<"all" | "screenshots" | "reels">("all");
  const { data: openEntries, error } = useLoad(() => ReviewService.list(), []);
  const { data: recentEntries = [] } = useLoad(() => (tab === "recent" ? ReviewService.recent() : Promise.resolve([] as ReviewEntry[])), [tab]);
  const entries = (tab === "open" ? openEntries ?? [] : recentEntries).filter((e) => source === "all" || (source === "reels" ? !!e.review.reelId : !e.review.reelId));
  const reelCount = (tab === "open" ? openEntries ?? [] : recentEntries).filter((e) => e.review.reelId).length;
  const totalCount = (tab === "open" ? openEntries ?? [] : recentEntries).length;

  // Opening an item shows the same viewer as Sources; ‹ › steps through the items listed here.
  const refs: SourceRef[] = [];
  entries.forEach((e) => {
    const ref: SourceRef | null = e.review.reelId ? { type: "reel", id: e.review.reelId } : e.review.screenshotId ? { type: "screenshot", id: e.review.screenshotId } : null;
    if (ref && !refs.some((r) => r.type === ref.type && r.id === ref.id)) refs.push(ref);
  });
  const open = (e: ReviewEntry) => {
    if (e.review.reelId) nav.openReel(e.review.reelId, undefined, refs);
    else if (e.review.screenshotId) nav.openScreenshot(e.review.screenshotId, undefined, refs);
    else if (e.placeA) nav.openPlace(e.placeA.id);
  };

  return (
    <div>
      <div className="toolbar">
        <Segmented label="Review" value={tab} onChange={setTab} options={[
          { value: "open", label: <>To review <span className="tab-count">{openEntries?.length ?? 0}</span></> },
          { value: "recent", label: <><History size={13} /> Recently answered</> },
        ]} />
        <Segmented label="Source" value={source} onChange={setSource} options={[
          { value: "all", label: <>All <span className="tab-count">{totalCount}</span></> },
          { value: "screenshots", label: <>Screenshots <span className="tab-count">{totalCount - reelCount}</span></> },
          { value: "reels", label: <>Reels <span className="tab-count">{reelCount}</span></> },
        ]} />
      </div>
      {error && <div className="error-note">{error}</div>}
      {tab === "recent" && entries.length > 0 && <p className="muted small" style={{ marginBottom: 12 }}>Your latest answers. If one was wrong, change it here — or open the item to see it in context.</p>}

      {tab === "open" && openEntries && entries.length === 0 ? (
        <Empty icon={<CheckCircle2 size={26} />} title="All caught up">Anything TravelSnapMap isn't sure about will appear here, with the reason it's asking.</Empty>
      ) : tab === "recent" && entries.length === 0 ? (
        <Empty icon={<History size={26} />} title="Nothing answered yet">Answers you give here or in a screenshot's viewer show up in this list.</Empty>
      ) : tab === "open" ? (
        REVIEW_GROUPS.map((g) => {
          const items = entries.filter((e) => e.review.kind === g.kind);
          if (items.length === 0) return null;
          return (
            <section key={g.kind} className="review-group" aria-label={g.title}>
              <div className="review-group-head">
                <span className="review-group-icon"><g.icon size={16} /></span>
                <div className="grow"><h3>{g.title} <span className="muted small">{items.length}</span></h3><p>{g.why}</p></div>
              </div>
              <div className="stack">{items.map((e) => <ReviewCard key={e.review.id} entry={e} onOpen={() => open(e)} />)}</div>
            </section>
          );
        })
      ) : (
        <div className="stack">{entries.map((e) => <AnsweredCard key={e.review.id} entry={e} onOpen={() => open(e)} />)}</div>
      )}
    </div>
  );
}
