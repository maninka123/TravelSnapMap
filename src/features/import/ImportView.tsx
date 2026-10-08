import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle, CheckCircle2, Clapperboard, FileImage, FolderOpen, Image as ImageIcon, Images, Loader2, Pause, Play, RotateCcw, Square, Upload, Video,
} from "lucide-react";
import { useEffect, useState } from "react";
import { AppService, PhotoLibraryService, ProcessingService, SettingsService } from "../../api/services";
import type { QueueSnapshot, RunReport } from "../../api/types";
import { ErrorNote, Modal, plural } from "../../components/common";
import { RunReportView } from "../../components/ScanControls";
import { Segmented, useToast } from "../../components/ui";
import { formatDate } from "../../lib/labels";
import { useAction, useLoad, useNav, type ImportTab } from "../../lib/nav";
import { importDroppedPaths } from "../../lib/dropImport";
import { ImportReelDialog, PhotosVideoPicker } from "../reels/ImportReelDialog";
import { ReviewView } from "../review/ReviewView";
import { SourcesView } from "../screenshots/SourcesView";

/** Import: add screenshots, folders, Reels and videos; watch them being read; answer what was uncertain; browse sources. */
export function ImportView() {
  const nav = useNav();
  const { data: overview } = useLoad(() => AppService.overview(), []);
  const reviews = overview?.openReviews ?? 0;
  return (
    <div className="page">
      <div className="page-head">
        <h1>Import</h1>
        <Segmented<ImportTab> label="Import sections" value={nav.importTab} onChange={nav.setImportTab} options={[
          { value: "add", label: "Add & progress" },
          { value: "review", label: <>Review{reviews > 0 && <span className="badge warn" style={{ marginLeft: 2 }}>{reviews}</span>}</> },
          { value: "library", label: "Sources" },
        ]} />
      </div>
      {nav.importTab === "add" && <AddTab />}
      {nav.importTab === "review" && <ReviewView />}
      {nav.importTab === "library" && <SourcesView />}
    </div>
  );
}

const DONE = ["complete", "notTravel", "needsReview", "ignored"];

function AddTab() {
  const nav = useNav();
  const toast = useToast();
  const action = useAction();
  const { data: preview, reload: reloadPreview } = useLoad(() => ProcessingService.scanPreview(), []);
  const { data: permission, reload: reloadPermission } = useLoad(() => PhotoLibraryService.permissionStatus(), []);
  const { data: settings } = useLoad(() => SettingsService.get(), []);
  const [dialog, setDialog] = useState<"reels" | "video" | null>(null);
  const [dragOver, setDragOver] = useState(false);
  useEffect(() => {
    const on = (e: Event) => setDragOver(!!(e as CustomEvent<boolean>).detail);
    window.addEventListener("tsm:dragover", on);
    return () => window.removeEventListener("tsm:dragover", on);
  }, []);

  const photosGranted = permission === "authorized" || permission === "limited";
  const photosNew = preview?.photos.new ?? 0;
  const folderNew = (preview?.folders ?? []).reduce((n, f) => n + (f.summary.new ?? 0), 0);
  const folders = settings?.config.screenshotFolders ?? [];

  const chooseImages = async () => {
    const picked = await open({ multiple: true, directory: false, title: "Choose screenshots to import",
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "heic", "heif", "webp", "tiff", "tif"] }] });
    const paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (!paths.length) return;
    const r = await action.run(() => importDroppedPaths(paths));
    if (r) toast.ok(r.added ? `${plural(r.added, "screenshot")} added${r.alreadyImported ? `, ${r.alreadyImported} already imported` : ""}.` : "Those screenshots are already in your library.");
  };
  const importFolder = async () => {
    const dir = await open({ directory: true, multiple: false, title: "Choose a folder of screenshots" });
    if (typeof dir === "string") {
      if (await action.run(() => ProcessingService.addFolder(dir)) !== undefined) toast.ok("Folder added — new screenshots in it are being read.");
      reloadPreview();
    }
  };


  return (
    <div className="import-grid">
      <div className="stack-l">
        <div className={`dropzone ${dragOver ? "drag-over" : ""}`} aria-label="Drop screenshots here">
          <div className="dropzone-art"><Upload size={28} /></div>
          <div className="grow">
            <h3>Drop screenshots anywhere in this window</h3>
            <p className="muted">PNG, JPEG, HEIC and more. Each picture is imported once — dropping it again changes nothing.</p>
          </div>
          <button className="btn primary" onClick={chooseImages} disabled={action.busy}><FileImage size={15} /> Choose images…</button>
        </div>
        <ErrorNote error={action.error} onClose={action.clearError} />

        <div>
          <h3 style={{ marginBottom: 2 }}>Sources</h3>
          <p className="muted small">Only items that aren't in TravelSnapMap yet are read. Text is recognised on your Mac; see Settings → Privacy for what is sent to the AI.</p>
          <div className="source-cards">
            <SourceCard icon={<Images size={18} />} color="#e8590c" title="Apple Photos"
              text={!photosGranted ? "Read the screenshots in your Photos library. TravelSnapMap asks for access first."
                : preview?.photos.error ? `Photos isn't available: ${preview.photos.error}`
                : `${(preview?.photos.total ?? 0).toLocaleString()} screenshots · ${photosNew.toLocaleString()} new`}>
              {photosGranted ? (
                <button className="btn primary small" disabled={action.busy || photosNew + folderNew === 0}
                        onClick={() => action.run(() => ProcessingService.start({ kind: "scanNew" }))}>
                  <Play size={13} /> {photosNew + folderNew > 0 ? `Read ${(photosNew + folderNew).toLocaleString()} new` : "Nothing new"}
                </button>
              ) : (
                <button className="btn primary small" onClick={async () => { await action.run(() => PhotoLibraryService.requestPermission()); reloadPermission(); reloadPreview(); }}>
                  Allow access to Photos
                </button>
              )}
            </SourceCard>
            <SourceCard icon={<FolderOpen size={18} />} color="#1c7ed6" title="Screenshot folder"
              text={folders.length ? `${plural(folders.length, "folder")} watched · ${folderNew.toLocaleString()} new. Screenshots from any phone or computer.` : "Screenshots from any phone or computer: choose a folder of images."}>
              <button className="btn small" onClick={importFolder}><FolderOpen size={13} /> Add folder…</button>
            </SourceCard>
            <SourceCard icon={<Clapperboard size={18} />} color="#c2255c" title="Instagram Reels"
              text="Paste one link or a whole list. Speech and on-screen text are read on your Mac, and each place keeps the moment it's mentioned.">
              <button className="btn small" onClick={() => setDialog("reels")}>Paste links…</button>
            </SourceCard>
            <SourceCard icon={<Video size={18} />} color="#5f3dc4" title="Video from Photos"
              text="If Instagram doesn't share a video, save it to Photos (or record your screen) and import it from there.">
              <button className="btn small" onClick={() => setDialog("video")}>Choose a video…</button>
            </SourceCard>
          </div>
        </div>

        <RecentRuns />
      </div>
      <div className="stack-l"><StatusCard onReview={() => nav.setImportTab("review")} /></div>
      {dialog === "reels" && <ImportReelDialog onClose={() => setDialog(null)} />}
      {dialog === "video" && <PhotosVideoPicker reelId={null} onClose={() => setDialog(null)} />}
    </div>
  );
}

function SourceCard({ icon, color, title, text, children }: { icon: React.ReactNode; color: string; title: string; text: string; children: React.ReactNode }) {
  return (
    <div className="source-card">
      <div className="source-card-head"><span className="source-card-icon" style={{ background: color }}>{icon}</span><h4>{title}</h4></div>
      <p>{text}</p>
      {children}
    </div>
  );
}

/** What's happening now, and the state of the whole library: done, places found, to review, couldn't read. */
function StatusCard({ onReview }: { onReview: () => void }) {
  const nav = useNav();
  const action = useAction();
  const [snap, setSnap] = useState<QueueSnapshot>();
  const { data: overview } = useLoad(() => AppService.overview(), []);
  const { data: diag } = useLoad(() => AppService.diagnostics(), []);
  useEffect(() => {
    ProcessingService.status().then(setSnap).catch(() => {});
    const unlisten = ProcessingService.onProgress(setSnap);
    return () => { void unlisten.then((u) => u()); };
  }, []);

  const counts = overview?.screenshotCounts ?? {};
  const total = Object.values(counts).reduce((a, b) => a + b, 0);
  const done = DONE.reduce((n, k) => n + (counts[k] ?? 0), 0);
  const failed = counts.failed ?? 0;
  const waiting = counts.waitingForNetwork ?? 0;
  const unfinished = Math.max(0, total - done - failed);
  const running = !!snap?.running;
  const pct = snap?.total ? Math.round((snap.processed / snap.total) * 100) : total ? Math.round((done / total) * 100) : 0;

  return (
    <div className="card status-card" aria-live="polite">
      <div className="status-card-head">
        <span className={`pb-icon ${snap?.lastError ? "bad" : ""}`}>
          {running ? (snap?.paused ? <Pause size={17} /> : <Loader2 size={17} className="spin" style={{ animation: "spin 1s linear infinite" }} />)
            : snap?.lastError ? <AlertCircle size={17} /> : <CheckCircle2 size={17} />}
        </span>
        <div className="grow">
          <div className="strong">{running ? (snap?.paused ? "Paused" : "Reading your screenshots") : snap?.lastError ? "Stopped" : unfinished > 0 ? "Not finished" : total ? "Up to date" : "Nothing imported yet"}</div>
          <div className="muted small truncate">{running ? snap?.phase : snap?.lastError ?? (unfinished > 0 ? `${unfinished.toLocaleString()} still to read` : total ? "Everything has been read" : "Add a source to begin")}</div>
        </div>
      </div>
      {(running || total > 0) && (
        <>
          <div className="progress" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}><div style={{ width: `${pct}%` }} /></div>
          <div className="muted small tabular">
            {running && snap?.total ? `This run: ${snap.processed.toLocaleString()} of ${snap.total.toLocaleString()} · ${snap.travel} travel · ${snap.notTravel} not travel` : `${done.toLocaleString()} of ${total.toLocaleString()} read · ${pct}%`}
          </div>
        </>
      )}
      <div className="row wrap">
        {running && !snap?.paused && <button className="btn small" onClick={() => action.run(() => ProcessingService.pause())}><Pause size={13} /> Pause</button>}
        {running && snap?.paused && <button className="btn small primary" onClick={() => action.run(() => ProcessingService.resume())}><Play size={13} /> Resume</button>}
        {running && <button className="btn small" onClick={() => action.run(() => ProcessingService.cancel())}><Square size={12} /> Stop</button>}
        {!running && unfinished > 0 && <button className="btn small primary" onClick={() => action.run(() => ProcessingService.start({ kind: "scanNew" }))}><Play size={13} /> Continue</button>}
        {!running && failed > 0 && <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess("failed"))}><RotateCcw size={13} /> Retry {failed.toLocaleString()} failed</button>}
      </div>
      <div className="stat-row">
        <button className="stat ok" onClick={() => nav.go("explore", { explore: "library" })}><div className="stat-value">{(overview?.places ?? 0).toLocaleString()}</div><div className="stat-label">Places found</div></button>
        <button className={`stat ${(overview?.openReviews ?? 0) > 0 ? "warn" : ""}`} onClick={onReview}><div className="stat-value">{(overview?.openReviews ?? 0).toLocaleString()}</div><div className="stat-label">To review</div></button>
        <button className={`stat ${failed ? "bad" : ""}`} onClick={() => nav.setImportTab("library")}><div className="stat-value">{failed.toLocaleString()}</div><div className="stat-label">Couldn't read</div></button>
        {waiting > 0 && <div className="stat"><div className="stat-value">{waiting.toLocaleString()}</div><div className="stat-label">Waiting for network</div></div>}
        <div className="stat"><div className="stat-value">{diag ? usd(diag.aiEstimatedCost) : "—"}</div><div className="stat-label" title="Estimated from exact token counts; DeepSeek bills from its own records">AI cost so far</div></div>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />
    </div>
  );
}

const usd = (v: number) => (v === 0 ? "$0" : v < 0.01 ? `$${v.toFixed(4)}` : `$${v.toFixed(2)}`);

function RecentRuns() {
  const { data: runs = [] } = useLoad(() => ProcessingService.runs(), []);
  const [report, setReport] = useState<RunReport>();
  if (runs.length === 0) return null;
  return (
    <div>
      <h3 style={{ marginBottom: 8 }}>Recent imports</h3>
      <div className="list">
        {runs.slice(0, 6).map((r) => (
          <div key={r.id} className="list-row clickable" role="button" tabIndex={0}
               onClick={async () => setReport((await ProcessingService.runReport(r.id)) ?? undefined)}>
            <ImageIcon size={16} className="muted" />
            <div className="grow">
              <div className="strong">{r.kind === "validation" ? `Test run · ${r.sample}` : r.kind === "scanFolder" ? "Folder" : "Photos"}</div>
              <div className="muted small">{formatDate(r.startedAt)} · {plural(r.screenshotCount, "screenshot")}{r.finishedAt ? "" : " · running"}</div>
            </div>
            <span className="muted small">Report</span>
          </div>
        ))}
      </div>
      {report && <Modal title="Import report" onClose={() => setReport(undefined)} wide><RunReportView report={report} /></Modal>}
    </div>
  );
}
