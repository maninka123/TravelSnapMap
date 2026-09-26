import { useEffect, useState } from "react";
import { AppService, isTauri } from "./api/services";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ProcessingBar } from "./components/ProcessingBar";
import { MapView } from "./features/map/MapView";
import { PlaceDetail } from "./features/places/PlaceDetail";
import { PlacesView } from "./features/places/PlacesView";
import { ReviewView } from "./features/review/ReviewView";
import { ScreenshotDetail } from "./features/screenshots/ScreenshotDetail";
import { SourcesView } from "./features/screenshots/SourcesView";
import { ReelDetail } from "./features/reels/ReelDetail";
import { SettingsView } from "./features/settings/SettingsView";
import { TripsView } from "./features/trips/TripsView";
import { NavProvider, useLoad, useNav, type View } from "./lib/nav";

const NAV: { view: View; icon: string; label: string }[] = [
  { view: "map", icon: "🗺️", label: "Map" },
  { view: "places", icon: "📍", label: "Places" },
  { view: "screenshots", icon: "📸", label: "Sources" },
  { view: "review", icon: "✅", label: "Review" },
  { view: "trips", icon: "✈️", label: "Trips" },
  { view: "settings", icon: "⚙️", label: "Settings" },
];

function Shell() {
  const nav = useNav();
  const { data: overview } = useLoad(() => AppService.overview(), []);

  useEffect(() => {
    const unlisten = [AppService.onLibraryChanged(() => nav.refresh()), AppService.onDataChanged(() => nav.refresh())];
    return () => { unlisten.forEach((p) => void p.then((u) => u())); };
  }, [nav.refresh]);

  const top = nav.panels[nav.panels.length - 1];
  // Symmetric motion: the sheet leaves the way it came in (to the right) before it unmounts.
  const [closing, setClosing] = useState(false);
  const dismiss = (all: boolean) => {
    if (!all && nav.panels.length > 1) { nav.back(); return; }
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    setClosing(true);
    window.setTimeout(() => { setClosing(false); nav.closePanels(); }, reduced ? 150 : 220);
  };

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">📍</span>
          <span>TravelSnapMap</span>
        </div>
        <nav>
          {NAV.map((n) => (
            <button key={n.view} className={`nav-item ${nav.view === n.view ? "active" : ""}`} onClick={() => nav.go(n.view)}>
              <span className="nav-icon">{n.icon}</span>
              <span>{n.label}</span>
              {n.view === "review" && !!overview?.openReviews && <span className="badge">{overview.openReviews}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot muted small">
          {overview && <>{overview.places} places</>}
        </div>
      </aside>

      <main className="content">
        <ErrorBoundary key={nav.view} label={NAV.find((n) => n.view === nav.view)?.label}>
          {nav.view === "map" && <MapView />}
          {nav.view === "places" && <PlacesView />}
          {nav.view === "screenshots" && <SourcesView />}
          {nav.view === "review" && <ReviewView />}
          {nav.view === "trips" && <TripsView />}
          {nav.view === "settings" && <SettingsView />}
        </ErrorBoundary>

        {top && (
          <div className={closing ? "panel-backdrop closing" : "panel-backdrop"} onMouseDown={() => dismiss(true)}>
            <section className="panel" onMouseDown={(e) => e.stopPropagation()}>
              <div className="panel-nav">
                <button className="btn ghost" onClick={() => dismiss(false)}>← Back</button>
                <button className="icon-btn" onClick={() => dismiss(true)} aria-label="Close">✕</button>
              </div>
              <ErrorBoundary key={top.id} label="This page">
                {top.type === "place" && <PlaceDetail key={top.id} id={top.id} />}
                {top.type === "screenshot" && <ScreenshotDetail key={top.id} id={top.id} highlight={top.highlight} />}
                {top.type === "reel" && <ReelDetail key={top.id} id={top.id} seek={top.seek} />}
              </ErrorBoundary>
            </section>
          </div>
        )}
        <ProcessingBar />
      </main>
    </div>
  );
}

export default function App() {
  if (!isTauri) {
    return (
      <div className="empty" style={{ height: "100vh" }}>
        <h3>TravelSnapMap runs inside its desktop shell</h3>
        <p className="muted">Start it with <code>npm run tauri dev</code>.</p>
      </div>
    );
  }
  return (
    <NavProvider>
      <Shell />
    </NavProvider>
  );
}
