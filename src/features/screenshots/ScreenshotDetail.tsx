import { useRef, useState } from "react";
import { fileUrl, ScreenshotService } from "../../api/services";
import type { Place, Rect, ScreenshotDetail as Detail } from "../../api/types";
import { CategoryChip, ErrorNote, Modal, PlaceLine, PlaceSearchDialog, Pill, Thumb } from "../../components/common";
import { FACT, formatDate, pct, PROCESSING, SOURCE } from "../../lib/labels";
import { useAction, useLoad, useNav } from "../../lib/nav";

export function ScreenshotDetail({ id, highlight }: { id: string; highlight?: string[] }) {
  const nav = useNav();
  const { data, error } = useLoad(() => ScreenshotService.detail(id), [id]);
  const action = useAction();
  const [overlay, setOverlay] = useState(!!highlight?.length);
  const [dialog, setDialog] = useState<"add" | { correct: Place } | "crop" | null>(null);

  if (error) return <p className="bad">{error}</p>;
  if (!data) return <p className="muted">Loading…</p>;
  const s = data.screenshot;
  const status = PROCESSING[s.status];
  const hl = new Set(highlight ?? []);
  const src = fileUrl(s.imagePath) ?? fileUrl(s.thumbnailPath);

  return (
    <div>
      <div className="detail-head">
        <h2>Screenshot · {formatDate(s.creationDate)}</h2>
        <div className="row wrap">
          <Pill tone={status.tone}>{status.label}</Pill>
          {s.sourceType !== "unknown" && <Pill>{SOURCE[s.sourceType]}</Pill>}
          {s.creator && <Pill>{s.creator}</Pill>}
          {s.statusDetail && <span className="muted small">{s.statusDetail}</span>}
        </div>
        <div className="row wrap">
          <button className="btn" onClick={() => action.run(() => ScreenshotService.action(id, "reprocess"))} disabled={action.busy}>↻ Reprocess</button>
          {s.status !== "complete" && s.classification !== "travel" &&
            <button className="btn" onClick={() => action.run(() => ScreenshotService.action(id, "markTravel"))}>Mark as travel</button>}
          <button className="btn" onClick={() => action.run(() => ScreenshotService.action(id, "markNotTravel"))}>Not travel</button>
          <button className="btn" onClick={() => action.run(() => ScreenshotService.action(id, "ignore"))}>Ignore</button>
          <button className="btn" onClick={() => setDialog("add")}>＋ Add place</button>
          {data.places.length > 0 && <button className="btn" onClick={() => setDialog("crop")}>✂️ Crop photo</button>}
        </div>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />

      <div className="shot-layout">
        <div className="stack">
          <label className="check"><input type="checkbox" checked={overlay} onChange={(e) => setOverlay(e.target.checked)} /> Show text regions</label>
          <div className="shot-frame">
            {src ? <img src={src} alt="Screenshot" /> : <Thumb path={null} />}
            {overlay && data.blocks.map((b) => (
              <div key={b.id} className={`ocr-box ${hl.has(b.id) ? "hl" : ""}`} title={`${b.text} (${pct(b.confidence)})`}
                   style={{ left: `${b.x * 100}%`, top: `${b.y * 100}%`, width: `${b.width * 100}%`, height: `${b.height * 100}%` }} />
            ))}
          </div>
        </div>

        <div className="stack">
          <div className="card">
            <h3>Detected places</h3>
            {data.places.length === 0 && <p className="muted small">No place linked yet.</p>}
            {data.places.map((p) => {
              const link = data.links.find((l) => l.placeId === p.id);
              return (
                <div key={p.id} className="candidate">
                  <div style={{ cursor: "pointer" }} onClick={() => nav.openPlace(p.id)}>
                    <div className="strong">{p.canonicalName}</div>
                    <PlaceLine place={p} />
                    <div className="row"><CategoryChip category={p.category} />
                      {link?.isUserVerified ? <Pill tone="ok">set by you</Pill> : link && <span className="muted small">{pct(link.confidence)} match</span>}
                    </div>
                  </div>
                  <button className="btn small" onClick={() => setDialog({ correct: p })}>Correct</button>
                </div>
              );
            })}
          </div>

          {data.facts.length > 0 && (
            <div className="card">
              <h3>Extracted information</h3>
              {data.facts.map((f) => (
                <div key={f.id} className="fact">
                  <div className="fact-text">{FACT[f.type]?.emoji} {f.text}</div>
                  {f.sourceQuote && <div className="muted small">“{f.sourceQuote}”</div>}
                </div>
              ))}
            </div>
          )}

          {data.images.length > 0 && (
            <div className="card">
              <h3>Extracted photos</h3>
              <div className="grid small-tiles">
                {data.images.map((img) => (
                  <div key={img.id} className="tile"><Thumb path={img.imagePath} />
                    <div className="tile-body small">{img.isAccepted ? "Used as place photo" : "Waiting for review"} · quality {pct(img.qualityScore)}</div>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="card">
            <h3>Processing</h3>
            <div className="kv">
              <span>Travel confidence</span><span>{pct(s.travelConfidence)} (local {pct(s.localTravelScore)})</span>
              <span>Level</span><span>{["—", "Local only", "AI", "AI + thinking"][s.escalationLevel] ?? s.escalationLevel}</span>
              {s.aiModel && <><span>AI model</span><span>{s.aiModel}{s.aiThinking ? " · thinking" : ""}{s.aiImageUsed ? " · image sent" : " · text only"}</span></>}
              {s.aiModel && <><span>Tokens</span><span>{s.aiInputTokens} in · {s.aiOutputTokens} out</span>
              <span>Estimated AI cost</span><span>${s.aiCost.toFixed(5)}</span></>}
              {s.aiLatencyMs > 0 && <><span>AI latency</span><span>{(s.aiLatencyMs / 1000).toFixed(1)} s{s.aiRetryCount ? ` · ${s.aiRetryCount} retries` : ""}</span></>}
            </div>
          </div>

          <div className="card">
            <h3>Recognised text</h3>
            <div className="ocr-text">{s.ocrFullText || "No text recognised."}</div>
          </div>
        </div>
      </div>

      {dialog === "add" && (
        <PlaceSearchDialog title="Add a place to this screenshot" onClose={() => setDialog(null)}
          onPick={async (c) => { setDialog(null); await action.run(() => ScreenshotService.addPlace(id, c)); }} />
      )}
      {dialog && typeof dialog === "object" && (
        <PlaceSearchDialog title={`Correct "${dialog.correct.canonicalName}"`} initialQuery={dialog.correct.canonicalName} onClose={() => setDialog(null)}
          onPick={async (c) => { const wrong = dialog.correct.id; setDialog(null); await action.run(() => ScreenshotService.correctPlace(id, wrong, c)); }} />
      )}
      {dialog === "crop" && <CropEditor detail={data} onClose={() => setDialog(null)} />}
    </div>
  );
}

/** Manual crop: drag to move, drag the corner to resize, then choose the place. */
function CropEditor({ detail, onClose }: { detail: Detail; onClose: () => void }) {
  const src = fileUrl(detail.screenshot.imagePath) ?? fileUrl(detail.screenshot.thumbnailPath);
  const [rect, setRect] = useState<Rect>(detail.images[0]?.crop ?? { x: 0.05, y: 0.15, width: 0.9, height: 0.4 });
  const [placeId, setPlaceId] = useState(detail.places[0]?.id ?? "");
  const frame = useRef<HTMLDivElement>(null);
  const action = useAction();

  const drag = (mode: "move" | "resize") => (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const box = frame.current!.getBoundingClientRect();
    const start = { x: e.clientX, y: e.clientY, rect };
    const onMove = (ev: MouseEvent) => {
      const dx = (ev.clientX - start.x) / box.width;
      const dy = (ev.clientY - start.y) / box.height;
      const r = start.rect;
      setRect(mode === "move"
        ? { ...r, x: clamp(r.x + dx, 0, 1 - r.width), y: clamp(r.y + dy, 0, 1 - r.height) }
        : { ...r, width: clamp(r.width + dx, 0.05, 1 - r.x), height: clamp(r.height + dy, 0.05, 1 - r.y) });
    };
    const onUp = () => { window.removeEventListener("mousemove", onMove); window.removeEventListener("mouseup", onUp); };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <Modal title="Crop a place photo" onClose={onClose} wide>
      <div className="shot-layout">
        <div className="shot-frame" ref={frame}>
          {src && <img src={src} alt="" draggable={false} />}
          <div className="crop-box" onMouseDown={drag("move")}
               style={{ left: `${rect.x * 100}%`, top: `${rect.y * 100}%`, width: `${rect.width * 100}%`, height: `${rect.height * 100}%` }}>
            <div className="crop-handle" onMouseDown={drag("resize")} />
          </div>
        </div>
        <div className="stack">
          <label className="field"><span className="strong">Use as photo for</span>
            <select value={placeId} onChange={(e) => setPlaceId(e.target.value)}>
              {detail.places.map((p) => <option key={p.id} value={p.id}>{p.canonicalName}</option>)}
            </select>
          </label>
          <ErrorNote error={action.error} />
          <button className="btn primary" disabled={!placeId || action.busy}
                  onClick={async () => { if (await action.run(() => ScreenshotService.saveCrop(detail.screenshot.id, placeId, rect)) !== undefined) onClose(); }}>
            Use as place photo
          </button>
        </div>
      </div>
    </Modal>
  );
}

function clamp(v: number, min: number, max: number) {
  return Math.min(Math.max(v, min), max);
}
