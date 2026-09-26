import { useEffect, useMemo, useState } from "react";
import { LibraryService, ProcessingService, ScreenshotService } from "../../api/services";
import type { ScreenshotView } from "../../api/types";
import { Empty, Pill, Thumb } from "../../components/common";
import { formatDate, formatTime, PROCESSING, SOURCE } from "../../lib/labels";
import { useAction, useLoad, useNav } from "../../lib/nav";
import { ImportReelDialog } from "../reels/ImportReelDialog";
import { ScanControls } from "../../components/ScanControls";
import { LibraryProgress } from "../../components/LibraryProgress";

// The three filters people actually use; the rest live one level deeper under "More".
const PRIMARY: { key: ScreenshotView; label: string }[] = [
  { key: "all", label: "Travel" },
  { key: "needsReview", label: "Needs review" },
  { key: "notTravel", label: "Not travel" },
];
const MORE: { key: ScreenshotView; label: string; section: "Travel" | "Other"; hint: string }[] = [
  { key: "processed", label: "Places found", section: "Travel", hint: "Place confirmed and on your map" },
  { key: "multiplePlaces", label: "Several places", section: "Travel", hint: "One screenshot covering 2+ places" },
  { key: "noPlace", label: "No place yet", section: "Travel", hint: "Travel-related, no specific place identified" },
  { key: "pending", label: "Not read yet", section: "Other", hint: "Waiting to be processed" },
  { key: "failed", label: "Couldn't read", section: "Other", hint: "Errors, e.g. an iCloud download failed" },
  { key: "ignored", label: "Ignored", section: "Other", hint: "Hidden by you" },
];

/** The library of evidence: travel screenshots and Instagram Reels side by side. */
export function SourcesView() {
  const nav = useNav();
  const [view, setView] = useState<ScreenshotView>("all");
  const [kind, setKind] = useState<"all" | "screenshot" | "reel">("all");
  const [search, setSearch] = useState("");
  const [importing, setImporting] = useState(false);
  const action = useAction();
  const PAGE = 300;
  const [limit, setLimit] = useState(PAGE);
  useEffect(() => setLimit(PAGE), [view, search]);
  const { data: items = [], error } = useLoad(() => LibraryService.items(view, search, limit), [view, search, limit]);
  // True totals, so the tabs add up to the whole library (Needs review is part of Travel).
  const { data: counts } = useLoad(async () => {
    const [travel, needsReview, notTravel, current] = await Promise.all([
      ScreenshotService.count("all", search), ScreenshotService.count("needsReview", search),
      ScreenshotService.count("notTravel", search), ScreenshotService.count(view, search),
    ]);
    return { all: travel, needsReview, notTravel, current } as Record<string, number>;
  }, [view, search]);
  const shownShots = items.filter((i) => i.kind === "screenshot").length;
  const { data: moreCounts } = useLoad(async () => {
    const entries = await Promise.all(MORE.map(async (m) => [m.key, await ScreenshotService.count(m.key, search)] as const));
    return Object.fromEntries(entries) as Record<string, number>;
  }, [search]);
  const shown = items.filter((i) => kind === "all" || i.kind === kind);
  // The viewer steps through the whole tab (not just the loaded page), in the same order as the grid.
  const { data: shotRefs } = useLoad(() => ScreenshotService.refs(view, search), [view, search]);
  const list = useMemo(() => {
    if (!shotRefs) return shown.map((i) => (i.kind === "reel" ? { type: "reel" as const, id: i.reel.id } : { type: "screenshot" as const, id: i.screenshot.id }));
    const reels = kind === "screenshot" ? [] : items.flatMap((i) => (i.kind === "reel" ? [{ type: "reel" as const, id: i.reel.id, date: i.date ?? "" }] : []));
    const shots = kind === "reel" ? [] : shotRefs.map((r) => ({ type: "screenshot" as const, id: r.id, date: r.date ?? "" }));
    return [...reels, ...shots].sort((a, b) => b.date.localeCompare(a.date)).map(({ type, id }) => ({ type, id }));
  }, [shotRefs, items, shown, kind]);

  return (
    <div className="page">
      <div className="page-head">
        <h2>Sources</h2>
        <div className="segmented">
          {(["all", "screenshot", "reel"] as const).map((k) => (
            <button key={k} className={kind === k ? "active" : ""} onClick={() => setKind(k)}>
              {k === "all" ? "All" : k === "screenshot" ? "📸 Screenshots" : "🎬 Reels"}
            </button>
          ))}
        </div>
        <ScanControls />
        <button className="btn" onClick={() => setImporting(true)}>🎬 Import Reels</button>
      </div>

      <LibraryProgress />

      <div className="toolbar">
        <input style={{ width: 280 }} placeholder="Search text, captions, transcripts, creators…" value={search} onChange={(e) => setSearch(e.target.value)} />
        <div className="segmented">
          {PRIMARY.map((v) => (
            <button key={v.key} className={view === v.key ? "active" : ""} onClick={() => setView(v.key)}>
              {v.label}{counts?.[v.key] !== undefined && <span className="tab-count">{counts[v.key].toLocaleString()}</span>}
            </button>
          ))}
        </div>
        <details className="menu filter-more" key={view}>
          <summary className={`chip-btn ${MORE.some((m) => m.key === view) ? "active" : ""}`}>
            {MORE.find((m) => m.key === view)?.label ?? "More"}
            {MORE.some((m) => m.key === view) && counts && <span className="tab-count">{counts.current.toLocaleString()}</span>} ▾
          </summary>
          <div className="menu-items card more-menu" style={{ left: 0, right: "auto" }}>
            {(["Travel", "Other"] as const).map((section) => (
              <div key={section} className="more-section">
                <h5>{section}</h5>
                {MORE.filter((m) => m.section === section)
                  // "Not read yet" only when something is actually waiting.
                  .filter((m) => m.key !== "pending" || (moreCounts?.pending ?? 0) > 0 || view === "pending")
                  .map((m) => (
                    <button key={m.key} className={`more-item ${view === m.key ? "active" : ""}`} onClick={() => setView(m.key)} title={m.hint}>
                      <span className="grow">{m.label}</span>
                      {moreCounts?.[m.key] !== undefined && <span className="tab-count">{moreCounts[m.key].toLocaleString()}</span>}
                    </button>
                  ))}
              </div>
            ))}
          </div>
        </details>
        {(view === "failed" || view === "needsReview") && shown.length > 0 && (
          <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess(view === "failed" ? "failed" : "needsReview"))}>
            Reprocess all
          </button>
        )}
      </div>

      {error && <p className="bad">{error}</p>}
      {shown.length === 0 ? (
        <Empty icon="📸" title="Nothing here yet">
          Scan your Photos library for screenshots, or paste an Instagram Reel link with “Import Reel”.
        </Empty>
      ) : (
        <div className="grid small-tiles">
          {shown.map((item) => {
            if (item.kind === "reel") {
              const r = item.reel;
              const p = PROCESSING[r.status];
              return (
                <div key={`r-${r.id}`} className="tile portrait" onClick={() => nav.openReel(r.id, undefined, list)}>
                  <span className="kind-badge">🎬 {r.durationSec ? formatTime(r.durationSec) : "Reel"}</span>
                  <Thumb path={r.thumbnailPath} fallback="🎬" />
                  <div className="tile-body">
                    <span className="small strong tile-title">{r.creator ?? "Instagram Reel"}</span>
                    <span className="muted small tile-title">{r.caption ?? r.url}</span>
                    <div className="row wrap">
                      {r.status === "needsReview" && <Pill tone={p.tone}>{p.label}</Pill>}
                      {r.placeCount > 0 && <span className="muted small">{r.placeCount} 📍</span>}
                      {r.transcript.length > 0 && <span className="muted small" title="Voice transcript saved">🎙️</span>}
                    </div>
                  </div>
                </div>
              );
            }
            const s = item.screenshot;
            const p = PROCESSING[s.status];
            return (
              <div key={`s-${s.id}`} className="tile portrait" onClick={() => nav.openScreenshot(s.id, undefined, list)}>
                <span className="kind-badge">📸</span>
                <Thumb path={s.thumbnailPath} />
                <div className="tile-body">
                  <span className="small">{formatDate(s.creationDate)}</span>
                  <span className="muted small">{SOURCE[s.sourceType]}{s.creator ? ` · ${s.creator}` : ""}</span>
                  <div className="row wrap">
                    {s.status === "needsReview" && <Pill tone={p.tone}>{p.label}</Pill>}
                    {s.placeCount > 0 && <span className="muted small">{s.placeCount} 📍</span>}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {kind !== "reel" && counts && counts.current > shownShots && (
        <div className="show-more">
          <span className="muted small">Showing {shownShots.toLocaleString()} of {counts.current.toLocaleString()} screenshots</span>
          <button className="btn" onClick={() => setLimit((n) => n + PAGE)}>Show more</button>
          <button className="btn ghost" onClick={() => setLimit(counts.current)}>Show all</button>
        </div>
      )}
      {importing && <ImportReelDialog onClose={() => setImporting(false)} />}
    </div>
  );
}
