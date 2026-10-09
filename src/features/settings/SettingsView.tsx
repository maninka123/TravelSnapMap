import { getVersion } from "@tauri-apps/api/app";
import {
  Activity, Archive, BadgeDollarSign, Check, ExternalLink, FolderOpen, Info, KeyRound, Lock, RotateCcw, Shield, SlidersHorizontal, Sparkles, Trash2,
} from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { AppService, BackupService, PhotoLibraryService, ProcessingService, ReelService, SettingsService } from "../../api/services";
import type { AppConfig, CostSummary, Diagnostics, RunReport, Settings } from "../../api/types";
import { ErrorNote, Modal } from "../../components/common";
import { RunReportView, TestRunDialog } from "../../components/ScanControls";
import { useToast } from "../../components/ui";
import { formatDate } from "../../lib/labels";
import { useAction, useLoad, useNav, type SettingsSection } from "../../lib/nav";
import { openUrl } from "../../lib/open";
import { getTheme, setTheme, type Theme } from "../../lib/theme";
import { BackupCard } from "./BackupCard";

const SECTIONS: { key: SettingsSection; label: string; icon: typeof Info }[] = [
  { key: "general", label: "General", icon: SlidersHorizontal },
  { key: "ai", label: "AI", icon: Sparkles },
  { key: "privacy", label: "Privacy", icon: Shield },
  { key: "sources", label: "Sources", icon: FolderOpen },
  { key: "costs", label: "Costs", icon: BadgeDollarSign },
  { key: "backup", label: "Backup & export", icon: Archive },
  { key: "diagnostics", label: "Diagnostics", icon: Activity },
  { key: "about", label: "About", icon: Info },
];

export const RELEASES_URL = "https://github.com/maninka123/TravelSnapMap/releases";

export function SettingsView() {
  const nav = useNav();
  const { data: settings, reload } = useLoad(() => SettingsService.get(), []);
  const [config, setConfig] = useState<AppConfig>();
  const action = useAction();
  const toast = useToast();
  useEffect(() => { if (settings) setConfig(settings.config); }, [settings]);

  // Settings save as you change them (no separate Save button to forget).
  const commit = async (next: AppConfig) => {
    setConfig(next);
    if (await action.run(() => SettingsService.save(next)) !== undefined) reload();
  };
  const set = <K extends keyof AppConfig>(k: K, v: AppConfig[K]) => config && commit({ ...config, [k]: v });

  return (
    <div className="page">
      <div className="page-head"><h1>Settings</h1>{action.busy && <span className="spinner" aria-label="Saving" />}</div>
      <div className="settings">
        <nav className="settings-nav" aria-label="Settings sections">
          {SECTIONS.map((s) => (
            <button key={s.key} className={`nav-item ${nav.settingsSection === s.key ? "active" : ""}`} onClick={() => nav.setSettingsSection(s.key)}
                    aria-current={nav.settingsSection === s.key ? "page" : undefined}>
              <s.icon size={16} /><span className="nav-label">{s.label}</span>
            </button>
          ))}
        </nav>
        <div className="settings-body">
          <ErrorNote error={action.error} onClose={action.clearError} />
          {!settings || !config ? <div className="skeleton" style={{ height: 160 }} /> : (
            <>
              {nav.settingsSection === "general" && <General config={config} set={set} />}
              {nav.settingsSection === "ai" && <AiSection settings={settings} config={config} set={set} onKeySaved={() => { reload(); toast.ok("API key saved in the macOS Keychain."); }}
                                                    onReset={() => commit({ ...config, model: settings.defaults.model, baseUrl: settings.defaults.baseUrl, maxOutputTokens: settings.defaults.maxOutputTokens, useThinkingByDefault: settings.defaults.useThinkingByDefault, allowThinkingEscalation: settings.defaults.allowThinkingEscalation })} />}
              {nav.settingsSection === "privacy" && <Privacy config={config} set={set} />}
              {nav.settingsSection === "sources" && <Sources config={config} set={set} reload={reload} />}
              {nav.settingsSection === "costs" && <Costs config={config} defaults={settings.defaults} set={set} />}
              {nav.settingsSection === "backup" && <BackupCard />}
              {nav.settingsSection === "diagnostics" && <DiagnosticsSection config={config} defaults={settings.defaults} commit={commit} />}
              {nav.settingsSection === "about" && <About />}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

type Setter = <K extends keyof AppConfig>(k: K, v: AppConfig[K]) => void;

function Group({ title, desc, children }: { title: string; desc?: ReactNode; children: ReactNode }) {
  return <section className="settings-group"><h3>{title}</h3>{desc && <p className="group-desc">{desc}</p>}<div style={{ paddingTop: 4 }}>{children}</div></section>;
}

function Row({ title, hint, children, top }: { title: ReactNode; hint?: ReactNode; children?: ReactNode; top?: boolean }) {
  return (
    <div className={`setting-row ${top ? "top" : ""}`}>
      <div className="grow"><span className="strong">{title}</span>{hint && <div className="hint">{hint}</div>}</div>
      {children}
    </div>
  );
}

function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return <input type="checkbox" role="switch" aria-label={label} checked={checked} onChange={(e) => onChange(e.target.checked)} style={{ width: 18, height: 18 }} />;
}

function Num({ value, onChange, step = 0.05, min, label }: { value: number | null; onChange: (v: number | null) => void; step?: number; min?: number; label: string }) {
  const [text, setText] = useState(value == null ? "" : String(value));
  useEffect(() => setText(value == null ? "" : String(value)), [value]);
  return <input type="number" aria-label={label} step={step} min={min} value={text} style={{ width: 110 }}
                onChange={(e) => setText(e.target.value)} onBlur={() => onChange(text === "" ? null : Number(text))} />;
}

function General({ config, set }: { config: AppConfig; set: Setter }) {
  const [theme, setThemeState] = useState<Theme>(getTheme);
  const choose = (t: Theme) => { setThemeState(t); setTheme(t); };
  return (
    <>
      <Group title="Appearance">
        <Row title="Theme" hint="Match follows your Mac's Light or Dark appearance.">
          <div className="theme-choices" role="radiogroup" aria-label="Theme">
            {([["system", "Match Mac", "linear-gradient(135deg,#f4f4f2 50%,#26262a 50%)"], ["light", "Light", "#f4f4f2"], ["dark", "Dark", "#26262a"]] as const).map(([k, label, bg]) => (
              <button key={k} role="radio" aria-checked={theme === k} className={`theme-choice ${theme === k ? "active" : ""}`} onClick={() => choose(k)}>
                <span className="theme-swatch" style={{ background: bg }} />{label}
              </button>
            ))}
          </div>
        </Row>
      </Group>
      <Group title="Importing">
        <Row title="Read new screenshots automatically" hint="When Photos reports new screenshots, read them right away. Off: start from Import when you like.">
          <Toggle label="Read new screenshots automatically" checked={config.autoProcessNewScreenshots} onChange={(v) => set("autoProcessNewScreenshots", v)} />
        </Row>
        <Row title="Screenshots read at the same time" hint="Higher is faster but uses more of your Mac and the network.">
          <Num label="Screenshots at the same time" value={config.maxConcurrentScreenshots} step={1} min={1} onChange={(v) => set("maxConcurrentScreenshots", Math.max(1, Math.min(8, v ?? 3)))} />
        </Row>
        <Row title="Only screenshots from these years" hint="Leave empty to include every year.">
          <div className="row">
            <Num label="From year" value={config.scanFromYear} step={1} onChange={(v) => set("scanFromYear", v)} />
            <span className="muted">to</span>
            <Num label="To year" value={config.scanToYear} step={1} onChange={(v) => set("scanToYear", v)} />
          </div>
        </Row>
        <Row title="Nearby places radius" hint="Shown on a place's page (km).">
          <Num label="Nearby radius" value={config.nearbyRadiusKm} step={1} min={1} onChange={(v) => set("nearbyRadiusKm", v ?? 5)} />
        </Row>
      </Group>
      <Group title="Welcome">
        <Row title="Show the welcome guide again" hint="Walks through Photos access, your AI key and a first small import.">
          <button className="btn" onClick={() => window.dispatchEvent(new Event("tsm:onboarding"))}>Show</button>
        </Row>
      </Group>
    </>
  );
}

function AiSection({ settings, config, set, onKeySaved, onReset }: { settings: Settings; config: AppConfig; set: Setter; onKeySaved: () => void; onReset: () => void }) {
  const [key, setKey] = useState("");
  const action = useAction();
  return (
    <>
      <Group title="DeepSeek" desc="TravelSnapMap uses DeepSeek to understand the text in your screenshots and Reels. Places are always located with Apple Maps — never by the AI.">
        <Row title="API key" hint={settings.hasApiKey ? <span className="row ok-text" style={{ gap: 4 }}><Check size={13} /> Stored ({settings.apiKeySource})</span> : "Not set — screenshots can be read, but places can't be extracted until you add a key."}>
          {settings.hasApiKey && <button className="btn small danger" onClick={() => action.run(() => SettingsService.saveApiKey(null)).then(onKeySaved)}><Trash2 size={13} /> Remove</button>}
        </Row>
        <form className="setting-row" onSubmit={async (e) => { e.preventDefault(); if (!key.trim()) return; if (await action.run(() => SettingsService.saveApiKey(key.trim())) !== undefined) { setKey(""); onKeySaved(); } }}>
          <KeyRound size={16} className="muted" />
          <input className="grow" type="password" autoComplete="off" spellCheck={false} placeholder={settings.hasApiKey ? "Replace the key…" : "Paste your DeepSeek API key"} value={key} onChange={(e) => setKey(e.target.value)} aria-label="DeepSeek API key" />
          <button className="btn primary" disabled={!key.trim() || action.busy}>Save to Keychain</button>
        </form>
        <div className="setting-row"><p className="hint">Get a key at platform.deepseek.com. It's stored only in the macOS Keychain, never in backups or logs.
          <button className="link-btn" style={{ marginLeft: 6 }} onClick={() => openUrl("https://platform.deepseek.com/api_keys")}>Open DeepSeek <ExternalLink size={11} /></button></p></div>
        <ErrorNote error={action.error} onClose={action.clearError} />
      </Group>
      <Group title="Model">
        <details className="advanced" style={{ padding: "4px 16px 14px" }}>
          <summary>Advanced model settings</summary>
          <div className="field"><label htmlFor="ai-model">Model</label><input id="ai-model" defaultValue={config.model} onBlur={(e) => e.target.value.trim() && e.target.value !== config.model && set("model", e.target.value.trim())} /></div>
          <div className="field"><label htmlFor="ai-url">API address</label><input id="ai-url" defaultValue={config.baseUrl} onBlur={(e) => e.target.value.trim() && e.target.value !== config.baseUrl && set("baseUrl", e.target.value.trim())} />
            <span className="hint">Only change this for a DeepSeek-compatible proxy you run yourself.</span></div>
          <label className="check"><input type="checkbox" checked={config.useThinkingByDefault} onChange={(e) => set("useThinkingByDefault", e.target.checked)} />
            <span>Use thinking mode for every request <span className="muted small">(slower and more expensive)</span></span></label>
          <label className="check"><input type="checkbox" checked={config.allowThinkingEscalation} onChange={(e) => set("allowThinkingEscalation", e.target.checked)} />
            <span>Retry genuinely ambiguous results once with thinking</span></label>
          <div className="row"><div className="field grow"><label>Max output tokens</label><Num label="Max output tokens" value={config.maxOutputTokens} step={50} onChange={(v) => set("maxOutputTokens", v ?? 1200)} /></div>
            <div className="field grow"><label>Daily request limit</label><Num label="Daily request limit" value={config.dailyAiRequestLimit} step={10} min={0} onChange={(v) => set("dailyAiRequestLimit", v ?? 0)} /><span className="hint">0 = no limit. Work pauses until tomorrow when reached.</span></div></div>
          <button className="btn small" onClick={onReset}><RotateCcw size={13} /> Use recommended model settings</button>
        </details>
      </Group>
    </>
  );
}

function Privacy({ config, set }: { config: AppConfig; set: Setter }) {
  const { data: permission, reload } = useLoad(() => PhotoLibraryService.permissionStatus(), []);
  const action = useAction();
  const granted = permission === "authorized" || permission === "limited";
  return (
    <>
      <Group title="What stays on your Mac" desc="Text recognition (Apple Vision), speech transcription, photo detection and place matching (Apple Maps) run on your Mac. Screenshots the local filter recognises as unrelated are never sent anywhere.">
        <Row title="What DeepSeek receives" hint="Only the recognised text of likely-travel screenshots and Reels, the date, and small hints such as the app name or creator handle. Your library, photos, notes and trips are never sent." top>
          <Lock size={18} className="muted" />
        </Row>
        <Row title="Never send screenshot images to the AI" hint="When off, a small compressed image is sent only for screenshots with almost no text, or to check an uncertain photo crop.">
          <Toggle label="Never send screenshot images" checked={!config.allowVisionRequests} onChange={(v) => set("allowVisionRequests", !v)} />
        </Row>
      </Group>
      <Group title="Photos access">
        <Row title={granted ? (permission === "limited" ? "Limited access" : "Full access") : permission === "denied" || permission === "restricted" ? "Access denied" : "Not allowed yet"}
             hint={granted ? "Only screenshots are read. Your own photos are used only when you attach them to a place."
               : permission === "denied" ? "Open System Settings → Privacy & Security → Photos and turn on TravelSnapMap Photos Bridge."
               : "TravelSnapMap needs access to read screenshots in your library."}>
          {!granted && permission !== "denied" && <button className="btn primary" onClick={async () => { await action.run(() => PhotoLibraryService.requestPermission()); reload(); }}>Allow access</button>}
          {permission === "denied" && <button className="btn" onClick={() => openUrl("x-apple.systempreferences:com.apple.preference.security?Privacy_Photos")}>Open System Settings</button>}
        </Row>
        <ErrorNote error={action.error} onClose={action.clearError} />
      </Group>
      <Group title="Your data">
        <Row title="Library folder" hint="Your places, sources and settings live here on this Mac.">
          <button className="btn" onClick={() => BackupService.revealDataFolder()}><FolderOpen size={14} /> Show in Finder</button>
        </Row>
      </Group>
    </>
  );
}

function Sources({ config, set, reload }: { config: AppConfig; set: Setter; reload: () => void }) {
  const { data: tools } = useLoad(() => ReelService.toolStatus(), []);
  const { data: locales = [] } = useLoad(() => ReelService.speechLocales(), []);
  const action = useAction();
  return (
    <>
      <Group title="Screenshot folders" desc="Folders of screenshots from any device. Only new images are read.">
        {config.screenshotFolders.length === 0 && <Row title="No folders" hint="Add one from Import → Screenshot folder." />}
        {config.screenshotFolders.map((f) => (
          <Row key={f} title={<span className="truncate" style={{ display: "block", maxWidth: 480 }} title={f}>{f.split(/[\\/]/).filter(Boolean).slice(-2).join("/")}</span>} hint={f}>
            <button className="btn small" onClick={async () => { await action.run(() => ProcessingService.removeFolder(f)); reload(); }}>Stop watching</button>
          </Row>
        ))}
        <ErrorNote error={action.error} onClose={action.clearError} />
      </Group>
      <Group title="Instagram Reels">
        <Row title="Video downloader" hint={tools?.ytDlp ? `yt-dlp found at ${tools.ytDlp}` : "yt-dlp isn't installed, so only captions are read from links. Install it (brew install yt-dlp), or import videos from Photos."}>
          {tools?.ytDlp ? <span className="pill pill-ok"><Check size={11} /> Ready</span> : <span className="pill pill-warn">Captions only</span>}
        </Row>
        <Row title="Speech language" hint="On-device where Apple supports it. Each Reel can be re-transcribed in another language.">
          <select aria-label="Speech language" value={config.transcriptionLocale} onChange={(e) => set("transcriptionLocale", e.target.value)}>
            <option value="auto">Detect automatically</option>
            {locales.map((l) => <option key={l.id} value={l.id}>{l.name}{l.onDevice ? "" : " (Apple server)"}</option>)}
          </select>
        </Row>
        <Row title="Key snapshots per Reel"><Num label="Key snapshots" value={config.maxKeyframes} step={1} min={1} onChange={(v) => set("maxKeyframes", v ?? 8)} /></Row>
        <details className="advanced" style={{ padding: "4px 16px 14px" }}>
          <summary>Advanced</summary>
          <div className="field"><label htmlFor="ytdlp">yt-dlp location</label><input id="ytdlp" defaultValue={config.ytDlpPath} placeholder="Find automatically" onBlur={(e) => e.target.value !== config.ytDlpPath && set("ytDlpPath", e.target.value)} /></div>
          <div className="field"><label htmlFor="cookies">Use a browser's Instagram login</label>
            <select id="cookies" value={config.cookiesFromBrowser} onChange={(e) => set("cookiesFromBrowser", e.target.value)}>
              <option value="">Don't use</option><option value="safari">Safari</option><option value="chrome">Chrome</option><option value="firefox">Firefox</option><option value="brave">Brave</option><option value="edge">Edge</option>
            </select>
            <span className="hint">Some Reels need you to be logged in. Cookies are read on your Mac by yt-dlp and never sent to DeepSeek.</span></div>
        </details>
      </Group>
    </>
  );
}

const usd = (v: number) => (v === 0 ? "$0" : v < 0.01 ? `$${v.toFixed(4)}` : `$${v.toFixed(2)}`);

function Costs({ config, defaults, set }: { config: AppConfig; defaults: AppConfig; set: Setter }) {
  const pricingKey = JSON.stringify(config.pricing);
  const { data: cost } = useLoad(async () => AppService.costSummary(config.pricing), [pricingKey]);
  return (
    <Group title="Estimated AI cost" desc="Calculated from the exact number of tokens used. DeepSeek bills from its own records, so treat this as an estimate.">
      <div className="group-body stack">
        {cost ? <CostSummaryView cost={cost} mode={config.pricing.mode} /> : <div className="skeleton" style={{ height: 80 }} />}
        <div className="row wrap">
          <span className="small strong">Billing</span>
          <select aria-label="Billing" value={config.pricing.mode} onChange={(e) => set("pricing", { ...config.pricing, mode: e.target.value as typeof config.pricing.mode })}>
            <option value="timeOfDay">By time of day</option><option value="peak">Always peak (cautious)</option><option value="offPeak">Always off-peak</option>
          </select>
        </div>
        <p className="hint">{config.pricing.custom ? "Using your own prices." : `${config.model} standard prices: $${config.pricing.peak.inputCacheMiss} input / $${config.pricing.peak.output} output per million tokens at peak, half that off-peak.`}</p>
        <details className="advanced" open={config.pricing.custom || undefined}>
          <summary>Custom prices</summary>
          <label className="check"><input type="checkbox" checked={config.pricing.custom}
            onChange={(e) => set("pricing", e.target.checked ? { ...config.pricing, custom: true } : { ...defaults.pricing, mode: config.pricing.mode })} />
            <span>Use my own prices <span className="muted small">(only if DeepSeek changes its rates or you use a proxy)</span></span></label>
          {config.pricing.custom && (
            <table className="price-table">
              <thead><tr><th>USD per 1M tokens</th><th>Peak</th><th>Off-peak</th></tr></thead>
              <tbody>
                {([["inputCacheMiss", "Input (cache miss)"], ["inputCacheHit", "Input (cache hit)"], ["output", "Output"]] as const).map(([k, label]) => (
                  <tr key={k}><td>{label}</td>
                    {(["peak", "offPeak"] as const).map((w) => (
                      <td key={w}><input type="number" step={0.001} min={0} aria-label={`${label} ${w}`} value={config.pricing[w][k]}
                                         onChange={(e) => set("pricing", { ...config.pricing, [w]: { ...config.pricing[w], [k]: Number(e.target.value) } })} /></td>
                    ))}</tr>
                ))}
              </tbody>
            </table>
          )}
        </details>
      </div>
    </Group>
  );
}

function CostSummaryView({ cost, mode }: { cost: CostSummary; mode: string }) {
  const options = [["timeOfDay", "By time of day", cost.byTimeOfDay], ["peak", "Always peak", cost.alwaysPeak], ["offPeak", "Always off-peak", cost.alwaysOffPeak]] as const;
  return (
    <div className="stack-s">
      <div className="cost-hero">
        <div><div className="stat-value">{usd(cost.total)}</div><div className="stat-label">so far · {cost.requests.toLocaleString()} AI requests</div></div>
        {cost.screenshotsProcessed > 0 && <div><div className="stat-value">{usd(cost.per100Screenshots)}</div><div className="stat-label">per 100 screenshots</div></div>}
        {cost.screenshotsRemaining > 0 && cost.screenshotsProcessed > 0 && <div><div className="stat-value">≈ {usd(cost.projectedRemaining)}</div><div className="stat-label">to finish {cost.screenshotsRemaining.toLocaleString()} remaining</div></div>}
      </div>
      <div className="cost-options">{options.map(([k, label, v]) => <div key={k} className={`cost-option ${mode === k ? "active" : ""}`}><span>{label}</span><strong>{usd(v)}</strong></div>)}</div>
    </div>
  );
}

function DiagnosticsSection({ config, defaults, commit }: { config: AppConfig; defaults: AppConfig; commit: (c: AppConfig) => void }) {
  const { data: diag } = useLoad(() => AppService.diagnostics(), []);
  const { data: runs = [] } = useLoad(() => ProcessingService.runs(), []);
  const [testing, setTesting] = useState(false);
  const [report, setReport] = useState<RunReport>();
  const action = useAction();
  const num = (k: keyof AppConfig, label: string, step = 0.05) => (
    <div className="field"><label>{label}</label><Num label={label} value={config[k] as number} step={step} onChange={(v) => commit({ ...config, [k]: v ?? defaults[k] })} /></div>
  );
  return (
    <>
      <Group title="Test on a sample" desc="Read 10–100 screenshots you haven't imported yet and see how well it works — travel found, places matched, time and cost — before importing everything.">
        <Row title="Test run"><button className="btn" onClick={() => setTesting(true)}>Start a test run…</button></Row>
        {runs.slice(0, 5).map((r) => (
          <Row key={r.id} title={r.kind === "validation" ? `Test run · ${r.sample}` : r.kind === "scanFolder" ? "Folder import" : "Photos import"} hint={`${formatDate(r.startedAt)} · ${r.screenshotCount} screenshots`}>
            <button className="btn small" onClick={async () => setReport((await ProcessingService.runReport(r.id)) ?? undefined)}>Report</button>
          </Row>
        ))}
      </Group>
      <Group title="Read again" desc="Recognised text is reused, and AI results are reused unless the prompt changed. Your corrections are always kept.">
        <div className="group-body row wrap">
          <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess("failed"))}>Couldn't read</button>
          <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess("needsReview"))}>Needs review</button>
          <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess("outdated"))}>Read with an older prompt</button>
          <button className="btn small danger" onClick={() => action.run(() => ProcessingService.reprocess("all"))}>Everything</button>
        </div>
        <ErrorNote error={action.error} onClose={action.clearError} />
      </Group>
      <Group title="Thresholds" desc="How sure the app must be before it decides on its own instead of asking you.">
        <details className="advanced" style={{ padding: "4px 16px 14px" }}>
          <summary>Show thresholds</summary>
          {num("localSkipScore", "Skip the AI below this local travel score")}
          {num("minimumAiConfidenceForAutoAcceptance", "Accept travel at AI confidence ≥")}
          {num("travelReviewThreshold", "Ask you about travel at AI confidence ≥")}
          {num("placeAutoAcceptScore", "Accept a map match at score ≥")}
          {num("placeReviewScore", "Ask you about a map match at score ≥")}
          {num("regionAutoAcceptConfidence", "Accept a photo crop at confidence ≥")}
          <button className="btn small" onClick={() => commit({ ...config, localSkipScore: defaults.localSkipScore, minimumAiConfidenceForAutoAcceptance: defaults.minimumAiConfidenceForAutoAcceptance,
            travelReviewThreshold: defaults.travelReviewThreshold, placeAutoAcceptScore: defaults.placeAutoAcceptScore, placeReviewScore: defaults.placeReviewScore, regionAutoAcceptConfidence: defaults.regionAutoAcceptConfidence })}>
            <RotateCcw size={13} /> Recommended values</button>
        </details>
      </Group>
      {diag && <DiagnosticsView diag={diag} />}
      {testing && <TestRunDialog onClose={() => setTesting(false)} />}
      {report && <Modal title="Import report" onClose={() => setReport(undefined)} wide><RunReportView report={report} /></Modal>}
    </>
  );
}

const ms = (v?: number) => (v === undefined ? "—" : v >= 1000 ? `${(v / 1000).toFixed(1)} s` : `${Math.round(v)} ms`);
const n = (v?: number) => (v === undefined ? "—" : v.toLocaleString());
type Stat = [label: string, value: string, tone?: "ok" | "warn" | "bad" | "accent"];

/** Usage and performance, grouped by what they describe. */
function DiagnosticsView({ diag }: { diag: Diagnostics }) {
  const metric = new Map(diag.metrics);
  const avg = (key: string) => { const total = metric.get(`${key}.ms`); return total === undefined ? undefined : total / (metric.get(`${key}.count`) || 1); };
  const processed = diag.totalScreenshots - diag.remaining;
  const groups: { title: string; tone: string; hint?: string; stats: Stat[] }[] = [
    { title: "Library", tone: "blue", hint: diag.totalScreenshots ? `${Math.round((processed / diag.totalScreenshots) * 100)}% read` : undefined, stats: [
      ["Screenshots", n(diag.totalScreenshots)], ["Read", n(processed), "ok"], ["Remaining", n(diag.remaining), diag.remaining ? "warn" : undefined], ["Couldn't read", n(diag.failed), diag.failed ? "bad" : undefined]] },
    { title: "Results", tone: "green", stats: [
      ["Travel", n(diag.travelScreenshots), "ok"], ["Places", n(diag.places), "ok"], ["Needs review", n(diag.needsReview), diag.needsReview ? "warn" : undefined],
      ["Not travel", n(diag.notTravel)], ["Skipped on your Mac", n(diag.skippedLocally)]] },
    { title: "Place matching (Apple Maps)", tone: "teal", stats: [
      ["Matched automatically", n(metric.get("maps.autoResolved")), "ok"], ["Asked you", n(metric.get("maps.review")), "warn"], ["Not found", n(metric.get("maps.unresolved"))], ["Places created", n(metric.get("places.created"))]] },
    { title: "AI requests", tone: "purple", hint: `${n(metric.get("ai.avoidedByLocalFilter") ?? 0)} avoided on your Mac`, stats: [
      ["Successful", n(diag.aiRequests - diag.aiFailures), "ok"], ["Failed", n(diag.aiFailures), diag.aiFailures ? "bad" : undefined], ["With image", n(diag.aiVisionRequests)],
      ["With thinking", n(diag.aiThinkingRequests)], ["Average response", ms(diag.aiAverageLatencyMs)]] },
    { title: "Speed (average per screenshot)", tone: "gray", stats: [
      ["Whole pipeline", ms(avg("pipeline.total")), "accent"], ["Photos export", ms(avg("photos.export"))], ["Text recognition", ms(avg("ocr"))], ["AI extraction", ms(avg("ai.extract"))], ["Map lookup", ms(avg("maps.resolve"))]] },
  ];
  return (
    <div className="diag-groups">
      {groups.map((g) => (
        <section key={g.title} className={`diag-group tone-${g.tone}`}>
          <header><span className="diag-dot" /><h4>{g.title}</h4>{g.hint && <span className="muted small">{g.hint}</span>}</header>
          <div className="diag-row">{g.stats.map(([label, value, tone]) => <div key={label} className={`diag-stat ${tone ? `is-${tone}` : ""}`}><div className="diag-value">{value}</div><div className="diag-label">{label}</div></div>)}</div>
        </section>
      ))}
    </div>
  );
}

function About() {
  const [version, setVersion] = useState<string>();
  useEffect(() => { getVersion().then(setVersion, () => setVersion(undefined)); }, []);
  return (
    <>
      <Group title="TravelSnapMap">
        <Row title="Version" hint="Local-first travel library for macOS."><span className="tabular strong">{version ?? "—"}</span></Row>
        <Row title="Updates" hint="New versions are published on GitHub with release notes. Download the .dmg and replace the app; your library is kept.">
          <button className="btn" onClick={() => openUrl(RELEASES_URL)}>Check for updates <ExternalLink size={12} /></button>
        </Row>
        <Row title="Logs" hint="~/Library/Logs/TravelSnapMap/travelsnapmap.log — screenshot contents and keys are never written to it." />
      </Group>
      <Group title="Credits">
        <div className="group-body small muted" style={{ lineHeight: 1.6 }}>
          Maps: OpenFreeMap (OpenStreetMap data © OpenStreetMap contributors). Map rendering: MapLibre GL JS. Icons: Lucide (ISC). Flags: flag-icons (MIT).
          Place matching: Apple Maps. Text and speech recognition: Apple Vision and Speech, on your Mac.
        </div>
      </Group>
    </>
  );
}
