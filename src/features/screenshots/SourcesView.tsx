import { useState } from "react";
import { LibraryService, ProcessingService } from "../../api/services";
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
const MORE: { key: ScreenshotView; label: string }[] = [
  { key: "processed", label: "Processed" },
  { key: "multiplePlaces", label: "Multiple places" },
  { key: "noPlace", label: "No place found" },
  { key: "lowConfidence", label: "Low confidence" },
  { key: "pending", label: "Pending" },
  { key: "failed", label: "Failed" },
  { key: "ignored", label: "Ignored" },
];

/** The library of evidence: travel screenshots and Instagram Reels side by side. */
export function SourcesView() {
  const nav = useNav();
  const [view, setView] = useState<ScreenshotView>("all");
  const [kind, setKind] = useState<"all" | "screenshot" | "reel">("all");
  const [search, setSearch] = useState("");
  const [importing, setImporting] = useState(false);
  const action = useAction();
  const { data: items = [], error } = useLoad(() => LibraryService.items(view, search), [view, search]);
  const shown = items.filter((i) => kind === "all" || i.kind === kind);
  // The viewer steps through exactly what's shown here, in this order.
  const list = shown.map((i) => (i.kind === "reel" ? { type: "reel" as const, id: i.reel.id } : { type: "screenshot" as const, id: i.screenshot.id }));

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
            <button key={v.key} className={view === v.key ? "active" : ""} onClick={() => setView(v.key)}>{v.label}</button>
          ))}
        </div>
        <details className="menu filter-more" key={view}>
          <summary className={`chip-btn ${MORE.some((m) => m.key === view) ? "active" : ""}`}>
            {MORE.find((m) => m.key === view)?.label ?? "More"} ▾
          </summary>
          <div className="menu-items card" style={{ left: 0, right: "auto" }}>
            {MORE.map((m) => (
              <button key={m.key} className="btn ghost" onClick={() => setView(m.key)}>{m.label}</button>
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
                      <Pill tone={p.tone}>{p.label}</Pill>
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
                    <Pill tone={p.tone}>{p.label}</Pill>
                    {s.placeCount > 0 && <span className="muted small">{s.placeCount} 📍</span>}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {importing && <ImportReelDialog onClose={() => setImporting(false)} />}
    </div>
  );
}
