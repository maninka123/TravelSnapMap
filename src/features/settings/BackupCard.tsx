import { open, save } from "@tauri-apps/plugin-dialog";
import { Archive, FileJson, Map as MapIcon, RotateCcw } from "lucide-react";
import { useState } from "react";
import { BackupService, type RestorePreview } from "../../api/services";
import { ErrorNote, Modal, plural } from "../../components/common";
import { useToast } from "../../components/ui";
import { useAction } from "../../lib/nav";

const today = () => new Date().toISOString().slice(0, 10);

/** Back up, restore, and export places. Backups never contain the API key. */
export function BackupCard() {
  const action = useAction();
  const toast = useToast();
  const [staged, setStaged] = useState<RestorePreview>();

  const backup = async () => {
    const path = await save({ title: "Back up TravelSnapMap", defaultPath: `TravelSnapMap Backup ${today()}.zip`, filters: [{ name: "Zip archive", extensions: ["zip"] }] });
    if (!path) return;
    const s = await action.run(() => BackupService.backup(path));
    if (s) toast.ok(`Backup saved: ${plural(s.places, "place")}, ${plural(s.screenshots, "screenshot")}, ${plural(s.reels, "Reel")} (${(s.sizeBytes / 1_048_576).toFixed(1)} MB).`);
  };

  const restore = async () => {
    const path = await open({ title: "Choose a TravelSnapMap backup", multiple: false, directory: false, filters: [{ name: "Zip archive", extensions: ["zip"] }] });
    if (typeof path !== "string") return;
    const preview = await action.run(() => BackupService.restore(path));
    if (preview) setStaged(preview);
  };

  const exportAs = async (format: "json" | "geojson") => {
    const ext = format === "geojson" ? "geojson" : "json";
    const path = await save({
      title: format === "geojson" ? "Export places as GeoJSON" : "Export places as JSON",
      defaultPath: `TravelSnapMap Places ${today()}.${ext}`,
      filters: [{ name: format === "geojson" ? "GeoJSON" : "JSON", extensions: [ext] }],
    });
    if (!path) return;
    const n = await action.run(() => BackupService.exportPlaces(path, format));
    if (n !== undefined) toast.ok(`Exported ${plural(n, format === "geojson" ? "confirmed place" : "place")}.`);
  };

  return (
    <>
      <section className="settings-group">
        <h3>Backup &amp; restore</h3>
        <p className="group-desc">A backup holds your library, place photos, your own photos, Reel audio and key snapshots, and settings — never your API key (it stays in the Keychain). Reel videos and full-size screenshot copies are left out; they can be fetched again.</p>
        <div className="setting-row">
          <div className="grow"><span className="strong">Back up now</span><div className="hint">Saves one .zip file wherever you choose.</div></div>
          <button className="btn primary" disabled={action.busy} onClick={backup}><Archive size={14} /> Back up…</button>
        </div>
        <div className="setting-row">
          <div className="grow"><span className="strong">Restore a backup</span><div className="hint">Replaces your current library after a restart. The current one is kept in the library folder's “backups”, not deleted.</div></div>
          <button className="btn" disabled={action.busy} onClick={restore}><RotateCcw size={14} /> Restore…</button>
        </div>
        <ErrorNote error={action.error} onClose={action.clearError} />
      </section>
      <section className="settings-group">
        <h3>Export places</h3>
        <p className="group-desc">GeoJSON opens in Google Earth, QGIS and most map tools; JSON includes every saved tip with its source.</p>
        <div className="setting-row">
          <div className="grow"><span className="strong">Places as GeoJSON</span><div className="hint">Confirmed places with name, city, country, kind, status and notes.</div></div>
          <button className="btn" disabled={action.busy} onClick={() => exportAs("geojson")}><MapIcon size={14} /> Export…</button>
        </div>
        <div className="setting-row">
          <div className="grow"><span className="strong">Places as JSON</span><div className="hint">All places with tips, sources, trips and Reel links.</div></div>
          <button className="btn" disabled={action.busy} onClick={() => exportAs("json")}><FileJson size={14} /> Export…</button>
        </div>
      </section>
      {staged && (
        <Modal title="Restore this backup?" onClose={async () => { await action.run(() => BackupService.cancelRestore()); setStaged(undefined); }}>
          <p>The backup contains {plural(staged.places, "place")}, {plural(staged.screenshots, "screenshot")}, {plural(staged.reels, "Reel")} and {plural(staged.trips, "trip")}.</p>
          <p className="muted small">TravelSnapMap restarts to switch libraries. Your current library is moved to the “backups” folder, so nothing is lost.</p>
          <div className="modal-actions">
            <button className="btn" onClick={async () => { await action.run(() => BackupService.cancelRestore()); setStaged(undefined); }}>Cancel</button>
            <button className="btn primary" onClick={() => void BackupService.restart()}>Restart and restore</button>
          </div>
        </Modal>
      )}
    </>
  );
}
