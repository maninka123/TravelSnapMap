import { useEffect, useState } from "react";
import { AppService, PhotoLibraryService, ProcessingService, ReelService, SettingsService } from "../../api/services";
import type { AppConfig, CostSummary, Diagnostics, RunReport } from "../../api/types";
import { ErrorNote, Modal } from "../../components/common";
import { RunReportView, TestRunDialog } from "../../components/ScanControls";
import { BackupCard } from "./BackupCard";
import { formatDate } from "../../lib/labels";
import { useAction, useLoad } from "../../lib/nav";

export function SettingsView() {
  const { data: settings, reload } = useLoad(() => SettingsService.get(), []);
  const { data: diag } = useLoad(() => AppService.diagnostics(), []);
  const { data: permission } = useLoad(() => PhotoLibraryService.permissionStatus(), []);
  const { data: tools } = useLoad(() => ReelService.toolStatus(), []);
  const { data: locales = [] } = useLoad(() => ReelService.speechLocales(), []);
  const { data: runs = [] } = useLoad(() => ProcessingService.runs(), []);
  const [config, setConfig] = useState<AppConfig>();
  const pricingKey = JSON.stringify(config?.pricing ?? null);
  const { data: cost } = useLoad(async () => (config ? AppService.costSummary(config.pricing) : undefined), [pricingKey]);
  const [runReport, setRunReport] = useState<RunReport>();
  const [testing, setTesting] = useState(false);
  const [key, setKey] = useState("");
  const [saved, setSaved] = useState(false);
  const action = useAction();

  useEffect(() => { if (settings) setConfig(settings.config); }, [settings]);
  if (!settings || !config) return <div className="page muted">Loading…</div>;

  const set = <K extends keyof AppConfig>(k: K, v: AppConfig[K]) => { setConfig({ ...config, [k]: v }); setSaved(false); };
  const num = (k: keyof AppConfig, step = 0.05) => (
    <input type="number" step={step} value={config[k] as number ?? ""} onChange={(e) => set(k, (e.target.value === "" ? null : Number(e.target.value)) as never)} />
  );
  const save = async () => { if (await action.run(() => SettingsService.save(config)) !== undefined) { setSaved(true); reload(); } };

  return (
    <div className="page">
      <div className="page-head">
        <h2>Settings</h2>
        {saved && <span className="muted small">Saved ✓</span>}
        <button className="btn" onClick={() => { setConfig(settings.defaults); setSaved(false); }}>Reset to defaults</button>
        <button className="btn primary" onClick={save}>Save settings</button>
      </div>
      <ErrorNote error={action.error} onClose={action.clearError} />

      <div className="card cost-card">
        <div className="row wrap">
          <h3 className="grow">Estimated AI cost</h3>
          <select value={config.pricing.mode} onChange={(e) => set("pricing", { ...config.pricing, mode: e.target.value as typeof config.pricing.mode })}>
            <option value="timeOfDay">Billing: by time of day</option>
            <option value="peak">Billing: always peak (conservative)</option>
            <option value="offPeak">Billing: always off-peak</option>
          </select>
        </div>
        {cost ? <CostSummaryView cost={cost} mode={config.pricing.mode} /> : <p className="muted small">Calculating…</p>}
        <p className="muted small">
          An estimate from the exact token counts — DeepSeek bills from its own records.{" "}
          {config.pricing.custom
            ? "Using your custom prices."
            : `Prices: ${config.model} standard rates (${config.pricing.source.replace(/^.*checked /, "checked ")}) — $${config.pricing.peak.inputCacheMiss} input / $${config.pricing.peak.output} output per 1M tokens at peak (Mon–Fri ${config.pricing.peakHoursUtc.map(([a, b]) => `${a}–${b}`).join(", ")} UTC), half that off-peak.`}
        </p>
        <details className="advanced" open={config.pricing.custom || undefined}>
          <summary>Advanced: custom prices</summary>
          <label className="check">
            <input type="checkbox" checked={config.pricing.custom}
                   onChange={(e) => set("pricing", e.target.checked ? { ...config.pricing, custom: true } : { ...settings.defaults.pricing, mode: config.pricing.mode })} />
            <span>Use my own prices <span className="muted small">(only if DeepSeek changes its rates or you use a proxy)</span></span>
          </label>
          {config.pricing.custom && (
            <table className="price-table">
              <thead><tr><th>USD per 1M tokens</th><th>Peak</th><th>Off-peak</th></tr></thead>
              <tbody>
                {([["inputCacheMiss", "Input (cache miss)"], ["inputCacheHit", "Input (cache hit)"], ["output", "Output"]] as const).map(([key, label]) => (
                  <tr key={key}>
                    <td>{label}</td>
                    {(["peak", "offPeak"] as const).map((window) => (
                      <td key={window}>
                        <input type="number" step={0.001} min={0} value={config.pricing[window][key]}
                               onChange={(e) => set("pricing", { ...config.pricing, [window]: { ...config.pricing[window], [key]: Number(e.target.value) } })} />
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </details>
      </div>

      <div className="settings-grid">
        <div className="card">
          <h3>Privacy</h3>
          <p className="muted small">
            Text recognition (Apple Vision), speech transcription, photo detection and place matching (Apple Maps) run on your Mac.
            Screenshots that the local filter marks as clearly unrelated are never sent anywhere. For the rest, DeepSeek receives
            only the recognised text, the screenshot date and small hints (app name, creator handle) — not the image.
          </p>
          <label className="check">
            <input type="checkbox" checked={!config.allowVisionRequests} onChange={(e) => set("allowVisionRequests", !e.target.checked)} />
            <span><b>Never send screenshot images to AI</b><br /><span className="muted small">When off, a small compressed image is sent only for screenshots with almost no text, or to check an uncertain photo crop.</span></span>
          </label>
          <div className="kv"><span>Photos access</span><span>{permission}</span></div>
          {permission !== "authorized" && permission !== "limited" &&
            <button className="btn" style={{ marginTop: 8 }} onClick={() => action.run(() => PhotoLibraryService.requestPermission())}>Allow Photos access</button>}
        </div>

        <div className="card">
          <h3>DeepSeek</h3>
          <div className="kv" style={{ marginBottom: 10 }}>
            <span>API key</span><span>{settings.hasApiKey ? `✓ ${settings.apiKeySource}` : "Not configured"}</span>
          </div>
          <form className="row" onSubmit={async (e) => { e.preventDefault(); await action.run(() => SettingsService.saveApiKey(key || null)); setKey(""); reload(); }}>
            <input className="grow" type="password" placeholder="Paste a key to store it in the macOS Keychain (optional)" value={key} onChange={(e) => setKey(e.target.value)} />
            <button className="btn">Save key</button>
          </form>
          <p className="muted small">Development: <code>DEEPSEEK_API_KEY</code> in <code>.env.local</code> is used first. Never commit keys.</p>
          <div className="field"><label>Model</label><input value={config.model} onChange={(e) => set("model", e.target.value)} /></div>
          <div className="field"><label>API base URL</label><input value={config.baseUrl} onChange={(e) => set("baseUrl", e.target.value)} />
            <span className="hint">Point at your own proxy in production so the secret stays on a server.</span></div>
          <label className="check"><input type="checkbox" checked={config.useThinkingByDefault} onChange={(e) => set("useThinkingByDefault", e.target.checked)} />
            <span>Use thinking mode for every request <span className="muted small">(slower, more expensive — off by default)</span></span></label>
          <label className="check"><input type="checkbox" checked={config.allowThinkingEscalation} onChange={(e) => set("allowThinkingEscalation", e.target.checked)} />
            <span>Retry genuinely ambiguous results once with thinking</span></label>
          <div className="field"><label>Max output tokens</label>{num("maxOutputTokens", 50)}</div>
          <div className="field"><label>Daily AI request limit</label>{num("dailyAiRequestLimit", 10)}<span className="hint">0 = unlimited. Work pauses and resumes the next day.</span></div>
        </div>

        <div className="card">
          <h3>Thresholds</h3>
          <div className="field"><label>Skip AI below local travel score</label>{num("localSkipScore")}</div>
          <div className="field"><label>Auto-accept travel at AI confidence ≥</label>{num("minimumAiConfidenceForAutoAcceptance")}</div>
          <div className="field"><label>Send to Review at AI confidence ≥</label>{num("travelReviewThreshold")}<span className="hint">Below this it is marked not travel (you can override).</span></div>
          <div className="field"><label>Auto-accept map match at score ≥</label>{num("placeAutoAcceptScore")}</div>
          <div className="field"><label>Ask about map match at score ≥</label>{num("placeReviewScore")}</div>
          <div className="field"><label>Auto-accept photo crop at confidence ≥</label>{num("regionAutoAcceptConfidence")}</div>
          <div className="field"><label>Nearby places radius (km)</label>{num("nearbyRadiusKm", 1)}</div>
        </div>

        <div className="card">
          <h3>Processing</h3>
          <label className="check"><input type="checkbox" checked={config.autoProcessNewScreenshots} onChange={(e) => set("autoProcessNewScreenshots", e.target.checked)} />
            <span><b>Automatically process new screenshots</b><br /><span className="muted small">When on, new screenshots are processed as soon as Photos reports them. Off: use “Scan New Screenshots”.</span></span></label>
          <h4 style={{ margin: "8px 0" }}>Screenshot folders</h4>
          {config.screenshotFolders.length === 0 && <p className="muted small">None. Use “Import Folder…” in Sources to add screenshots from any device.</p>}
          {config.screenshotFolders.map((f) => (
            <div key={f} className="candidate"><span className="small">📁 {f}</span>
              <button className="btn small" onClick={async () => { await action.run(() => ProcessingService.removeFolder(f)); reload(); }}>Remove</button></div>
          ))}
          <div className="field"><label>Screenshots processed in parallel</label>{num("maxConcurrentScreenshots", 1)}</div>
          <div className="row">
            <div className="field grow"><label>Only scan from year</label>{num("scanFromYear", 1)}</div>
            <div className="field grow"><label>to year</label>{num("scanToYear", 1)}</div>
          </div>
          <h4 style={{ margin: "8px 0" }}>Reprocess</h4>
          <div className="row wrap">
            <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess("failed"))}>Failed</button>
            <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess("needsReview"))}>Needs review</button>
            <button className="btn small" onClick={() => action.run(() => ProcessingService.reprocess("outdated"))}>Older prompt version</button>
            <button className="btn small danger" onClick={() => action.run(() => ProcessingService.reprocess("all"))}>Everything</button>
          </div>
          <p className="muted small">OCR results are reused; cached AI results are reused unless the prompt version changed.</p>
        </div>

        <div className="card">
          <h3>Instagram Reels</h3>
          <div className="kv" style={{ marginBottom: 10 }}>
            <span>Video downloader</span><span>{tools?.ytDlp ? `✓ yt-dlp (${tools.ytDlp})` : "yt-dlp not found — caption only; use Import video from Photos"}</span>
          </div>
          <div className="field"><label>Transcription language</label>
            <select value={config.transcriptionLocale} onChange={(e) => set("transcriptionLocale", e.target.value)}>
              <option value="auto">Auto language (detect from caption / on-screen text)</option>
              {locales.map((l) => <option key={l.id} value={l.id}>{l.name}{l.onDevice ? "" : " (Apple server)"}</option>)}
            </select>
            <span className="hint">On-device where Apple supports it. Each Reel can be re-transcribed in another language.</span></div>
          <div className="field"><label>Key snapshots per Reel</label>{num("maxKeyframes", 1)}</div>
          <div className="field"><label>yt-dlp path (optional)</label><input value={config.ytDlpPath} onChange={(e) => set("ytDlpPath", e.target.value)} placeholder="auto-detect" /></div>
          <div className="field"><label>Use browser cookies (optional)</label>
            <select value={config.cookiesFromBrowser} onChange={(e) => set("cookiesFromBrowser", e.target.value)}>
              <option value="">None</option><option value="chrome">Chrome</option><option value="firefox">Firefox</option><option value="safari">Safari</option><option value="brave">Brave</option><option value="edge">Edge</option>
            </select>
            <span className="hint">Some Reels need you to be logged in. Cookies are read locally by yt-dlp and never sent to DeepSeek.</span></div>
        </div>

        <BackupCard />

      </div>

      <div className="section"><h3>Diagnostics</h3></div>
      <div className="card test-run-card">
        <div className="grow">
          <div className="strong">Test on a sample</div>
          <div className="muted small">Process 10–100 unprocessed screenshots and see how well it works — travel found, places resolved, time and estimated AI cost — before running the whole library.</div>
        </div>
        <button className="btn" onClick={() => setTesting(true)}>🧪 Test run…</button>
      </div>
      {testing && <TestRunDialog onClose={() => setTesting(false)} />}

      {runs.length > 0 && (
        <>
          <div className="section"><h4>Past test runs &amp; scans</h4></div>
          <div className="list">
            {runs.map((r) => (
              <div key={r.id} className="list-row" onClick={async () => setRunReport((await ProcessingService.runReport(r.id)) ?? undefined)}>
                <div className="grow"><div className="strong">{r.kind === "validation" ? `Test run · ${r.sample}` : r.kind === "scanFolder" ? "Folder scan" : "Scan New Screenshots"}</div>
                  <div className="muted small">{formatDate(r.startedAt)} · {r.screenshotCount} screenshots{r.finishedAt ? "" : " · running"}</div></div>
                <span className="muted small">View report →</span>
              </div>
            ))}
          </div>
        </>
      )}
      {runReport && <Modal title="Run report" onClose={() => setRunReport(undefined)} wide><RunReportView report={runReport} /></Modal>}

      {diag && <DiagnosticsView diag={diag} />}
    </div>
  );
}

const usd = (v: number) => (v === 0 ? "$0" : v < 0.01 ? `$${v.toFixed(4)}` : `$${v.toFixed(2)}`);

/** Spend so far under the prices below, plus what the same tokens cost under each option and a projection. */
function CostSummaryView({ cost, mode }: { cost: CostSummary; mode: string }) {
  const options = [
    ["timeOfDay", "By time of day", cost.byTimeOfDay],
    ["peak", "Always peak", cost.alwaysPeak],
    ["offPeak", "Always off-peak", cost.alwaysOffPeak],
  ] as const;
  const parts = [["Screenshots", cost.screenshots], ["Reels", cost.reels], ["Trip summaries", cost.other]].filter(([, v]) => (v as number) > 0);
  return (
    <div className="cost-summary">
      <div className="cost-hero">
        <div>
          <div className="stat-value">{usd(cost.total)}</div>
          <div className="stat-label">spent so far · {cost.requests.toLocaleString()} AI requests
            {cost.requests > 0 && ` (${Math.round((cost.peakRequests / cost.requests) * 100)}% in peak hours)`}</div>
        </div>
        {cost.screenshotsProcessed > 0 && (
          <div>
            <div className="stat-value">{usd(cost.per100Screenshots)}</div>
            <div className="stat-label">per 100 screenshots</div>
          </div>
        )}
        {cost.screenshotsRemaining > 0 && cost.screenshotsProcessed > 0 && (
          <div>
            <div className="stat-value">≈ {usd(cost.projectedRemaining)}</div>
            <div className="stat-label">to finish {cost.screenshotsRemaining.toLocaleString()} remaining</div>
          </div>
        )}
      </div>
      {parts.length > 1 && <div className="muted small">{parts.map(([l, v]) => `${l} ${usd(v as number)}`).join(" · ")}</div>}
      <div className="cost-options">
        {options.map(([key, label, value]) => (
          <div key={key} className={`cost-option ${mode === key ? "active" : ""}`}>
            <span>{label}</span><strong>{usd(value)}</strong>
          </div>
        ))}
      </div>
    </div>
  );
}

const ms = (v?: number) => (v === undefined ? "—" : v >= 1000 ? `${(v / 1000).toFixed(1)} s` : `${Math.round(v)} ms`);
const n = (v?: number) => (v === undefined ? "—" : v.toLocaleString());

type Stat = [label: string, value: string, tone?: "ok" | "warn" | "bad" | "accent"];

/** Diagnostics grouped by what they describe, each group with its own colour. */
function DiagnosticsView({ diag }: { diag: Diagnostics }) {
  const metric = new Map(diag.metrics);
  const avg = (key: string) => {
    const total = metric.get(`${key}.ms`);
    return total === undefined ? undefined : total / (metric.get(`${key}.count`) || 1);
  };
  const processed = diag.totalScreenshots - diag.remaining;
  const pct = diag.totalScreenshots ? Math.round((processed / diag.totalScreenshots) * 100) : 0;
  const successful = diag.aiRequests - diag.aiFailures;

  const groups: { title: string; tone: string; hint?: string; stats: Stat[] }[] = [
    { title: "Library", tone: "blue", hint: `${pct}% processed`, stats: [
      ["Screenshots", n(diag.totalScreenshots)], ["Processed", n(processed), "ok"], ["Remaining", n(diag.remaining), diag.remaining ? "warn" : undefined],
      ["Failed", n(diag.failed), diag.failed ? "bad" : undefined],
    ] },
    { title: "Results", tone: "green", stats: [
      ["Travel", n(diag.travelScreenshots), "ok"], ["Places", n(diag.places), "ok"], ["Needs review", n(diag.needsReview), diag.needsReview ? "warn" : undefined],
      ["Not travel", n(diag.notTravel)], ["Skipped on your Mac", n(diag.skippedLocally)], ["Open reviews", n(diag.openReviews)],
    ] },
    { title: "Place matching (Apple Maps)", tone: "teal", stats: [
      ["Auto-resolved", n(metric.get("maps.autoResolved")), "ok"], ["Sent to review", n(metric.get("maps.review")), "warn"],
      ["Unresolved", n(metric.get("maps.unresolved"))], ["Places created", n(metric.get("places.created"))],
      ["Photo regions accepted", n(metric.get("images.accepted"))],
    ] },
    { title: "AI requests", tone: "purple", hint: `${n(metric.get("ai.avoidedByLocalFilter") ?? 0)} avoided by the local filter`, stats: [
      ["Successful", n(successful), "ok"], ["Failed", n(diag.aiFailures), diag.aiFailures ? "bad" : undefined],
      ["Text only", n(diag.aiTextRequests)], ["With image", n(diag.aiVisionRequests)], ["With thinking", n(diag.aiThinkingRequests)],
      ["Avg response", ms(diag.aiAverageLatencyMs)],
    ] },
    { title: "Tokens & cost", tone: "orange", stats: [
      ["Input tokens", n(diag.aiInputTokens)], ["Cache-hit tokens", n(diag.aiCacheHitTokens)], ["Output tokens", n(diag.aiOutputTokens)],
      ["Estimated cost", usd(diag.aiEstimatedCost), "accent"],
      ["Per travel screenshot", diag.travelScreenshots ? usd(diag.aiEstimatedCost / diag.travelScreenshots) : "—"],
      ["Per place", diag.places ? usd(diag.aiEstimatedCost / diag.places) : "—"],
    ] },
    { title: "Speed (average per screenshot)", tone: "gray", stats: [
      ["Whole pipeline", ms(avg("pipeline.total")), "accent"], ["Photos export", ms(avg("photos.export"))], ["Text recognition", ms(avg("ocr"))],
      ["Photo analysis", ms(avg("images.analyze"))], ["AI extraction", ms(avg("ai.extract"))], ["Map lookup", ms(avg("maps.resolve"))],
    ] },
  ];

  return (
    <>
      <div className="section"><h3>Usage &amp; performance</h3></div>
      <div className="diag-groups">
        {groups.map((g) => (
          <section key={g.title} className={`diag-group tone-${g.tone}`}>
            <header><span className="diag-dot" /><h4>{g.title}</h4>{g.hint && <span className="muted small">{g.hint}</span>}</header>
            <div className="diag-row">
              {g.stats.map(([label, value, tone]) => (
                <div key={label} className={`diag-stat ${tone ? `is-${tone}` : ""}`}>
                  <div className="diag-value">{value}</div>
                  <div className="diag-label">{label}</div>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
    </>
  );
}
