import { AppService, ProcessingService } from "../api/services";
import { useAction, useLoad } from "../lib/nav";
import { ErrorNote } from "./common";

const DONE = ["complete", "notTravel", "needsReview", "ignored"];

/** "Photos library: 1,946 screenshots · 14% processed" with a way to continue to 100%. */
export function LibraryProgress() {
  const { data: overview } = useLoad(() => AppService.overview(), []);
  const { data: preview } = useLoad(() => ProcessingService.scanPreview(), []);
  const action = useAction();
  if (!overview) return null;

  const counts = overview.screenshotCounts ?? {};
  const known = Object.values(counts).reduce((a, b) => a + b, 0);
  const done = DONE.reduce((n, s) => n + (counts[s] ?? 0), 0);
  const failed = counts.failed ?? 0;
  const inSources = (preview?.photos.total ?? 0) + (preview?.folders ?? []).reduce((n, f) => n + (f.summary.total ?? 0), 0);
  const total = Math.max(known, inSources);
  if (total === 0) return null;

  const remaining = Math.max(0, total - done - failed);
  const pct = Math.floor((done / total) * 100);
  const running = overview.queue.running;

  return (
    <div className="library-progress">
      <div className="lp-text">
        <div>
          <span className="strong">{preview?.folders.length ? "Screenshot library" : "Photos library"}</span>
          <span className="muted"> · {total.toLocaleString()} screenshots · </span>
          <span className="strong">{pct}% processed</span>
        </div>
        <div className="muted small">
          {done.toLocaleString()} done
          {remaining > 0 && ` · ${remaining.toLocaleString()} to go`}
          {failed > 0 && ` · ${failed.toLocaleString()} couldn't be read`}
        </div>
      </div>
      <div className="lp-bar" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
        <div style={{ width: `${(done / total) * 100}%` }} />
      </div>
      <div className="lp-actions">
        {running ? (
          <span className="muted small">Processing…</span>
        ) : remaining > 0 ? (
          <button className="btn primary small" disabled={action.busy} onClick={() => action.run(() => ProcessingService.start({ kind: "scanNew" }))}>
            Continue to 100%
          </button>
        ) : failed === 0 ? (
          <span className="small" style={{ color: "var(--ok)" }}>✓ All processed</span>
        ) : null}
        {!running && failed > 0 && (
          <button className="btn small" disabled={action.busy} onClick={() => action.run(() => ProcessingService.reprocess("failed"))}>
            Retry {failed.toLocaleString()} failed
          </button>
        )}
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />
    </div>
  );
}
