import { ChevronLeft, ChevronRight, Compass, Download, Map as MapIcon, Settings as SettingsIcon, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { AppService, isTauri } from "./api/services";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ProcessingBar } from "./components/ProcessingBar";
import { ToastProvider, useToast } from "./components/ui";
import { ExploreView } from "./features/explore/ExploreView";
import { ImportView } from "./features/import/ImportView";
import { Onboarding, shouldShowOnboarding } from "./features/onboarding/Onboarding";
import { PlaceDetail } from "./features/places/PlaceDetail";
import { ReelDetail } from "./features/reels/ReelDetail";
import { ScreenshotDetail } from "./features/screenshots/ScreenshotDetail";
import { SettingsView } from "./features/settings/SettingsView";
import { TripsView } from "./features/trips/TripsView";
import { NavProvider, useLoad, useNav, type View } from "./lib/nav";
import { applyTheme } from "./lib/theme";
import { importDroppedPaths, onFileDrop } from "./lib/dropImport";

const NAV: { view: View; icon: typeof MapIcon; label: string; key: string }[] = [
  { view: "explore", icon: Compass, label: "Explore", key: "1" },
  { view: "import", icon: Download, label: "Import", key: "2" },
  { view: "trips", icon: MapIcon, label: "My Trips", key: "3" },
];

function BrandMark() {
  return (
    <svg width="26" height="26" viewBox="0 0 26 26" aria-hidden="true">
      <defs><linearGradient id="bm" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="#1aa39a" /><stop offset="1" stopColor="#0b5f86" /></linearGradient></defs>
      <rect width="26" height="26" rx="7" fill="url(#bm)" />
      <path d="M13 5.5a5.2 5.2 0 0 0-5.2 5.2c0 3.9 5.2 9.8 5.2 9.8s5.2-5.9 5.2-9.8A5.2 5.2 0 0 0 13 5.5Z" fill="#fff" />
      <circle cx="13" cy="10.7" r="2" fill="#0b6f86" />
    </svg>
  );
}

function Shell() {
  const nav = useNav();
  const toast = useToast();
  const { data: overview } = useLoad(() => AppService.overview(), []);
  const [onboarding, setOnboarding] = useState(false);
  const checkedOnboarding = useRef(false);

  useEffect(() => {
    if (overview && !checkedOnboarding.current) {
      checkedOnboarding.current = true;
      setOnboarding(shouldShowOnboarding(overview));
    }
  }, [overview]);
  useEffect(() => {
    const open = () => setOnboarding(true);
    window.addEventListener("tsm:onboarding", open);
    return () => window.removeEventListener("tsm:onboarding", open);
  }, []);

  useEffect(() => {
    const unlisten = [AppService.onLibraryChanged(() => nav.refresh()), AppService.onDataChanged(() => nav.refresh())];
    return () => { unlisten.forEach((p) => void p.then((u) => u())); };
  }, [nav.refresh]);

  // Images dropped anywhere on the window are imported (Import shows their progress).
  useEffect(() => onFileDrop(async (paths) => {
    const r = await importDroppedPaths(paths).catch((e) => { toast.error(String(e)); return undefined; });
    if (!r) return;
    if (r.added > 0) {
      toast.ok(`${r.added} screenshot${r.added === 1 ? "" : "s"} added${r.queuedBehindScan ? " — they'll be read after the current scan" : ""}.`,
        { label: "Show", run: () => nav.go("import", { import: "add" }) });
      nav.refresh();
    } else if (r.alreadyImported > 0) toast.info("Those screenshots are already in your library.");
    else toast.error("Only images can be imported (PNG, JPEG, HEIC, WebP, TIFF). For Reels, paste links in Import.");
  }, (over) => window.dispatchEvent(new CustomEvent("tsm:dragover", { detail: over }))), [nav, toast]);

  // Mac shortcuts: ⌘1–3 destinations, ⌘, Settings, ⌘F search, ⌘I Import, ←/→ step through sources in the viewer.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = (e.target as HTMLElement)?.closest?.("input, textarea, select, [contenteditable]");
      if (!typing && !e.metaKey && !e.altKey && !e.ctrlKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
        if (nav.step(e.key === "ArrowRight" ? 1 : -1)) e.preventDefault();
        return;
      }
      if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
      const item = NAV.find((n) => n.key === e.key);
      if (item) { e.preventDefault(); nav.go(item.view); return; }
      if (e.key === ",") { e.preventDefault(); nav.go("settings"); return; }
      if (e.key.toLowerCase() === "i" && !e.shiftKey) { e.preventDefault(); nav.go("import", { import: "add" }); return; }
      if (e.key.toLowerCase() === "f") {
        const field = document.querySelector<HTMLInputElement>("[data-search]");
        if (field) { e.preventDefault(); field.focus(); field.select(); }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [nav]);

  const top = nav.panels[nav.panels.length - 1];
  // Symmetric motion: the sheet leaves the way it came in before it unmounts.
  const [closing, setClosing] = useState(false);
  const dismiss = (all: boolean) => {
    if (!all && nav.panels.length > 1) { nav.back(); return; }
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    setClosing(true);
    window.setTimeout(() => { setClosing(false); nav.closePanels(); }, reduced ? 120 : 200);
  };
  useEffect(() => {
    if (!top) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector(".modal, .menu-content, .popover-content")) dismiss(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }); // eslint-disable-line react-hooks/exhaustive-deps

  const reviews = overview?.openReviews ?? 0;
  const counts = overview?.screenshotCounts ?? {};
  const total = Object.values(counts).reduce((a, b) => a + b, 0);
  const running = overview?.queue.running;
  const unfinished = total - ["complete", "notTravel", "needsReview", "ignored", "failed"].reduce((n, k) => n + (counts[k] ?? 0), 0);
  const placesOnMap = overview ? overview.places - overview.unconfirmedPlaces : 0;

  return (
    <div className="app">
      <aside className="sidebar" aria-label="Sidebar">
        <div className="brand"><span className="brand-mark"><BrandMark /></span><span>TravelSnapMap</span></div>
        <nav aria-label="Main" className="col" style={{ gap: 2 }}>
          {NAV.map((n) => (
            <button key={n.view} className={`nav-item ${nav.view === n.view ? "active" : ""}`} onClick={() => nav.go(n.view)}
                    aria-current={nav.view === n.view ? "page" : undefined} title={`${n.label} (⌘${n.key})`}>
              <n.icon size={17} strokeWidth={2} aria-hidden="true" />
              <span className="nav-label">{n.label}</span>
              {n.view === "import" && reviews > 0 && <span className="badge warn" aria-label={`${reviews} to review`}>{reviews}</span>}
              <span className="kbd">⌘{n.key}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          {overview && total > 0 && (
            <button className="sidebar-stat" style={{ border: 0, textAlign: "left", cursor: "pointer" }} onClick={() => nav.go("import", { import: "add" })}>
              <span><strong>{placesOnMap.toLocaleString()}</strong> place{placesOnMap === 1 ? "" : "s"} saved</span>
              {running ? <span>Reading screenshots…</span> : unfinished > 0 ? <span className="warn-text">{unfinished.toLocaleString()} not read yet</span> : <span>{total.toLocaleString()} sources</span>}
              {running && <div className="progress indeterminate"><div /></div>}
            </button>
          )}
          <button className={`nav-item ${nav.view === "settings" ? "active" : ""}`} onClick={() => nav.go("settings")} title="Settings (⌘,)"
                  aria-current={nav.view === "settings" ? "page" : undefined}>
            <SettingsIcon size={17} aria-hidden="true" /><span className="nav-label">Settings</span><span className="kbd">⌘,</span>
          </button>
        </div>
      </aside>

      <main className="content">
        <ErrorBoundary key={nav.view} label={NAV.find((n) => n.view === nav.view)?.label ?? "Settings"}>
          {nav.view === "explore" && <ExploreView />}
          {nav.view === "import" && <ImportView />}
          {nav.view === "trips" && <TripsView />}
          {nav.view === "settings" && <SettingsView />}
        </ErrorBoundary>

        {top && (
          <div className={closing ? "panel-backdrop closing" : "panel-backdrop"} onMouseDown={() => dismiss(true)}>
            <section className="panel" role="dialog" aria-modal="true" aria-label="Details" onMouseDown={(e) => e.stopPropagation()}>
              <div className="panel-nav">
                <button className="btn ghost small" onClick={() => dismiss(false)}><ChevronLeft size={16} /> {nav.panels.length > 1 ? "Back" : "Close"}</button>
                {top.list && top.list.length > 1 && (() => {
                  const i = top.list.findIndex((r) => r.id === top.id && r.type === top.type);
                  return (
                    <span className="panel-stepper">
                      <button className="icon-btn" disabled={i <= 0} onClick={() => nav.step(-1)} aria-label="Previous (←)" title="Previous (←)"><ChevronLeft size={17} /></button>
                      <span className="muted small">{i + 1} of {top.list.length}</span>
                      <button className="icon-btn" disabled={i >= top.list.length - 1} onClick={() => nav.step(1)} aria-label="Next (→)" title="Next (→)"><ChevronRight size={17} /></button>
                    </span>
                  );
                })()}
                <button className="close-btn" style={top.list && top.list.length > 1 ? undefined : { marginLeft: "auto" }} onClick={() => dismiss(true)} aria-label="Close"><X size={14} /></button>
              </div>
              <ErrorBoundary key={top.id} label="This page">
                {top.type === "place" && <PlaceDetail key={top.id} id={top.id} />}
                {top.type === "screenshot" && <ScreenshotDetail key={top.id} id={top.id} highlight={top.highlight} />}
                {top.type === "reel" && <ReelDetail key={top.id} id={top.id} seek={top.seek} />}
              </ErrorBoundary>
            </section>
          </div>
        )}
        {!(nav.view === "import" && nav.importTab === "add") && <ProcessingBar />}
      </main>
      {onboarding && <Onboarding onDone={() => { setOnboarding(false); nav.go("explore"); nav.refresh(); }} />}
    </div>
  );
}

export default function App() {
  useEffect(() => { applyTheme(); }, []);
  if (!isTauri) {
    return (
      <div className="empty" style={{ height: "100vh" }}>
        <h3>TravelSnapMap runs inside its desktop app</h3>
        <p className="muted">Start it with <code>npm run tauri dev</code>, or preview the interface with <code>npm run demo</code>.</p>
      </div>
    );
  }
  return (
    <NavProvider>
      <ToastProvider>
        <Shell />
      </ToastProvider>
    </NavProvider>
  );
}

