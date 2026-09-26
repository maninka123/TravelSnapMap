//! Instagram Reel fetching: URL → caption/creator metadata and, when accessible, the video.
//! Uses `yt-dlp` when installed (most reliable), otherwise the page's public Open Graph tags.
//! Nothing here is sent to DeepSeek.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use crate::config::AppConfig;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReelMetadata {
    pub creator: Option<String>,
    pub caption: Option<String>,
    pub posted_at: Option<String>,
    pub thumbnail_url: Option<String>,
    pub video_url: Option<String>,
    pub duration_sec: Option<f64>,
}

#[async_trait]
pub trait ReelFetcher: Send + Sync {
    async fn metadata(&self, url: &str, config: &AppConfig) -> Result<ReelMetadata>;
    /// Downloads the video to `out`. Returns how it was obtained ("ytdlp" / "og"), or None if unavailable.
    async fn download(&self, url: &str, meta: &ReelMetadata, out: &Path, config: &AppConfig) -> Result<Option<String>>;
    async fn download_file(&self, url: &str, out: &Path) -> Result<()>;
    fn yt_dlp(&self, config: &AppConfig) -> Option<PathBuf>;

    /// Metadata and video together (one round-trip when the backend supports it).
    async fn fetch(&self, url: &str, out: &Path, config: &AppConfig) -> Result<(ReelMetadata, Option<String>)> {
        let meta = self.metadata(url, config).await?;
        let source = self.download(url, &meta, out, config).await?;
        Ok((meta, source))
    }
}

/// Shortcode from an Instagram Reel/Post URL (identifies the same Reel across links).
pub fn shortcode(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    let mut parts = path.split('/').filter(|p| !p.is_empty()).skip_while(|p| !matches!(*p, "reel" | "reels" | "p" | "tv"));
    parts.next()?;
    parts.next().filter(|c| c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')).map(String::from)
}

/// Every Instagram Reel/Post link in free text (a pasted list, a .txt/.md/.csv file, notes…),
/// normalised to https://www.instagram.com/… and de-duplicated by shortcode, in order of appearance.
pub fn extract_instagram_urls(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for raw in text.split(|c: char| c.is_whitespace() || "\"'<>()[]{},;|".contains(c)) {
        let Some(pos) = raw.to_lowercase().find("instagram.com/") else { continue };
        let tail = raw[pos..].trim_end_matches(['.', '!', '?', ':']);
        let url = format!("https://www.{tail}");
        if let Some(code) = shortcode(&url) {
            if seen.insert(code) {
                out.push(url);
            }
        }
    }
    out
}

pub fn is_instagram_url(url: &str) -> bool {
    let u = url.trim().to_lowercase();
    (u.starts_with("https://") || u.starts_with("http://")) && u.contains("instagram.com/") && shortcode(url).is_some()
}

pub struct InstagramFetcher {
    http: reqwest::Client,
}

impl Default for InstagramFetcher {
    fn default() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                // Instagram serves Open Graph tags (caption, preview) to link-preview crawlers.
                .user_agent("facebookexternalhit/1.1 (+http://www.facebook.com/externalhit_uatext.php)")
                .build()
                .expect("http client"),
        }
    }
}

impl InstagramFetcher {
    async fn run_yt_dlp(&self, bin: &Path, args: &[&str], config: &AppConfig) -> Result<String> {
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(args).arg("--no-warnings").arg("--no-playlist");
        if !config.cookies_from_browser.trim().is_empty() {
            cmd.arg("--cookies-from-browser").arg(config.cookies_from_browser.trim());
        }
        let output = tokio::time::timeout(Duration::from_secs(300), cmd.output()).await.context("yt-dlp timed out")??;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("yt-dlp: {}", err.lines().last().unwrap_or("failed")));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[async_trait]
impl ReelFetcher for InstagramFetcher {
    /// One yt-dlp run prints the metadata and downloads the video (saves a process start and a round-trip).
    async fn fetch(&self, url: &str, out: &Path, config: &AppConfig) -> Result<(ReelMetadata, Option<String>)> {
        if let Some(bin) = self.yt_dlp(config) {
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let out_str = out.to_string_lossy().to_string();
            let args = ["-j", "--no-simulate", "-f", "b[ext=mp4][vcodec!=none][acodec!=none]/b[ext=mp4]/b", "-o", out_str.as_str(), "--force-overwrites", url];
            // Instagram briefly rate-limits anonymous requests; retry with a pause before falling back.
            for attempt in 0..3u64 {
                match self.run_yt_dlp(&bin, &args, config).await {
                    Ok(json) => {
                        if let Some(v) = json.lines().rev().find_map(|l| serde_json::from_str::<Value>(l).ok().filter(|v| v.is_object())) {
                            return Ok((from_yt_dlp(&v), out.exists().then(|| "ytdlp".to_string())));
                        }
                        break; // no media info (e.g. login-only post): don't hammer Instagram
                    }
                    Err(e) => {
                        log::warn!(target: "reels", "yt-dlp attempt {} failed: {e}", attempt + 1);
                        let msg = e.to_string().to_lowercase();
                        let transient = msg.contains("rate") || msg.contains("429") || msg.contains("try again") || msg.contains("timed out") || msg.contains("unable to download");
                        if !transient || attempt == 2 {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(6 * (attempt + 1))).await;
                    }
                }
            }
        }
        let html = self.http.get(url).send().await?.text().await?;
        let meta = parse_open_graph(&html);
        let mut source = None;
        if let Some(video) = &meta.video_url {
            if self.download_file(video, out).await.is_ok() && out.exists() {
                source = Some("og".to_string());
            }
        }
        Ok((meta, source))
    }

    fn yt_dlp(&self, config: &AppConfig) -> Option<PathBuf> {
        let explicit = config.yt_dlp_path.trim();
        if !explicit.is_empty() {
            return Path::new(explicit).exists().then(|| PathBuf::from(explicit));
        }
        // GUI apps don't inherit the shell PATH, so check the usual install locations too.
        let mut candidates: Vec<PathBuf> = ["/opt/homebrew/bin/yt-dlp", "/usr/local/bin/yt-dlp"].iter().map(PathBuf::from).collect();
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(PathBuf::from(home).join(".local/bin/yt-dlp"));
        }
        if let Ok(path) = std::env::var("PATH") {
            candidates.extend(path.split(':').map(|d| PathBuf::from(d).join("yt-dlp")));
        }
        candidates.into_iter().find(|p| p.exists())
    }

    async fn metadata(&self, url: &str, config: &AppConfig) -> Result<ReelMetadata> {
        if let Some(bin) = self.yt_dlp(config) {
            match self.run_yt_dlp(&bin, &["-J", "--skip-download", url], config).await {
                Ok(json) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&json) {
                        return Ok(from_yt_dlp(&v));
                    }
                }
                Err(e) => log::warn!(target: "reels", "yt-dlp metadata failed, falling back to page tags: {e}"),
            }
        }
        let html = self.http.get(url).send().await?.text().await?;
        Ok(parse_open_graph(&html))
    }

    async fn download(&self, url: &str, meta: &ReelMetadata, out: &Path, config: &AppConfig) -> Result<Option<String>> {
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if let Some(bin) = self.yt_dlp(config) {
            let out_str = out.to_string_lossy().to_string();
            // A single progressive MP4 (video + audio) avoids needing ffmpeg for merging.
            let args = ["-f", "b[ext=mp4][vcodec!=none][acodec!=none]/b[ext=mp4]/b", "-o", out_str.as_str(), "--force-overwrites", url];
            match self.run_yt_dlp(&bin, &args, config).await {
                Ok(_) if out.exists() => return Ok(Some("ytdlp".into())),
                Ok(_) => {}
                Err(e) => log::warn!(target: "reels", "yt-dlp download failed: {e}"),
            }
        }
        if let Some(video) = &meta.video_url {
            if self.download_file(video, out).await.is_ok() && out.exists() {
                return Ok(Some("og".into()));
            }
        }
        Ok(None)
    }

    async fn download_file(&self, url: &str, out: &Path) -> Result<()> {
        let bytes = self.http.get(url).send().await?.error_for_status()?.bytes().await?;
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(out, &bytes)?;
        Ok(())
    }
}

fn from_yt_dlp(v: &Value) -> ReelMetadata {
    let s = |k: &str| v[k].as_str().map(str::trim).filter(|x| !x.is_empty()).map(String::from);
    let handle = s("channel").or_else(|| s("uploader_id")).or_else(|| s("uploader"));
    ReelMetadata {
        creator: handle.map(|h| if h.starts_with('@') || h.contains(' ') { h } else { format!("@{h}") }),
        caption: s("description").or_else(|| s("title")),
        posted_at: v["timestamp"].as_i64().and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|d| d.to_rfc3339()),
        thumbnail_url: s("thumbnail"),
        video_url: s("url"),
        duration_sec: v["duration"].as_f64(),
    }
}

/// Minimal Open Graph parsing for public Reel pages.
pub fn parse_open_graph(html: &str) -> ReelMetadata {
    let meta = |prop: &str| -> Option<String> {
        let needle = format!("property=\"{prop}\"");
        let start = html.find(&needle)?;
        // The content attribute may come before or after the property attribute within the tag.
        let tag_start = html[..start].rfind('<')?;
        let tag_end = start + html[start..].find('>')?;
        let tag = &html[tag_start..tag_end];
        let c = tag.find("content=\"")? + 9;
        let end = tag[c..].find('"')?;
        Some(unescape(&tag[c..c + end]))
    };
    let title = meta("og:title");
    let description = meta("og:description");
    // og:title looks like: `Some Creator on Instagram: "caption…"`
    let caption = title.as_deref().and_then(quoted).or_else(|| description.as_deref().and_then(quoted)).or(description.clone());
    let creator = description
        .as_deref()
        .and_then(|d| d.split(" - ").nth(1))
        .and_then(|rest| rest.split(" on ").next())
        .map(str::trim)
        .filter(|h| !h.is_empty() && !h.contains(' '))
        .map(|h| format!("@{}", h.trim_start_matches('@')))
        .or_else(|| title.as_deref().and_then(|t| t.split(" on Instagram").next()).map(|n| n.trim().to_string()).filter(|n| !n.is_empty()));
    ReelMetadata {
        creator,
        caption,
        posted_at: None,
        thumbnail_url: meta("og:image"),
        video_url: meta("og:video:secure_url").or_else(|| meta("og:video")),
        duration_sec: None,
    }
}

fn quoted(s: &str) -> Option<String> {
    let start = s.find(": \"")? + 3;
    let end = s.rfind('"')?;
    (end > start).then(|| s[start..end].trim().to_string()).filter(|c| !c.is_empty())
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let Some(end) = tail.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..end];
        let decoded = match entity {
            "amp" => Some('&'), "quot" => Some('"'), "lt" => Some('<'), "gt" => Some('>'), "apos" => Some('\''), "nbsp" => Some(' '),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => { out.push(c); rest = &tail[end + 1..]; }
            None => { out.push('&'); rest = &tail[1..]; }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcodes() {
        assert_eq!(shortcode("https://www.instagram.com/reel/C9abc_12-X/?igsh=xyz").as_deref(), Some("C9abc_12-X"));
        assert_eq!(shortcode("https://instagram.com/p/ABC123/").as_deref(), Some("ABC123"));
        assert_eq!(shortcode("https://www.instagram.com/travelwithxyz/reel/DEF456"), Some("DEF456".into()));
        assert_eq!(shortcode("https://www.instagram.com/travelwithxyz/"), None);
        assert!(is_instagram_url("https://www.instagram.com/reel/C9abc/"));
        assert!(!is_instagram_url("https://example.com/reel/C9abc/"));
    }

    #[test]
    fn open_graph_caption_and_creator() {
        let html = r#"<meta property="og:title" content="Travel Guy on Instagram: &quot;Lake Kawaguchi 🗻 Come early for clear Fuji views&quot;" />
            <meta content="https://cdn/x.jpg" property="og:image"/>
            <meta property="og:description" content="1,234 likes, 56 comments - travelwithxyz on March 3, 2025: &quot;Lake Kawaguchi 🗻&quot;"/>
            <meta property="og:video" content="https://cdn/v.mp4"/>"#;
        let m = parse_open_graph(html);
        assert_eq!(m.caption.as_deref(), Some("Lake Kawaguchi 🗻 Come early for clear Fuji views"));
        assert_eq!(m.creator.as_deref(), Some("@travelwithxyz"));
        assert_eq!(m.thumbnail_url.as_deref(), Some("https://cdn/x.jpg"));
        assert_eq!(m.video_url.as_deref(), Some("https://cdn/v.mp4"));
    }

    #[test]
    fn yt_dlp_metadata() {
        let v: Value = serde_json::json!({"channel": "travelwithxyz", "description": "Kyoto in 3 days", "timestamp": 1700000000, "duration": 42.5});
        let m = from_yt_dlp(&v);
        assert_eq!(m.creator.as_deref(), Some("@travelwithxyz"));
        assert_eq!(m.caption.as_deref(), Some("Kyoto in 3 days"));
        assert_eq!(m.duration_sec, Some(42.5));
        assert!(m.posted_at.unwrap().starts_with("2023-11-14"));
    }

    #[test]
    fn entities() {
        assert_eq!(unescape("Tom &amp; Jerry &#x27;s &#39;x&#39; &unknown; & done"), "Tom & Jerry 's 'x' &unknown; & done");
    }
}

#[cfg(test)]
mod bulk_tests {
    use super::*;

    #[test]
    fn extracts_all_links_from_messy_text() {
        let text = "My saves:\n1. https://www.instagram.com/reel/AAA111/?igsh=xyz\n- instagram.com/p/BBB222, and (https://instagram.com/reel/CCC333/).\n\
                    dup: https://www.instagram.com/reel/AAA111/ \n not a reel https://www.instagram.com/someone/ \n\"http://instagram.com/tv/DDD444\"";
        let urls = extract_instagram_urls(text);
        assert_eq!(urls.len(), 4, "{urls:?}");
        assert_eq!(shortcode(&urls[0]).as_deref(), Some("AAA111"));
        assert!(urls.iter().all(|u| u.starts_with("https://www.instagram.com/") && is_instagram_url(u)));
        assert_eq!(shortcode(&urls[3]).as_deref(), Some("DDD444"));
    }
}
