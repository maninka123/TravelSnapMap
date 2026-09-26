import { useEffect, useRef, useState } from "react";
import { AppService, ProcessingService } from "../api/services";
import type { QueueSnapshot } from "../api/types";
import { useLoad, useNav } from "../lib/nav";

const DONE = ["complete", "notTravel", "needsReview", "ignored"];

/** Always-visible progress for the background pipeline. Never blocks the UI. */
export function ProcessingBar() {
  const { refresh } = useNav();
  const [snap, setSnap] = useState<QueueSnapshot>();
  const lastRefresh = useRef(0);
  const [dismissed, setDismissed] = useState(false);
  // Whole-library progress (refreshed every ~2 s while processing), so the bar continues from where you are.
  const { data: overview } = useLoad(() => AppService.overview(), []);

  useEffect(() => {
    ProcessingService.status().then(setSnap).catch(() => {});
    const unlisten = ProcessingService.onProgress((s) => {
      setSnap(s);
      // Refresh views at most every 2 s while processing, and once at the end.
      const now = Date.now();
      if (!s.running || now - lastRefresh.current > 2000) {
        lastRefresh.current = now;
        refresh();
      }
    });
    return () => { void unlisten.then((u) => u()); };
  }, [refresh]);

  const counts = overview?.screenshotCounts ?? {};
  const libraryTotal = Object.values(counts).reduce((a, b) => a + b, 0);
  const libraryDone = DONE.reduce((n, k) => n + (counts[k] ?? 0), 0);
  const unfinished = libraryTotal - libraryDone - (counts.failed ?? 0);

  // Nothing running (e.g. the app was quit mid-scan): offer to continue instead of hiding the progress.
  if (snap && !snap.running && !snap.lastError && unfinished > 0 && !dismissed) {
    const donePct = libraryTotal ? Math.round((libraryDone / libraryTotal) * 100) : 0;
    return (
      <div className="processing-bar">
        <div className="pb-main">
          <div className="pb-title">{unfinished.toLocaleString()} screenshots not finished yet</div>
          <div className="pb-stats"><span className="strong">{libraryDone.toLocaleString()} of {libraryTotal.toLocaleString()} done · {donePct}%</span></div>
          <div className="progress"><div style={{ width: `${donePct}%` }} /></div>
        </div>
        <div className="pb-actions">
          <button className="btn primary" onClick={() => ProcessingService.start({ kind: "scanNew" })}>Continue</button>
          <button className="btn" onClick={() => setDismissed(true)}>Later</button>
        </div>
      </div>
    );
  }
  if (!snap || (!snap.running && snap.total === 0 && !snap.lastError)) return null;
  const pct = libraryTotal ? Math.round((libraryDone / libraryTotal) * 100) : snap.total ? Math.round((snap.processed / snap.total) * 100) : 0;

  return (
    <div className="processing-bar">
      <div className="pb-main">
        <div className="pb-title">
          {snap.running ? (snap.paused ? "⏸ Paused" : "⏳ Scanning screenshots") : snap.lastError ? "⚠️ " + snap.lastError : "✓ " + snap.phase}
        </div>
        <div className="pb-stats">
          <span className="strong">
            {libraryTotal ? `${libraryDone.toLocaleString()} of ${libraryTotal.toLocaleString()} screenshots done · ${pct}%` : `${snap.processed} of ${snap.total} done`}
          </span>
        </div>
        {snap.total > 0 && (
          <div className="pb-note">
            This run: {snap.processed.toLocaleString()} of {snap.total.toLocaleString()}
            {snap.processed > 0 && <> · {snap.travel} travel{snap.needsReview > 0 ? ` (${snap.needsReview} to review)` : ""} · {snap.notTravel} not travel</>}
            {snap.failed > 0 && <span className="bad"> · {snap.failed} failed</span>}
            {snap.waiting > 0 && <> · {snap.waiting} waiting for iCloud</>}
          </div>
        )}
        {snap.running && <div className="progress"><div style={{ width: `${pct}%` }} /></div>}
      </div>
      <div className="pb-actions">
        {snap.running && !snap.paused && <button className="btn" onClick={() => ProcessingService.pause()}>Pause</button>}
        {snap.running && snap.paused && <button className="btn" onClick={() => ProcessingService.resume()}>Resume</button>}
        {snap.running && <button className="btn" onClick={() => ProcessingService.cancel()}>Stop</button>}
      </div>
    </div>
  );
}
