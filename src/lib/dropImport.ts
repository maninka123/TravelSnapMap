// Files dropped on the window. Tauri delivers real file paths (the webview's own drag events don't), so screenshots
// dropped anywhere can be imported. Outside the desktop app (demo, tests) this simply does nothing.
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ProcessingService, type ImportedFiles } from "../api/services";

const IMAGE = /\.(png|jpe?g|heic|heif|webp|tiff?)$/i;

export function isImagePath(path: string): boolean {
  return IMAGE.test(path);
}

/** Calls `onDrop` with dropped file paths; returns a cleanup function. `onHover` reports drag-over state. */
export function onFileDrop(onDrop: (paths: string[]) => void, onHover?: (over: boolean) => void): () => void {
  let unlisten: (() => void) | undefined;
  let cancelled = false;
  try {
    getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter" || p.type === "over") onHover?.(true);
      else if (p.type === "leave") onHover?.(false);
      else if (p.type === "drop") { onHover?.(false); if (p.paths.length) onDrop(p.paths); }
    }).then((u) => { if (cancelled) u(); else unlisten = u; }, () => {});
  } catch { /* not running inside the desktop app */ }
  return () => { cancelled = true; unlisten?.(); };
}

export function importDroppedPaths(paths: string[]): Promise<ImportedFiles> {
  return ProcessingService.importFiles(paths);
}
