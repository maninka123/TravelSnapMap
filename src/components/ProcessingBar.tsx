import { AlertCircle, Loader2, Pause, Play, Square, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { ProcessingService } from "../api/services";
import type { QueueSnapshot } from "../api/types";
import { useNav } from "../lib/nav";

/** Background work at a glance, on every screen except Import (which shows it in full). Never blocks the UI. */
export function ProcessingBar() {
  const nav = useNav();
  const { refresh } = nav;
  const [snap, setSnap] = useState<QueueSnapshot>();
  const lastRefresh = useRef(0);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    ProcessingService.status().then(setSnap).catch(() => {});
    const unlisten = ProcessingService.onProgress((s) => {
      setSnap(s);
      if (s.running) setDismissed(false);
      // Refresh views at most every 2 s while processing, and once at the end.
      const now = Date.now();
      if (!s.running || now - lastRefresh.current > 2000) { lastRefresh.current = now; refresh(); }
    });
    return () => { void unlisten.then((u) => u()); };
  }, [refresh]);

  if (!snap || dismissed) return null;

  // Idle with unfinished work is shown in Import and the sidebar, not as a floating bar.
  if (!snap.running && !snap.lastError) return null;
  const pct = snap.total ? Math.round((snap.processed / snap.total) * 100) : 0;
  return (
    <div className="processing-bar" role="status" aria-live="polite">
      <span className={`pb-icon ${snap.lastError && !snap.running ? "bad" : ""}`}>
        {snap.running ? (snap.paused ? <Pause size={16} /> : <Loader2 size={16} style={{ animation: "spin 1s linear infinite" }} />) : <AlertCircle size={16} />}
      </span>
      <div className="pb-main">
        <div className="pb-title">{snap.running ? (snap.paused ? "Paused" : "Reading screenshots") : snap.lastError}</div>
        {snap.total > 0 && (
          <div className="pb-stats">
            <span>{snap.processed.toLocaleString()} of {snap.total.toLocaleString()}</span>
            {snap.travel > 0 && <span>{snap.travel} travel</span>}
            {snap.needsReview > 0 && <span className="warn-text">{snap.needsReview} to review</span>}
            {snap.failed > 0 && <span className="bad">{snap.failed} failed</span>}
            {snap.waiting > 0 && <span>{snap.waiting} waiting for iCloud</span>}
          </div>
        )}
        {snap.running && <div className={`progress ${snap.total ? "" : "indeterminate"}`}><div style={{ width: `${pct}%` }} /></div>}
      </div>
      <div className="pb-actions">
        {snap.running && !snap.paused && <button className="btn small" onClick={() => ProcessingService.pause()}><Pause size={13} /> Pause</button>}
        {snap.running && snap.paused && <button className="btn small primary" onClick={() => ProcessingService.resume()}><Play size={13} /> Resume</button>}
        {snap.running && <button className="btn small" onClick={() => ProcessingService.cancel()}><Square size={11} /> Stop</button>}
        <button className="btn small ghost" onClick={() => nav.go("import", { import: "add" })}>Details</button>
        {!snap.running && <button className="icon-btn" aria-label="Hide" onClick={() => setDismissed(true)}><X size={15} /></button>}
      </div>
    </div>
  );
}
