import { open } from "@tauri-apps/plugin-dialog";
import { useMemo, useState } from "react";
import { ReelService, type BulkReelImport } from "../../api/services";
import { ErrorNote, Modal } from "../../components/common";
import { formatDate, formatTime } from "../../lib/labels";
import { useAction, useLoad, useNav } from "../../lib/nav";

const LINK = /instagram\.com\/(?:[\w.]+\/)?(?:reel|reels|p|tv)\/([\w-]+)/gi;

/** Unique Reel/Post codes found in any text (mirrors the backend's extraction, for the live count). */
function countLinks(text: string): number {
  return new Set([...text.matchAll(LINK)].map((m) => m[1])).size;
}

/**
 * Import one Reel, many pasted links, or every link in a text file. Several Reels are processed one after
 * another in the background (gentle on Instagram's rate limits); you can keep using the app meanwhile.
 */
export function ImportReelDialog({ onClose }: { onClose: () => void }) {
  const nav = useNav();
  const [text, setText] = useState("");
  const [result, setResult] = useState<BulkReelImport>();
  const action = useAction();
  const count = useMemo(() => countLinks(text), [text]);

  const finish = (r: BulkReelImport | undefined) => {
    if (!r) return;
    if (r.found === 1 && r.ids.length <= 1) {
      // A single Reel: go straight to its page to watch the stages.
      onClose();
      const id = r.ids[0];
      if (id) nav.openReel(id);
      return;
    }
    setResult(r);
    setText("");
  };

  const importText = async () => finish(await action.run(() => ReelService.importMany(text)));

  const importFile = async () => {
    const path = await open({
      title: "Choose a text file with Instagram links",
      multiple: false,
      directory: false,
      filters: [{ name: "Text", extensions: ["txt", "md", "csv", "tsv", "json", "html", "rtf"] }, { name: "All files", extensions: ["*"] }],
    });
    if (typeof path === "string") finish(await action.run(() => ReelService.importFile(path)));
  };

  return (
    <Modal title="Import Instagram Reels" onClose={onClose}>
      <p className="muted small">
        Paste one link or many (one per line, or mixed with other text), or choose a text file full of links. Audio is
        transcribed and key frames are read on your Mac; DeepSeek receives only the caption, transcript and on-screen text.
      </p>
      <textarea
        autoFocus
        rows={6}
        placeholder={"https://www.instagram.com/reel/…\nhttps://www.instagram.com/reel/…"}
        value={text}
        onChange={(e) => { setText(e.target.value); setResult(undefined); }}
        onKeyDown={(e) => { if (e.key === "Enter" && (e.metaKey || e.ctrlKey) && count > 0) void importText(); }}
      />
      <div className="row wrap">
        <span className="muted small grow">
          {text.trim() === "" ? "Tip: ⌘V a whole list, then ⌘↩ to import." : count === 0 ? "No Instagram Reel or post links found yet." : `${count} link${count === 1 ? "" : "s"} found`}
        </span>
        <button className="btn" disabled={action.busy} onClick={importFile}>📄 Choose text file…</button>
        <button className="btn primary" disabled={count === 0 || action.busy} onClick={importText}>
          {action.busy ? "Importing…" : count > 1 ? `Import ${count} Reels` : "Import"}
        </button>
      </div>
      {result && (
        <div className="disclaimer">
          ✓ Found {result.found} link{result.found === 1 ? "" : "s"}: {result.added} new
          {result.alreadyImported > 0 ? `, ${result.alreadyImported} already in your library` : ""}.
          {result.ids.length > 0 ? ` Processing ${result.ids.length} one after another in the background — watch them under Sources → Reels.` : ""}
        </div>
      )}
      {result && result.found === 0 && <p className="muted small">That file didn't contain any Instagram Reel or post links.</p>}
      <ErrorNote error={action.error} onClose={action.clearError} />
    </Modal>
  );
}

/** Fallback: pick a video already saved in Photos (e.g. a downloaded Reel or screen recording). */
export function PhotosVideoPicker({ reelId, onClose }: { reelId: string | null; onClose: () => void }) {
  const { data: videos, error } = useLoad(() => ReelService.listPhotoVideos(), []);
  const action = useAction();
  return (
    <Modal title="Import video from Photos" onClose={onClose}>
      <p className="muted small">Most recent videos in your library. The video stays in Photos; a working copy is made for transcription.</p>
      <ErrorNote error={error ?? action.error} />
      {!videos && !error && <p className="muted">Loading videos…</p>}
      <div className="candidate-list">
        {videos?.slice(0, 60).map((v) => (
          <div key={v.id} className="candidate">
            <span>🎞️ {formatDate(v.creationDate)} · {formatTime(v.durationSec)}</span>
            <button className="btn" disabled={action.busy} onClick={async () => {
              if (await action.run(() => ReelService.attachPhotoVideo(reelId, v.id)) !== undefined) onClose();
            }}>Use</button>
          </div>
        ))}
      </div>
    </Modal>
  );
}
