import { useState } from "react";
import { ReelService } from "../../api/services";
import type { Reel, ReelStage } from "../../api/types";
import { useAction, useLoad } from "../../lib/nav";

const STAGES: { key: ReelStage; label: string }[] = [
  { key: "caption", label: "Caption" },
  { key: "video", label: "Video" },
  { key: "audio", label: "Audio" },
  { key: "transcript", label: "Transcript" },
  { key: "keyframes", label: "Keyframes" },
  { key: "ocr", label: "OCR" },
  { key: "ai", label: "AI extraction" },
  { key: "places", label: "Places resolved" },
];

const ICON: Record<string, string> = { pending: "○", running: "⏳", done: "✅", skipped: "⏭️", failed: "⚠️" };

/** Each stage of Reel processing, with what it found. A failed stage doesn't stop the others. */
export function ReelStages({ reel }: { reel: Reel }) {
  return (
    <div className="card">
      <h3>Processing stages</h3>
      <div className="stages">
        {STAGES.map((s) => {
          const st = reel.stages?.[s.key];
          const status = st?.status ?? "pending";
          return (
            <div key={s.key} className={`stage stage-${status}`}>
              <span className="stage-icon">{ICON[status]}</span>
              <span className="stage-label">{s.label}</span>
              <span className="muted small grow">{st?.detail ?? (status === "pending" ? "Waiting" : "")}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Pick the spoken language (or Auto) and re-transcribe; places are updated afterwards. */
export function TranscriptLanguage({ reel }: { reel: Reel }) {
  const { data: locales = [] } = useLoad(() => ReelService.speechLocales(), []);
  const [choice, setChoice] = useState(reel.transcriptLocaleOverride ?? "auto");
  const action = useAction();
  const requested = ["en-US", "ja-JP", "zh-CN", "ko-KR", "si-LK", "ta-IN"];
  const supported = new Set(locales.map((l) => l.id));
  const common = requested.map((id) => {
    const l = locales.find((x) => x.id === id);
    const name = l?.name ?? { "si-LK": "Sinhala (Sri Lanka)", "ta-IN": "Tamil (India)" }[id] ?? id;
    return { id, label: supported.has(id) ? `${name}${l?.onDevice ? "" : " (Apple server)"}` : `${name} — not supported on this Mac` };
  });
  const others = locales.filter((l) => !requested.includes(l.id));

  return (
    <div className="row wrap">
      <select value={choice} onChange={(e) => setChoice(e.target.value)}>
        <option value="auto">Auto language</option>
        <optgroup label="Common">
          {common.map((c) => <option key={c.id} value={c.id}>{c.label}</option>)}
        </optgroup>
        <optgroup label="All supported">
          {others.map((l) => <option key={l.id} value={l.id}>{l.name}{l.onDevice ? "" : " (Apple server)"}</option>)}
        </optgroup>
      </select>
      <button className="btn small" disabled={action.busy || !reel.mediaPath}
              onClick={() => action.run(() => ReelService.retranscribe(reel.id, choice))}>
        ↻ Re-transcribe
      </button>
      {reel.transcriptConfidence != null && <span className="muted small">confidence {Math.round(reel.transcriptConfidence * 100)}%</span>}
    </div>
  );
}
