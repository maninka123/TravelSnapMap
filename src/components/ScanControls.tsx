import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { ProcessingService } from "../api/services";
import type { QueueSnapshot, RunReport } from "../api/types";
import { formatDate, pct, PROCESSING, SOURCE } from "../lib/labels";
import { useAction, useLoad, useNav } from "../lib/nav";
import { ErrorNote, Modal, Pill, Thumb } from "./common";

/** Main screenshot actions: Scan New Screenshots and Import Folder… (test runs live in Settings → Diagnostics). */
export function ScanControls() {
  const { data: preview, reload } = useLoad(() => ProcessingService.scanPreview(), []);
  const action = useAction();
  const newCount = (preview?.photos.new ?? 0) + (preview?.folders ?? []).reduce((n, f) => n + (f.summary.new ?? 0), 0);

  const importFolder = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Choose a folder of screenshots" });
    if (typeof dir === "string") {
      await action.run(() => ProcessingService.addFolder(dir));
      reload();
    }
  };

  return (
    <>
      <button className="btn primary" disabled={action.busy} onClick={() => action.run(() => ProcessingService.start({ kind: "scanNew" }))}
              title="Only screenshots not already in TravelSnapMap (Photos + your folders)">
        📸 Scan New Screenshots{preview ? ` (${newCount.toLocaleString()} new)` : ""}
      </button>
      <button className="btn" onClick={importFolder} title="Screenshots from any device: choose a folder of PNG/JPG/HEIC images">📁 Import Folder…</button>
      {preview?.photos.error && <span className="muted small" title={preview.photos.error}>Photos: not available</span>}
      <ErrorNote error={action.error} onClose={action.clearError} />
    </>
  );
}

/** Process a small number of real screenshots and show how the pipeline did. */
export function TestRunDialog({ onClose }: { onClose: () => void }) {
  const [count, setCount] = useState(25);
  const [random, setRandom] = useState(true);
  const [snap, setSnap] = useState<QueueSnapshot>();
  const [report, setReport] = useState<RunReport>();
  const [started, setStarted] = useState(false);
  const action = useAction();

  useEffect(() => {
    const unlisten = ProcessingService.onProgress(async (s) => {
      setSnap(s);
      if (started && !s.running && s.runId) {
        setReport((await ProcessingService.runReport(s.runId)) ?? undefined);
      }
    });
    return () => { void unlisten.then((u) => u()); };
  }, [started]);

  const start = async () => {
    setReport(undefined);
    setStarted(true);
    await action.run(() => ProcessingService.start({ kind: "validation", count, random }));
  };

  return (
    <Modal title="Test on real screenshots" onClose={onClose} wide>
      {!started && (
        <>
          <p className="muted small">
            Processes only a few screenshots you haven't processed yet, through the full pipeline (local OCR → local filter →
            DeepSeek → Apple Maps), then shows how it went. Results are saved like any other scan.
          </p>
          <div className="row wrap">
            <span className="strong">Screenshots:</span>
            {[10, 25, 50, 100].map((n) => (
              <button key={n} className={`chip-btn ${count === n ? "active" : ""}`} onClick={() => setCount(n)}>{n}</button>
            ))}
            <span className="spacer" />
            <div className="segmented">
              <button className={random ? "active" : ""} onClick={() => setRandom(true)}>Random sample</button>
              <button className={!random ? "active" : ""} onClick={() => setRandom(false)}>Newest</button>
            </div>
          </div>
          <button className="btn primary" onClick={start}>Start test run</button>
        </>
      )}
      <ErrorNote error={action.error} />
      {started && !report && snap && (
        <div className="stack">
          <div className="strong">{snap.phase}</div>
          <div className="progress"><div style={{ width: `${snap.total ? (snap.processed / snap.total) * 100 : 0}%` }} /></div>
          <div className="muted small">{snap.processed}/{snap.total} · travel {snap.travel} · not travel {snap.notTravel} · review {snap.needsReview} · failed {snap.failed}</div>
        </div>
      )}
      {report && <RunReportView report={report} />}
    </Modal>
  );
}

export function RunReportView({ report }: { report: RunReport }) {
  const nav = useNav();
  const [filter, setFilter] = useState("");
  const rows = report.rows.filter((r) => !filter || r.status === filter);
  const stats: [string, string | number, string?][] = [
    ["Screenshots", report.total],
    ["Travel detected", report.travel, "complete"],
    ["Not travel", report.notTravel, "notTravel"],
    ["Needs review", report.needsReview, "needsReview"],
    ["Failed", report.failed, "failed"],
    ["Skipped locally (no AI)", report.skippedLocally],
    ["AI requests", report.aiRequests],
    ["Places found", report.placesFound],
    ["Place resolution", report.placesExtracted ? `${report.placesAutoResolved}/${report.placesExtracted} (${pct(report.resolutionRate)})` : "—"],
    ["Avg OCR time", `${Math.round(report.avgOcrMs)} ms`],
    ["Avg AI time", report.avgAiMs ? `${(report.avgAiMs / 1000).toFixed(1)} s` : "—"],
    ["Avg total / screenshot", `${(report.avgTotalMs / 1000).toFixed(1)} s`],
    ["Estimated AI cost", `$${report.totalCost.toFixed(4)}`],
    ["Estimated AI cost per travel screenshot", report.travel ? `$${report.costPerTravelScreenshot.toFixed(5)}` : "—"],
    ["Tokens", `${report.inputTokens.toLocaleString()} in / ${report.outputTokens.toLocaleString()} out`],
  ];
  return (
    <div className="stack">
      <div className="muted small">{report.run.kind === "validation" ? `Test run · ${report.run.sample}` : "Scan"} · {formatDate(report.run.startedAt)}</div>
      <div className="stat-grid">
        {stats.map(([label, value, status]) => (
          <div key={label} className="stat" style={status ? { cursor: "pointer", outline: filter === status ? "2px solid var(--accent)" : undefined } : undefined}
               onClick={() => status && setFilter(filter === status ? "" : status)}>
            <div className="stat-value">{value}</div><div className="stat-label">{label}</div>
          </div>
        ))}
      </div>
      <div className="list">
        {rows.map((r) => {
          const p = PROCESSING[r.status] ?? PROCESSING.discovered;
          return (
            <div key={r.screenshotId} className="list-row" onClick={() => nav.openScreenshot(r.screenshotId)}>
              <Thumb path={r.thumbnailPath} />
              <div className="grow">
                <div className="row wrap"><Pill tone={p.tone}>{p.label}</Pill>
                  <span className="small">{r.places.join(", ") || <span className="muted">no place</span>}</span></div>
                <div className="muted small">{formatDate(r.creationDate)} · {SOURCE[r.sourceType]} · {r.escalationLevel === 1 ? "local only" : `AI ${pct(r.travelConfidence)}`}
                  {r.statusDetail ? ` · ${r.statusDetail}` : ""}</div>
              </div>
              <span className="muted small">{(r.pipelineMs / 1000).toFixed(1)} s{r.aiCost ? ` · est. $${r.aiCost.toFixed(5)}` : ""}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
