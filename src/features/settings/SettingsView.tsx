import { useEffect, useState } from "react";
import { AppService, PhotoLibraryService, ProcessingService, ReelService, SettingsService } from "../../api/services";
import type { AppConfig, RunReport } from "../../api/types";
import { ErrorNote, Modal } from "../../components/common";
import { RunReportView } from "../../components/ScanControls";
import { formatDate } from "../../lib/labels";
import { useAction, useLoad } from "../../lib/nav";

export function SettingsView() {
  const { data: settings, reload } = useLoad(() => SettingsService.get(), []);
  const { data: diag } = useLoad(() => AppService.diagnostics(), []);
  const { data: permission } = useLoad(() => PhotoLibraryService.permissionStatus(), []);
  const { data: tools } = useLoad(() => ReelService.toolStatus(), []);
  const { data: locales = [] } = useLoad(() => ReelService.speechLocales(), []);
  const { data: runs = [] } = useLoad(() => ProcessingService.runs(), []);
  const [runReport, setRunReport] = useState<RunReport>();
  const [config, setConfig] = useState<AppConfig>();
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

        <div className="card">
          <h3>Cost estimate (USD per 1M tokens)</h3>
          <div className="field"><label>Input (cache miss)</label>{num("priceInputCacheMissPerMillion", 0.01)}</div>
          <div className="field"><label>Input (cache hit)</label>{num("priceInputCacheHitPerMillion", 0.001)}</div>
          <div className="field"><label>Output</label>{num("priceOutputPerMillion", 0.01)}</div>
          <span className="hint small muted">Check DeepSeek's pricing page and update these for accurate estimates.</span>
        </div>
      </div>

      {runs.length > 0 && (
        <>
          <div className="section"><h3>Test runs &amp; scans</h3></div>
          <div className="list">
            {runs.map((r) => (
              <div key={r.id} className="list-row" onClick={async () => setRunReport((await ProcessingService.runReport(r.id)) ?? undefined)}>
                <div className="grow"><div className="strong">{r.kind === "validation" ? `Test run ·  ()` : r.kind === "scanFolder" ? "Folder scan" : "Scan New Screenshots"}</div>
                  <div className="muted small">{formatDate(r.startedAt)} · {r.screenshotCount} screenshots{r.finishedAt ? "" : " · running"}</div></div>
                <span className="muted small">View report →</span>
              </div>
            ))}
          </div>
        </>
      )}
      {runReport && <Modal title="Run report" onClose={() => setRunReport(undefined)} wide><RunReportView report={runReport} /></Modal>}

      {diag && (
        <>
          <div className="section"><h3>Diagnostics</h3></div>
          <div className="stat-grid">
            {[
              ["Screenshots", diag.totalScreenshots], ["Travel", diag.travelScreenshots], ["Skipped locally", diag.skippedLocally],
              ["Not travel", diag.notTravel], ["Needs review", diag.needsReview], ["Failed", diag.failed], ["Remaining", diag.remaining],
              ["Places", diag.places], ["AI requests", diag.aiRequests], ["Text-only", diag.aiTextRequests], ["With image", diag.aiVisionRequests],
              ["With thinking", diag.aiThinkingRequests], ["AI failures", diag.aiFailures],
              ["Input tokens", diag.aiInputTokens.toLocaleString()], ["Output tokens", diag.aiOutputTokens.toLocaleString()],
              ["Cache-hit tokens", diag.aiCacheHitTokens.toLocaleString()], ["Est. AI cost", `$${diag.aiEstimatedCost.toFixed(4)}`],
              ["Cost / travel shot", diag.travelScreenshots ? `$${(diag.aiEstimatedCost / diag.travelScreenshots).toFixed(5)}` : "—"],
              ["Cost / place", diag.places ? `$${(diag.aiEstimatedCost / diag.places).toFixed(5)}` : "—"],
              ["Avg AI latency", `${(diag.aiAverageLatencyMs / 1000).toFixed(1)} s`],
            ].map(([label, value]) => (
              <div key={label as string} className="stat"><div className="stat-value">{value}</div><div className="stat-label">{label}</div></div>
            ))}
          </div>
          <div className="section"><h4>Timings & counters</h4></div>
          <div className="stat-grid">
            {timings(diag.metrics).map(([label, value]) => (
              <div key={label} className="stat"><div className="stat-value">{value}</div><div className="stat-label">{label}</div></div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/** Turns `x.ms` / `x.count` pairs into averages; other counters are shown as-is. */
function timings(metrics: [string, number][]): [string, string][] {
  const m = new Map(metrics);
  const out: [string, string][] = [];
  for (const [k, v] of metrics) {
    if (k.endsWith(".ms")) {
      const base = k.slice(0, -3);
      const n = m.get(`${base}.count`) ?? 1;
      out.push([`avg ${base}`, `${Math.round(v / n)} ms`]);
    } else if (!k.endsWith(".count")) {
      out.push([k, String(v)]);
    }
  }
  return out;
}
