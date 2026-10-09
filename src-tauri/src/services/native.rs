//! Service interfaces for Apple-specific capabilities, and their implementation via the Swift
//! `photos-bridge` sidecar (newline-delimited JSON over stdin/stdout).
//! Nothing outside this file knows the bridge exists; the UI never sees Apple framework details.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{broadcast, oneshot, Mutex};

use crate::models::{PlaceCandidate, Rect};
use crate::services::places::{PlaceError, PlaceSearchProvider};

// MARK: - Interfaces

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetInfo {
    pub id: String,
    pub creation_date: Option<String>,
    #[serde(default)]
    pub width: i64,
    #[serde(default)]
    pub height: i64,
}

/// One of your own photos (not a screenshot), for attaching to a visited place.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnPhoto {
    pub id: String,
    pub creation_date: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub distance_m: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OcrBlockData {
    pub text: String,
    pub confidence: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub language: Option<String>,
}

#[async_trait]
pub trait PhotoLibraryService: Send + Sync {
    async fn authorization_status(&self) -> Result<String>;
    async fn request_permission(&self) -> Result<String>;
    async fn list_screenshots(&self, from_year: Option<i32>, to_year: Option<i32>) -> Result<Vec<AssetInfo>>;
    async fn metadata(&self, asset_id: &str) -> Result<Value>;
    /// Writes an orientation-corrected, downsized JPEG of the screenshot to `path`.
    async fn export_image(&self, asset_id: &str, path: &Path, max_pixel_size: u32) -> Result<()>;
    async fn crop_image(&self, source: &Path, dest: &Path, rect: Rect, max_pixel_size: u32) -> Result<()>;
    /// Emits `photosLibraryChanged` on the event channel when screenshots may have been added.
    async fn observe_new_screenshots(&self) -> Result<()>;
    /// Your own photos taken within `radius_km` of a point (for "My visit").
    async fn photos_near(&self, _lat: f64, _lon: f64, _radius_km: f64, _limit: u32) -> Result<Vec<OwnPhoto>> {
        anyhow::bail!("Not available")
    }
    /// Your own photos taken between two days (YYYY-MM-DD, inclusive).
    async fn photos_between(&self, _from: &str, _to: &str, _limit: u32) -> Result<Vec<OwnPhoto>> {
        anyhow::bail!("Not available")
    }
    /// Small square-ish preview of any Photos asset.
    async fn thumbnail(&self, _asset_id: &str, _path: &Path, _size: u32) -> Result<()> {
        anyhow::bail!("Not available")
    }
}

#[async_trait]
pub trait OcrService: Send + Sync {
    async fn recognize(&self, image: &Path) -> Result<Vec<OcrBlockData>>;
}

#[derive(Debug, Clone, Deserialize)]
pub struct WordTiming {
    pub text: String,
    pub start: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcription {
    pub words: Vec<WordTiming>,
    #[serde(default)]
    pub on_device: bool,
    #[serde(default)]
    pub locale: String,
    #[serde(default)]
    pub engine: String,
}

impl Transcription {
    /// Mean word confidence (0 when there are no words).
    pub fn confidence(&self) -> f64 {
        if self.words.is_empty() { 0.0 } else { self.words.iter().map(|w| w.confidence).sum::<f64>() / self.words.len() as f64 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechLocale {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub on_device: bool,
    #[serde(default)]
    pub engine: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameInfo {
    pub time_sec: f64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoVideo {
    pub id: String,
    pub creation_date: Option<String>,
    #[serde(default)]
    pub duration_sec: f64,
    #[serde(default)]
    pub width: i64,
    #[serde(default)]
    pub height: i64,
}

/// Audio/video capabilities for Reels (AVFoundation + Speech via the bridge).
#[async_trait]
pub trait MediaService: Send + Sync {
    /// Saves the audio track as .m4a. Returns (path if the video has audio, duration).
    async fn extract_audio(&self, video: &Path, out: &Path) -> Result<(Option<PathBuf>, f64)>;
    /// Word-level, timestamped, on-device transcription.
    async fn transcribe(&self, audio: &Path, locale: &str) -> Result<Transcription>;
    /// Visually distinct frames only (never every frame). Returns (duration, frames).
    async fn keyframes(&self, video: &Path, out_dir: &Path, max_frames: u32) -> Result<(f64, Vec<FrameInfo>)>;
    async fn list_videos(&self, limit: u32) -> Result<Vec<PhotoVideo>>;
    /// Locales Apple speech recognition supports on this Mac.
    async fn speech_locales(&self) -> Result<Vec<SpeechLocale>>;
    /// Language hypotheses (BCP-47 language, probability) for a text.
    async fn detect_language(&self, text: &str) -> Result<Vec<(String, f64)>>;
    /// Exports a Photos video to `out`. Returns its creation date if known.
    async fn export_video(&self, asset_id: &str, out: &Path) -> Result<Option<String>>;
}

#[async_trait]
pub trait VisionService: Send + Sync {
    async fn saliency(&self, image: &Path) -> Result<Vec<(Rect, f64)>>;
    async fn feature_print(&self, image: &Path, rect: Option<Rect>) -> Result<Vec<f32>>;
}

// MARK: - Swift bridge process

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>>;

struct BridgeProcess {
    stdin: ChildStdin,
    alive: Arc<AtomicBool>,
    _child: Child,
}

pub struct NativeBridge {
    binary: PathBuf,
    process: Mutex<Option<BridgeProcess>>,
    pending: Pending,
    next_id: AtomicU64,
    events: broadcast::Sender<String>,
}

impl NativeBridge {
    pub fn new(binary: PathBuf) -> Self {
        let (events, _) = broadcast::channel(16);
        Self {
            binary,
            process: Mutex::new(None),
            pending: Arc::new(std::sync::Mutex::new(HashMap::new())),
            next_id: AtomicU64::new(1),
            events,
        }
    }

    /// Locates the sidecar: bundled next to the app binary, or the dev build in `src-tauri/binaries`.
    pub fn locate_binary() -> PathBuf {
        if let Ok(p) = std::env::var("TRAVELSNAPMAP_BRIDGE") {
            return PathBuf::from(p);
        }
        if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
            let bundled = dir.join("photos-bridge");
            if bundled.exists() {
                return bundled;
            }
        }
        let triple = if cfg!(target_arch = "aarch64") { "aarch64-apple-darwin" } else { "x86_64-apple-darwin" };
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join(format!("photos-bridge-{triple}"))
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.events.subscribe()
    }

    async fn spawn(&self) -> Result<BridgeProcess> {
        let mut child = Command::new(&self.binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("starting native bridge at {}", self.binary.display()))?;
        let stdin = child.stdin.take().context("bridge stdin")?;
        let stdout = child.stdout.take().context("bridge stdout")?;
        let alive = Arc::new(AtomicBool::new(true));

        let (pending, events, alive_flag) = (self.pending.clone(), self.events.clone(), alive.clone());
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(event) = msg["event"].as_str() {
                    let _ = events.send(event.to_string());
                    continue;
                }
                let Some(id) = msg["id"].as_u64() else { continue };
                if let Some(tx) = pending.lock().unwrap().remove(&id) {
                    let _ = tx.send(match msg.get("error").and_then(Value::as_str) {
                        Some(err) => Err(err.to_string()),
                        None => Ok(msg["result"].clone()),
                    });
                }
            }
            alive_flag.store(false, Ordering::SeqCst);
            for (_, tx) in pending.lock().unwrap().drain() {
                let _ = tx.send(Err("native bridge exited".into()));
            }
        });
        Ok(BridgeProcess { stdin, alive, _child: child })
    }

    /// Sends one request, (re)starting the helper if needed.
    pub async fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        {
            let mut guard = self.process.lock().await;
            if guard.as_ref().is_none_or(|p| !p.alive.load(Ordering::SeqCst)) {
                *guard = Some(self.spawn().await?);
            }
            let process = guard.as_mut().unwrap();
            let mut line = serde_json::to_vec(&json!({"id": id, "method": method, "params": params}))?;
            line.push(b'\n');
            if let Err(e) = process.stdin.write_all(&line).await {
                self.pending.lock().unwrap().remove(&id);
                *guard = None;
                return Err(anyhow!("native bridge write failed: {e}"));
            }
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(message))) => Err(anyhow!(message)),
            Ok(Err(_)) => Err(anyhow!("native bridge dropped the request")),
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(anyhow!("native bridge timed out on {method}"))
            }
        }
    }
}

const SHORT: Duration = Duration::from_secs(30);
/// Vision's first OCR call loads language models and can take ~40 s; later calls take ~0.2 s.
const OCR_TIMEOUT: Duration = Duration::from_secs(180);
/// iCloud downloads can be slow.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(180);

fn rect_json(r: Rect) -> Value {
    json!({"x": r.x, "y": r.y, "width": r.width, "height": r.height})
}

#[async_trait]
impl PhotoLibraryService for NativeBridge {
    async fn authorization_status(&self) -> Result<String> {
        Ok(self.call("photos.authorizationStatus", json!({}), SHORT).await?.as_str().unwrap_or("unknown").to_string())
    }

    async fn request_permission(&self) -> Result<String> {
        let v = self.call("photos.requestPermission", json!({}), Duration::from_secs(600)).await?;
        Ok(v.as_str().unwrap_or("unknown").to_string())
    }

    async fn list_screenshots(&self, from_year: Option<i32>, to_year: Option<i32>) -> Result<Vec<AssetInfo>> {
        let v = self.call("photos.listScreenshots", json!({"fromYear": from_year, "toYear": to_year}), Duration::from_secs(120)).await?;
        Ok(serde_json::from_value(v)?)
    }

    async fn metadata(&self, asset_id: &str) -> Result<Value> {
        self.call("photos.metadata", json!({"id": asset_id}), SHORT).await
    }

    async fn export_image(&self, asset_id: &str, path: &Path, max_pixel_size: u32) -> Result<()> {
        self.call("photos.exportImage", json!({"id": asset_id, "path": path, "maxPixelSize": max_pixel_size, "quality": 0.85}), EXPORT_TIMEOUT).await?;
        Ok(())
    }

    async fn crop_image(&self, source: &Path, dest: &Path, rect: Rect, max_pixel_size: u32) -> Result<()> {
        self.call("image.crop", json!({"path": source, "outPath": dest, "rect": rect_json(rect), "maxPixelSize": max_pixel_size}), SHORT).await?;
        Ok(())
    }

    async fn observe_new_screenshots(&self) -> Result<()> {
        self.call("photos.observe", json!({}), SHORT).await?;
        Ok(())
    }

    async fn photos_near(&self, lat: f64, lon: f64, radius_km: f64, limit: u32) -> Result<Vec<OwnPhoto>> {
        let v = self.call("photos.listNear", json!({"latitude": lat, "longitude": lon, "radiusKm": radius_km, "limit": limit}), Duration::from_secs(120)).await?;
        Ok(serde_json::from_value(v)?)
    }

    async fn photos_between(&self, from: &str, to: &str, limit: u32) -> Result<Vec<OwnPhoto>> {
        let v = self.call("photos.listBetween", json!({"from": from, "to": to, "limit": limit}), Duration::from_secs(60)).await?;
        Ok(serde_json::from_value(v)?)
    }

    async fn thumbnail(&self, asset_id: &str, path: &Path, size: u32) -> Result<()> {
        self.call("photos.thumbnail", json!({"id": asset_id, "path": path, "size": size}), EXPORT_TIMEOUT).await?;
        Ok(())
    }
}

#[async_trait]
impl OcrService for NativeBridge {
    async fn recognize(&self, image: &Path) -> Result<Vec<OcrBlockData>> {
        let v = self.call("vision.ocr", json!({"path": image}), OCR_TIMEOUT).await?;
        Ok(serde_json::from_value(v["blocks"].clone())?)
    }
}

#[async_trait]
impl VisionService for NativeBridge {
    async fn saliency(&self, image: &Path) -> Result<Vec<(Rect, f64)>> {
        let v = self.call("vision.saliency", json!({"path": image}), SHORT).await?;
        let regions = v["regions"].as_array().cloned().unwrap_or_default();
        Ok(regions
            .iter()
            .map(|r| {
                let f = |k: &str| r[k].as_f64().unwrap_or(0.0);
                (Rect::new(f("x"), f("y"), f("width"), f("height")), f("confidence"))
            })
            .collect())
    }

    async fn feature_print(&self, image: &Path, rect: Option<Rect>) -> Result<Vec<f32>> {
        let v = self.call("vision.featurePrint", json!({"path": image, "rect": rect.map(rect_json)}), SHORT).await?;
        Ok(serde_json::from_value(v["vector"].clone())?)
    }
}

/// Apple MapKit search via the bridge.
#[async_trait]
impl PlaceSearchProvider for NativeBridge {
    async fn search(&self, query: &str, near: Option<&str>) -> Result<Vec<PlaceCandidate>, PlaceError> {
        match self.call("maps.search", json!({"query": query, "limit": 8, "near": near}), SHORT).await {
            Ok(v) => serde_json::from_value(v).map_err(|e| PlaceError::Unavailable(e.to_string())),
            Err(e) if e.to_string().contains("throttled") => Err(PlaceError::Throttled),
            Err(e) => Err(PlaceError::Unavailable(e.to_string())),
        }
    }
}

const MEDIA_TIMEOUT: Duration = Duration::from_secs(600);

#[async_trait]
impl MediaService for NativeBridge {
    async fn extract_audio(&self, video: &Path, out: &Path) -> Result<(Option<PathBuf>, f64)> {
        let v = self.call("media.extractAudio", json!({"path": video, "outPath": out}), MEDIA_TIMEOUT).await?;
        let path = v["path"].as_str().map(PathBuf::from);
        Ok((path, v["durationSec"].as_f64().unwrap_or(0.0)))
    }

    async fn transcribe(&self, audio: &Path, locale: &str) -> Result<Transcription> {
        let v = self.call("speech.transcribe", json!({"path": audio, "locale": locale}), MEDIA_TIMEOUT).await?;
        Ok(serde_json::from_value(v)?)
    }

    async fn keyframes(&self, video: &Path, out_dir: &Path, max_frames: u32) -> Result<(f64, Vec<FrameInfo>)> {
        let v = self.call("video.keyframes", json!({"path": video, "outDir": out_dir, "maxFrames": max_frames, "interval": 0.75}), MEDIA_TIMEOUT).await?;
        Ok((v["durationSec"].as_f64().unwrap_or(0.0), serde_json::from_value(v["frames"].clone())?))
    }

    async fn list_videos(&self, limit: u32) -> Result<Vec<PhotoVideo>> {
        let v = self.call("photos.listVideos", json!({"limit": limit}), SHORT).await?;
        Ok(serde_json::from_value(v)?)
    }

    async fn speech_locales(&self) -> Result<Vec<SpeechLocale>> {
        let v = self.call("speech.supportedLocales", json!({}), SHORT).await?;
        Ok(serde_json::from_value(v)?)
    }

    async fn detect_language(&self, text: &str) -> Result<Vec<(String, f64)>> {
        let v = self.call("text.detectLanguage", json!({"text": text}), SHORT).await?;
        Ok(v.as_array().cloned().unwrap_or_default().iter()
            .filter_map(|h| Some((h["language"].as_str()?.to_string(), h["probability"].as_f64()?)))
            .collect())
    }

    async fn export_video(&self, asset_id: &str, out: &Path) -> Result<Option<String>> {
        let v = self.call("photos.exportVideo", json!({"id": asset_id, "outPath": out}), MEDIA_TIMEOUT).await?;
        Ok(v["creationDate"].as_str().map(String::from))
    }
}
