import { useState } from "react";
import { ReelService } from "../../api/services";
import { ErrorNote, Modal } from "../../components/common";
import { formatDate, formatTime } from "../../lib/labels";
import { useAction, useLoad, useNav } from "../../lib/nav";

const INSTAGRAM = /^https?:\/\/(www\.)?instagram\.com\/(reel|reels|p|tv)\/[\w-]+/i;

/** Paste an Instagram Reel/Post link → import runs in the background. */
export function ImportReelDialog({ onClose }: { onClose: () => void }) {
  const nav = useNav();
  const [url, setUrl] = useState("");
  const action = useAction();
  const valid = INSTAGRAM.test(url.trim());

  const submit = async () => {
    const id = await action.run(() => ReelService.importUrl(url.trim()));
    if (id) { onClose(); nav.openReel(id); }
  };

  return (
    <Modal title="Import Instagram Reel" onClose={onClose}>
      <p className="muted small">
        Copy the Reel or post link in Instagram (Share → Copy link) and paste it here. The audio is transcribed and key
        frames are read on your Mac; DeepSeek receives only the caption, transcript and on-screen text.
      </p>
      <form className="row" onSubmit={(e) => { e.preventDefault(); if (valid) void submit(); }}>
        <input autoFocus className="grow" placeholder="https://www.instagram.com/reel/…" value={url} onChange={(e) => setUrl(e.target.value)}
               onPaste={(e) => { const t = e.clipboardData.getData("text"); if (INSTAGRAM.test(t.trim())) { e.preventDefault(); setUrl(t.trim()); } }} />
        <button className="btn primary" disabled={!valid || action.busy}>{action.busy ? "Importing…" : "Import"}</button>
      </form>
      {url && !valid && <p className="muted small">That doesn’t look like an Instagram Reel or post link.</p>}
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
