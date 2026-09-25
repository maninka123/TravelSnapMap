import { invoke } from "@tauri-apps/api/core";

/** Opens a URL in the user's default browser (via the backend, so the webview never navigates away). */
export function openUrl(url: string): void {
  void invoke("open_external", { url });
}
