import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

export type View = "map" | "places" | "screenshots" | "review" | "trips" | "settings";

export type Panel =
  | { type: "place"; id: string }
  | { type: "screenshot"; id: string; highlight?: string[] }
  | { type: "reel"; id: string; seek?: number };

interface Nav {
  view: View;
  panels: Panel[];
  go: (view: View) => void;
  openPlace: (id: string) => void;
  openScreenshot: (id: string, highlight?: string[]) => void;
  openReel: (id: string, seek?: number) => void;
  back: () => void;
  closePanels: () => void;
  /** Bumped after any mutation or processing progress so views reload their data. */
  revision: number;
  refresh: () => void;
}

const NavContext = createContext<Nav | null>(null);

export function NavProvider({ children }: { children: ReactNode }) {
  const [view, setView] = useState<View>("map");
  const [panels, setPanels] = useState<Panel[]>([]);
  const [revision, setRevision] = useState(0);

  const refresh = useCallback(() => setRevision((r) => r + 1), []);
  const value = useMemo<Nav>(() => ({
    view,
    panels,
    revision,
    refresh,
    go: (v) => { setView(v); setPanels([]); },
    openPlace: (id) => setPanels((p) => [...p, { type: "place", id }]),
    openScreenshot: (id, highlight) => setPanels((p) => [...p, { type: "screenshot", id, highlight }]),
    openReel: (id, seek) => setPanels((p) => [...p, { type: "reel", id, seek }]),
    back: () => setPanels((p) => p.slice(0, -1)),
    closePanels: () => setPanels([]),
  }), [view, panels, revision, refresh]);

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
