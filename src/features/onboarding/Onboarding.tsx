import { open } from "@tauri-apps/plugin-dialog";
import { ArrowRight, Check, Clapperboard, FolderOpen, Images, KeyRound, Lock, MapPinned, ShieldCheck, Sparkles } from "lucide-react";
import { useEffect, useState } from "react";
import { AppService, PhotoLibraryService, ProcessingService, SettingsService } from "../../api/services";
import type { Overview, QueueSnapshot } from "../../api/types";
import { ErrorNote } from "../../components/common";
import { useAction, useLoad } from "../../lib/nav";
import { openUrl } from "../../lib/open";
import { ImportReelDialog } from "../reels/ImportReelDialog";

const DONE_KEY = "onboarding.done";
type Source = "photos" | "folder" | "reels";

/** First run: an empty library and the guide not finished or dismissed before. */
export function shouldShowOnboarding(overview: Overview): boolean {
  try { if (localStorage.getItem(DONE_KEY) === "1") return false; } catch { /* show it */ }
  const sources = Object.values(overview.screenshotCounts ?? {}).reduce((a, b) => a + b, 0);
  return overview.places === 0 && sources === 0;
}

function markDone() {
  try { localStorage.setItem(DONE_KEY, "1"); } catch { /* shown again next time; harmless */ }
}

/** Welcome → privacy & Photos → choose a source → AI key → a small first import → your places. */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const [step, setStep] = useState(0);
  const [source, setSource] = useState<Source>("photos");
  const finish = () => { markDone(); onDone(); };
  const steps = 5;
  return (
    <div className="onboarding" role="dialog" aria-modal="true" aria-label="Welcome to TravelSnapMap">
      <div className="onboarding-card">
        <div className="onboarding-steps" aria-hidden="true">{Array.from({ length: steps }, (_, i) => <span key={i} className={i <= step ? "done" : ""} />)}</div>
        {step === 0 && <Welcome onNext={() => setStep(1)} onSkip={finish} />}
        {step === 1 && <PrivacyStep onNext={() => setStep(2)} onBack={() => setStep(0)} />}
        {step === 2 && <SourceStep value={source} onChange={setSource} onNext={() => setStep(3)} onBack={() => setStep(1)} />}
        {step === 3 && <KeyStep onNext={() => setStep(4)} onBack={() => setStep(2)} />}
        {step === 4 && <FirstImport source={source} onDone={finish} onBack={() => setStep(3)} />}
      </div>
    </div>
  );
}

function Welcome({ onNext, onSkip }: { onNext: () => void; onSkip: () => void }) {
  return (
    <>
      <div className="onboarding-hero"><MapPinned size={34} /></div>
      <h1>Your travel screenshots, on a map</h1>
      <p className="lead">TravelSnapMap reads the travel posts you saved — screenshots and Instagram Reels — finds the real places in them, and keeps every tip with the post it came from.</p>
      <div className="feature-list">
        <Feature icon={<Images size={17} />} title="Import once, keep everything" text="Screenshots from Photos or any folder, and Reels by link. Nothing is imported twice." />
        <Feature icon={<MapPinned size={17} />} title="Real places, checked" text="Every place is located with Apple Maps. When it isn't sure, it asks you." />
        <Feature icon={<Lock size={17} />} title="Private by design" text="Your library stays on this Mac. Text is recognised on your Mac; only that text is sent to the AI." />
      </div>
      <div className="onboarding-foot">
        <button className="btn ghost" onClick={onSkip}>Skip for now</button>
        <span className="spacer" />
        <button className="btn primary large" onClick={onNext}>Get started <ArrowRight size={15} /></button>
      </div>
    </>
  );
}

function Feature({ icon, title, text }: { icon: React.ReactNode; title: string; text: string }) {
  return <div className="feature"><span className="feature-icon">{icon}</span><div><span className="strong">{title}</span><span className="muted">{text}</span></div></div>;
}

function PrivacyStep({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const { data: permission, reload } = useLoad(() => PhotoLibraryService.permissionStatus(), []);
  const action = useAction();
  const granted = permission === "authorized" || permission === "limited";
  const denied = permission === "denied" || permission === "restricted";
  return (
    <>
      <h1>Privacy and Photos</h1>
      <div className="feature-list">
        <Feature icon={<ShieldCheck size={17} />} title="Only screenshots are read" text="TravelSnapMap looks at screenshots in your library — not your other photos, unless you attach them to a place yourself." />
        <Feature icon={<Lock size={17} />} title="Recognised on your Mac" text="Apple Vision reads the text. Screenshots that are clearly unrelated never leave your Mac; images are never sent unless you allow it in Settings." />
      </div>
      <div className="note info">
        <Images size={16} />
        <div className="grow">
          {granted ? <span className="row ok-text" style={{ gap: 6 }}><Check size={15} /> Photos access is allowed{permission === "limited" ? " (selected photos only)" : ""}.</span>
            : denied ? <>macOS has blocked access. Open <b>System Settings → Privacy &amp; Security → Photos</b> and turn on <b>TravelSnapMap Photos Bridge</b>, then come back.</>
            : <>macOS will ask once. The request comes from <b>TravelSnapMap Photos Bridge</b>, the small helper that reads your screenshots.</>}
        </div>
        {!granted && !denied && <button className="btn primary" disabled={action.busy} onClick={async () => { await action.run(() => PhotoLibraryService.requestPermission()); reload(); }}>Allow access</button>}
        {denied && <button className="btn" onClick={() => openUrl("x-apple.systempreferences:com.apple.preference.security?Privacy_Photos")}>Open System Settings</button>}
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />
      <p className="muted small">Only importing from folders or Reels? You can skip this.</p>
      <div className="onboarding-foot">
        <button className="btn ghost" onClick={onBack}>Back</button><span className="spacer" />
        <button className="btn primary large" onClick={onNext}>{granted ? "Continue" : "Continue without Photos"} <ArrowRight size={15} /></button>
      </div>
    </>
  );
}

function SourceStep({ value, onChange, onNext, onBack }: { value: Source; onChange: (s: Source) => void; onNext: () => void; onBack: () => void }) {
  const choices: [Source, React.ReactNode, string, string][] = [
    ["photos", <Images size={20} key="p" />, "Screenshots in Apple Photos", "Start with a small sample of your screenshots."],
    ["folder", <FolderOpen size={20} key="f" />, "A folder of screenshots", "Images from another phone or computer."],
    ["reels", <Clapperboard size={20} key="r" />, "Instagram Reels", "Paste the links of Reels you saved."],
  ];
  return (
    <>
      <h1>Where are your travel posts?</h1>
      <p className="lead">Pick one to start. You can add the others any time from Import.</p>
      <div className="choice-list" role="radiogroup" aria-label="First source">
        {choices.map(([k, icon, title, text]) => (
          <button key={k} role="radio" aria-checked={value === k} className={`choice ${value === k ? "active" : ""}`} onClick={() => onChange(k)}>
            <span className="feature-icon">{icon}</span>
            <span className="grow"><span className="strong">{title}</span><span className="muted small">{text}</span></span>
            {value === k && <Check size={18} className="ok-text" />}
          </button>
        ))}
      </div>
      <div className="onboarding-foot">
        <button className="btn ghost" onClick={onBack}>Back</button><span className="spacer" />
        <button className="btn primary large" onClick={onNext}>Continue <ArrowRight size={15} /></button>
      </div>
    </>
  );
}

function KeyStep({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const { data: settings, reload } = useLoad(() => SettingsService.get(), []);
  const [key, setKey] = useState("");
  const action = useAction();
  const has = !!settings?.hasApiKey;
  return (
    <>
      <h1>Connect the AI</h1>
      <p className="lead">TravelSnapMap uses DeepSeek to understand the text in your posts. It costs about two to four US cents per hundred screenshots, billed by DeepSeek to your own account.</p>
      {has ? (
        <div className="note info"><Check size={16} className="ok-text" /><span className="grow">An API key is set up ({settings?.apiKeySource}).</span></div>
      ) : (
        <form className="stack-s" onSubmit={async (e) => { e.preventDefault(); if (key.trim() && await action.run(() => SettingsService.saveApiKey(key.trim())) !== undefined) { setKey(""); reload(); } }}>
          <label className="field-label" htmlFor="onb-key">DeepSeek API key</label>
          <div className="row">
            <KeyRound size={16} className="muted" />
            <input id="onb-key" className="grow" type="password" autoComplete="off" spellCheck={false} placeholder="sk-…" value={key} onChange={(e) => setKey(e.target.value)} />
            <button className="btn primary" disabled={!key.trim() || action.busy}>Save</button>
          </div>
          <p className="hint">Saved only in the macOS Keychain — never in backups or logs. Get a key at <button type="button" className="link-btn" onClick={() => openUrl("https://platform.deepseek.com/api_keys")}>platform.deepseek.com</button>.</p>
        </form>
      )}
      <ErrorNote error={action.error} onClose={action.clearError} />
      <div className="onboarding-foot">
        <button className="btn ghost" onClick={onBack}>Back</button><span className="spacer" />
        <button className="btn primary large" onClick={onNext}>{has ? "Continue" : "Skip for now"} <ArrowRight size={15} /></button>
      </div>
    </>
  );
}

function FirstImport({ source, onDone, onBack }: { source: Source; onDone: () => void; onBack: () => void }) {
  const action = useAction();
  const [snap, setSnap] = useState<QueueSnapshot>();
  const [started, setStarted] = useState(false);
  const [reels, setReels] = useState(false);
  const { data: overview, reload } = useLoad(() => AppService.overview(), []);
  useEffect(() => {
    const unlisten = ProcessingService.onProgress((s) => { setSnap(s); if (!s.running) reload(); });
    return () => { void unlisten.then((u) => u()); };
  }, [reload]);

  const start = async () => {
    if (source === "photos") {
      if (await action.run(() => ProcessingService.start({ kind: "validation", count: 25, random: false })) !== undefined) setStarted(true);
    } else if (source === "folder") {
      const dir = await open({ directory: true, multiple: false, title: "Choose a folder of screenshots" });
      if (typeof dir === "string" && await action.run(() => ProcessingService.addFolder(dir)) !== undefined) setStarted(true);
    } else setReels(true);
  };
  const running = !!snap?.running;
  const pct = snap?.total ? Math.round((snap.processed / snap.total) * 100) : 0;

  return (
    <>
      <h1>{started ? (running ? "Reading your first posts…" : "Your first places") : "Try it on a few"}</h1>
      {!started ? (
        <p className="lead">{source === "photos" ? "TravelSnapMap will read your 25 newest screenshots so you can see the results before importing everything."
          : source === "folder" ? "Choose a folder; TravelSnapMap reads the screenshots in it and keeps an eye on it for new ones."
          : "Paste one or more Reel links. Each Reel takes about 20 seconds."}</p>
      ) : (
        <div className="stack-s" aria-live="polite">
          <div className={`progress ${running && !snap?.total ? "indeterminate" : ""}`}><div style={{ width: `${pct}%` }} /></div>
          <span className="muted small">{running ? `${snap?.phase ?? "Working"} · ${snap?.processed ?? 0} of ${snap?.total ?? "…"}` : "Done for now."}</span>
          <div className="stat-row">
            <div className="stat ok"><div className="stat-value">{overview?.places ?? 0}</div><div className="stat-label">Places found</div></div>
            <div className="stat"><div className="stat-value">{snap?.travel ?? 0}</div><div className="stat-label">Travel posts</div></div>
            <div className={`stat ${(overview?.openReviews ?? 0) ? "warn" : ""}`}><div className="stat-value">{overview?.openReviews ?? 0}</div><div className="stat-label">To review</div></div>
          </div>
          {!overview?.aiConfigured && <div className="note warn"><Sparkles size={15} /><span>Without an AI key, text is read but places can't be extracted yet. Add one in Settings → AI.</span></div>}
        </div>
      )}
      <ErrorNote error={action.error} onClose={action.clearError} />
      <div className="onboarding-foot">
        {!started && <button className="btn ghost" onClick={onBack}>Back</button>}
        <span className="spacer" />
        {!started && <button className="btn ghost" onClick={onDone}>Skip</button>}
        {!started ? <button className="btn primary large" disabled={action.busy} onClick={start}>{source === "reels" ? "Paste links…" : source === "folder" ? "Choose folder…" : "Read 25 screenshots"}</button>
          : <button className="btn primary large" onClick={onDone}>{running ? "Continue in the background" : "See my map"} <ArrowRight size={15} /></button>}
      </div>
      {reels && <ImportReelDialog onClose={() => { setReels(false); setStarted(true); }} />}
    </>
  );
}
