import { save } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { BackupService } from "../../api/services";
import { ErrorNote } from "../../components/common";
import { useAction } from "../../lib/nav";

const today = () => new Date().toISOString().slice(0, 10);

/** Backup TravelSnapMap · Export Places as JSON · Export Places as GeoJSON. */
export function BackupCard() {
  const action = useAction();
  const [message, setMessage] = useState<string>();

  const backup = async () => {
    const path = await save({ title: "Backup TravelSnapMap", defaultPath: `TravelSnapMap Backup ${today()}.zip`, filters: [{ name: "Zip archive", extensions: ["zip"] }] });
    if (!path) return;
    const s = await action.run(() => BackupService.backup(path));
    if (s) setMessage(`Backup saved: ${s.places} places, ${s.screenshots} screenshots, ${s.reels} Reels, ${s.files} files (${(s.sizeBytes / 1_048_576).toFixed(1)} MB).`);
  };

  const exportAs = async (format: "json" | "geojson") => {
    const ext = format === "geojson" ? "geojson" : "json";
    const path = await save({
      title: format === "geojson" ? "Export Places as GeoJSON" : "Export Places as JSON",
      defaultPath: `TravelSnapMap Places ${today()}.${ext}`,
      filters: [{ name: format === "geojson" ? "GeoJSON" : "JSON", extensions: [ext] }],
    });
    if (!path) return;
    const n = await action.run(() => BackupService.exportPlaces(path, format));
    if (n !== undefined) setMessage(`Exported ${n} ${format === "geojson" ? "verified places as GeoJSON" : "places as JSON"}.`);
  };

  return (
    <div className="card">
      <h3>Backup &amp; export</h3>
      <p className="muted small">
        A backup contains your library database, place photos and crops, Reel audio and key snapshots, and your settings.
        It never contains your DeepSeek API key (that stays in the macOS Keychain). Reel videos and full-size screenshot
        copies are left out because they can be downloaded or regenerated again.
      </p>
      <div className="stack" style={{ gap: 8 }}>
        <button className="btn primary" disabled={action.busy} onClick={backup}>💾 Backup TravelSnapMap…</button>
        <button className="btn" disabled={action.busy} onClick={() => exportAs("json")}>Export Places as JSON…</button>
        <button className="btn" disabled={action.busy} onClick={() => exportAs("geojson")}>Export Places as GeoJSON…</button>
      </div>
      <p className="muted small">GeoJSON contains verified places with name, city, country, category, personal status, notes and source count — it opens in Google Earth, QGIS and most map tools.</p>
      {message && <p className="small">✓ {message}</p>}
      <ErrorNote error={action.error} onClose={action.clearError} />
    </div>
  );
}
