import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

/** Main destinations: Explore (map + library), Import (add, review, sources), My Trips; Settings is secondary. */
export type View = "explore" | "import" | "trips" | "settings";
export type ExploreMode = "map" | "library";
export type ImportTab = "add" | "review" | "library";
export type SettingsSection = "general" | "ai" | "privacy" | "sources" | "costs" | "backup" | "diagnostics" | "about";
/** Where to go inside a destination. */
export type Sub = { explore?: ExploreMode; import?: ImportTab; settings?: SettingsSection; focusPlaceId?: string; tripId?: string };

const VIEW_KEY = "app.view";
function initialView(): View {
  try {
    const v = localStorage.getItem(VIEW_KEY);
    if (v === "explore" || v === "import" || v === "trips" || v === "settings") return v;
  } catch { /* storage unavailable */ }
  return "explore";
}
function remember(key: string, value: string) {
  try { localStorage.setItem(key, value); } catch { /* per-device convenience only */ }
}
function recall<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key) as T | null;
    if (v && allowed.includes(v)) return v;
  } catch { /* storage unavailable */ }
  return fallback;
}

/** The list a screenshot/Reel was opened from, so the viewer can step to the next one. */
export type SourceRef = { type: "screenshot" | "reel" | "place"; id: string };

export type Panel =
  | { type: "place"; id: string; list?: SourceRef[] }
  | { type: "screenshot"; id: string; highlight?: string[]; list?: SourceRef[] }
  | { type: "reel"; id: string; seek?: number; list?: SourceRef[] };

interface Nav {
  view: View;
  panels: Panel[];
  exploreMode: ExploreMode;
  importTab: ImportTab;
  settingsSection: SettingsSection;
  /** A place to select and show on the map once (e.g. "Show on map"); cleared by Explore. */
  focusPlaceId?: string;
  /** A trip to open in My Trips. */
  tripId?: string;
  go: (view: View, sub?: Sub) => void;
  setExploreMode: (m: ExploreMode) => void;
  setImportTab: (t: ImportTab) => void;
  setSettingsSection: (s: SettingsSection) => void;
  clearFocus: () => void;
  setTripId: (id: string | undefined) => void;
  openPlace: (id: string, list?: SourceRef[]) => void;
  openScreenshot: (id: string, highlight?: string[], list?: SourceRef[]) => void;
  openReel: (id: string, seek?: number, list?: SourceRef[]) => void;
  /** Step through the list the current screenshot/Reel came from (+1 next, -1 previous). */
  step: (delta: 1 | -1) => boolean;
  /** After removing the current item (e.g. Ignore): show the next one, or close if it was the last. */
  advanceAfterRemoval: () => void;
  back: () => void;
  closePanels: () => void;
  /** Bumped after any mutation or processing progress so views reload their data. */
  revision: number;
  refresh: () => void;
}

const NavContext = createContext<Nav | null>(null);

export function NavProvider({ children }: { children: ReactNode }) {
  const [view, setView] = useState<View>(initialView);
  const [panels, setPanels] = useState<Panel[]>([]);
  const [revision, setRevision] = useState(0);
  const [exploreMode, setExploreModeState] = useState<ExploreMode>(() => recall("explore.mode", ["map", "library"] as const, "map"));
  const [importTab, setImportTab] = useState<ImportTab>("add");
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("general");
  const [focusPlaceId, setFocusPlaceId] = useState<string>();
  const [tripId, setTripId] = useState<string>();

  const refresh = useCallback(() => setRevision((r) => r + 1), []);
  const setExploreMode = useCallback((m: ExploreMode) => { setExploreModeState(m); remember("explore.mode", m); }, []);
  const value = useMemo<Nav>(() => ({
    view,
    panels,
    revision,
    refresh,
    exploreMode,
    importTab,
    settingsSection,
    focusPlaceId,
    tripId,
    setExploreMode,
    setImportTab,
    setSettingsSection,
    clearFocus: () => setFocusPlaceId(undefined),
    setTripId,
    go: (v, sub) => {
      setView(v);
      remember(VIEW_KEY, v);
      setPanels([]);
      if (sub?.explore) setExploreMode(sub.explore);
      if (sub?.import) setImportTab(sub.import);
      if (sub?.settings) setSettingsSection(sub.settings);
      if (sub?.focusPlaceId) setFocusPlaceId(sub.focusPlaceId);
      if (v === "trips") setTripId(sub?.tripId);
    },
    openPlace: (id, list) => setPanels((p) => [...p, { type: "place", id, list }]),
    openScreenshot: (id, highlight, list) => setPanels((p) => [...p, { type: "screenshot", id, highlight, list }]),
    openReel: (id, seek, list) => setPanels((p) => [...p, { type: "reel", id, seek, list }]),
    step: (delta) => {
      const top = panels[panels.length - 1];
      if (!top || !top.list) return false;
      const i = top.list.findIndex((r) => r.id === top.id && r.type === top.type);
      const next = top.list[i + delta];
      if (i < 0 || !next) return false;
      setPanels((p) => [...p.slice(0, -1), { type: next.type, id: next.id, list: top.list } as Panel]);
      return true;
    },
    advanceAfterRemoval: () => {
      const top = panels[panels.length - 1];
      if (!top || !top.list) { setPanels((p) => p.slice(0, -1)); return; }
      const i = top.list.findIndex((r) => r.id === top.id && r.type === top.type);
      const list = top.list.filter((_, k) => k !== i);
      const next = list[Math.min(i, list.length - 1)];
      setPanels((p) => (next ? [...p.slice(0, -1), { type: next.type, id: next.id, list } as Panel] : p.slice(0, -1)));
    },
    back: () => setPanels((p) => p.slice(0, -1)),
    closePanels: () => setPanels([]),
  }), [view, panels, revision, refresh, exploreMode, importTab, settingsSection, focusPlaceId, tripId, setExploreMode]);

  return <NavContext.Provider value={value}>{children}</NavContext.Provider>;
}

export function useNav(): Nav {
  const nav = useContext(NavContext);
  if (!nav) throw new Error("useNav outside NavProvider");
  return nav;
}

/** Loads data and reloads whenever `deps` or the global revision change. */
export function useLoad<T>(load: () => Promise<T>, deps: unknown[]): { data: T | undefined; error: string | undefined; reload: () => void } {
  const { revision } = useNav();
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string>();
  const [tick, setTick] = useState(0);
  useEffect(() => {
    let alive = true;
    load().then(
      (d) => { if (alive) { setData(d); setError(undefined); } },
      (e) => { if (alive) setError(String(e)); },
    );
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, revision, tick]);
  return { data, error, reload: () => setTick((t) => t + 1) };
}

/** Runs a mutation, surfaces errors, then refreshes all views. */
export function useAction() {
  const { refresh } = useNav();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();
  const run = useCallback(async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true);
    setError(undefined);
    try {
      const result = await fn();
      refresh();
      return result;
    } catch (e) {
      setError(String(e));
      return undefined;
    } finally {
      setBusy(false);
    }
  }, [refresh]);
  return { run, busy, error, clearError: () => setError(undefined) };
}
