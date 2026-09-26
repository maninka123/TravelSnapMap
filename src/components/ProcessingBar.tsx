import { useEffect, useRef, useState } from "react";
import { ProcessingService } from "../api/services";
import type { QueueSnapshot } from "../api/types";
import { useNav } from "../lib/nav";

/** Always-visible progress for the background pipeline. Never blocks the UI. */
export function ProcessingBar() {
  const { refresh } = useNav();
  const [snap, setSnap] = useState<QueueSnapshot>();
  const lastRefresh = useRef(0);

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

  if (!snap || (!snap.running && snap.total === 0 && !snap.lastError)) return null;
  const pct = snap.total ? Math.round((snap.processed / snap.total) * 100) : 0;

  return (
    <div className="processing-bar">
      <div className="pb-main">
        <div className="pb-title">
          {snap.running ? (snap.paused ? "⏸ Paused" : "⏳ Scanning screenshots") : snap.lastError ? "⚠️ " + snap.lastError : "✓ " + snap.phase}
        </div>
        <div className="pb-stats">
          {snap.inPhotos > 0 && <span>{snap.inPhotos.toLocaleString()} screenshots · {snap.alreadyKnown.toLocaleString()} known · {snap.newlyDiscovered.toLocaleString()} new</span>}
          <span>Processed {snap.processed}/{snap.total}</span>
          <span>Travel {snap.travel}</span>
          <span>Not travel {snap.notTravel}</span>
          {snap.needsReview > 0 && <span>Needs review {snap.needsReview}</span>}
          {snap.failed > 0 && <span className="bad">Failed {snap.failed}</span>}
          {snap.waiting > 0 && <span>Waiting {snap.waiting}</span>}
          {snap.running && <span>Remaining {snap.remaining}</span>}
        </div>
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
