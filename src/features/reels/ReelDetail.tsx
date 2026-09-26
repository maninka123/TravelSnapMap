import { useEffect, useRef, useState } from "react";
import { openUrl } from "../../lib/open";
import { fileUrl, ReelService } from "../../api/services";
import { CategoryChip, ErrorNote, PlaceLine, Pill, Thumb } from "../../components/common";
import { AddFact, FactRow } from "../../components/FactsEditor";
import { PlaceSearchDialog } from "../../components/PlacePicker";
import { formatDate, formatTime, PROCESSING, SOURCE_KIND } from "../../lib/labels";
import { useAction, useLoad, useNav } from "../../lib/nav";
import { PhotosVideoPicker } from "./ImportReelDialog";
import { ReelStages, TranscriptLanguage } from "./ReelStages";

/** A saved Reel: video, saved voice audio, timestamped transcript, key snapshots and what was extracted. */
export function ReelDetail({ id, seek }: { id: string; seek?: number }) {
  const nav = useNav();
  const { data, error } = useLoad(() => ReelService.detail(id), [id]);
  const action = useAction();
  const video = useRef<HTMLVideoElement>(null);
  const audio = useRef<HTMLAudioElement>(null);
  const [picker, setPicker] = useState(false);
  const [addingPlace, setAddingPlace] = useState(false);
  const [current, setCurrent] = useState(seek ?? -1);

  const jump = (t: number) => {
    setCurrent(t);
    const el = video.current ?? audio.current;
    if (el) { el.currentTime = t; void el.play().catch(() => {}); }
  };
  useEffect(() => { if (seek != null && data) jump(Math.max(0, seek - 0.5)); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [seek, !!data]);

  if (error) return <p className="bad">{error}</p>;
  if (!data) return <p className="muted">Loading…</p>;
  const r = data.reel;
  const status = PROCESSING[r.status];
  const mediaSrc = fileUrl(r.mediaPath);
  const audioSrc = fileUrl(r.audioPath);
  const busy = !["complete", "needsReview", "failed", "needsMedia", "notTravel", "ignored"].includes(r.status);

  return (
    <div>
      <div className="detail-head">
        <h2>🎬 Instagram Reel{r.creator ? ` · ${r.creator}` : ""}</h2>
        <div className="row wrap">
          <Pill tone={status.tone}>{status.label}</Pill>
          {r.durationSec && <Pill>{formatTime(r.durationSec)}</Pill>}
          {r.mediaSource && <Pill>{r.mediaSource === "photos" ? "Video from Photos" : "Downloaded"}</Pill>}
          <span className="muted small">{formatDate(r.createdAt)}</span>
          {r.statusDetail && <span className="muted small">{r.statusDetail}</span>}
        </div>
        <div className="row wrap">
          <button className="btn" onClick={() => openUrl(r.url)}>Open on Instagram ↗</button>
          <button className="btn" disabled={busy} onClick={() => action.run(() => ReelService.action(id, "reprocess"))}>↻ Reprocess</button>
          <button className="btn" onClick={() => setPicker(true)}>🎞️ Import video from Photos</button>
          <button className="btn" title="Hide it for good: never processed again, not counted as unfinished"
                  onClick={async () => { if (await action.run(() => ReelService.action(id, "ignore")) !== undefined) nav.advanceAfterRemoval(); }}>Ignore</button>
          <button className="btn danger" onClick={async () => { if (await action.run(() => ReelService.action(id, "delete")) !== undefined) nav.advanceAfterRemoval(); }}>Delete</button>
        </div>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />

      {r.status === "needsMedia" && (
        <div className="disclaimer">
          Instagram didn’t provide the video for this link, so only the caption and metadata were saved.
          If you have the video in Photos (for example a saved Reel or a screen recording), use <b>Import video from Photos</b>.
        </div>
      )}

      <ReelStages reel={r} />

      <div className="shot-layout">
        <div className="stack">
          {mediaSrc ? (
            <video ref={video} src={mediaSrc} controls playsInline onTimeUpdate={(e) => setCurrent(e.currentTarget.currentTime)} />
          ) : (
            <Thumb path={r.thumbnailPath} fallback="🎬" />
          )}
          {audioSrc && (
            <div className="card">
              <h4>🎙️ Saved voice audio</h4>
              <audio ref={audio} src={audioSrc} controls onTimeUpdate={(e) => !mediaSrc && setCurrent(e.currentTarget.currentTime)} />
            </div>
          )}
        </div>

        <div className="stack">
          {r.caption && (
            <div className="card">
              <h3>Caption</h3>
              <div style={{ whiteSpace: "pre-wrap" }}>{r.caption}</div>
            </div>
          )}

          <div className="card">
            <h3>Voice transcript {r.transcriptLocale && <span className="muted small">({r.transcriptLocale})</span>}</h3>
            <TranscriptLanguage reel={r} />
            {r.transcript.length === 0 ? <p className="muted small">{busy ? "Transcribing…" : "No speech found."}</p> : (
              <div className="transcript">
                {r.transcript.map((seg, i) => (
                  <div key={i} className={`transcript-line ${current >= seg.start && current < seg.end ? "hl" : ""}`} onClick={() => jump(seg.start)}>
                    <span className="ts">{formatTime(seg.start)}</span><span>{seg.text}</span>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="card">
            <div className="row"><h3 className="grow">Places</h3><button className="btn small" onClick={() => setAddingPlace(true)}>＋ Add place</button></div>
            {data.places.length === 0 && <p className="muted small">{busy ? "Working on it…" : "No place found."}</p>}
            {data.places.map((p) => (
              <div key={p.id} className="candidate" style={{ cursor: "pointer" }} onClick={() => nav.openPlace(p.id)}>
                <div><div className="strong">{p.canonicalName}</div><PlaceLine place={p} /></div>
                <span className="row">
                  <CategoryChip category={p.category} />
                  <button className="btn small ghost" title="This Reel isn't about this place"
                          onClick={(e) => { e.stopPropagation(); void action.run(() => ReelService.removePlace(p.id, id)); }}>Remove</button>
                </span>
              </div>
            ))}
            {data.reviews.some((rv) => !rv.isResolved) && (
              <button className="btn small" style={{ marginTop: 8 }} onClick={() => nav.go("review")}>Review uncertain results →</button>
            )}
          </div>

          {(data.facts.length > 0 || data.places.length > 0) && (
            <div className="card">
              <h3>Extracted information</h3>
              {data.facts.map((f) => (
                <FactRow key={f.id} fact={f} source={
                  <div className="fact-source">
                    <span>{SOURCE_KIND[f.sourceKind] ?? f.sourceKind}</span>
                    {f.sourceTimeSec != null && <button onClick={() => jump(f.sourceTimeSec!)}>{formatTime(f.sourceTimeSec)} ▶</button>}
                  </div>} />
              ))}
              <AddFact places={data.places} reelId={id} />
            </div>
          )}
        </div>
      </div>

      {data.keyframes.length > 0 && (
        <>
          <div className="section"><h3>Key snapshots</h3><span className="muted small">{data.keyframes.length} frames · on-screen text read locally</span></div>
          <div className="keyframes">
            {data.keyframes.map((k) => (
              <div key={k.id} className="keyframe" onClick={() => jump(k.timeSec)} title={k.ocrText || "No on-screen text"}>
                <Thumb path={k.imagePath} />
                <span className="ts-badge">{formatTime(k.timeSec)}</span>
              </div>
            ))}
          </div>
        </>
      )}

      {picker && <PhotosVideoPicker reelId={id} onClose={() => setPicker(false)} />}
      {addingPlace && (
        <PlaceSearchDialog title="Add a place to this Reel" onClose={() => setAddingPlace(false)}
          onPick={async (c) => { setAddingPlace(false); await action.run(() => ReelService.addPlace(id, c)); }} />
      )}
    </div>
  );
}
