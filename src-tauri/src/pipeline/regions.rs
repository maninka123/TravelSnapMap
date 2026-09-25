//! Local detection of photographic regions inside a screenshot (Instagram photo, TikTok frame…).
//! Pure image statistics + OCR geometry + Vision saliency. The AI is only asked when this is unsure.

use std::collections::HashSet;

use image::RgbImage;
use serde::Serialize;

use crate::models::{Rect, RegionType, SourceType};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RegionCandidate {
    pub rect: Rect,
    pub region_type: RegionType,
    pub confidence: f64,
    pub quality: f64,
    pub text_coverage: f64,
}

const GRID_W: u32 = 90;

struct Cell {
    rgb: [f64; 3],
    luma: f64,
    text: bool,
}

/// Fraction of `region` covered by OCR text boxes.
pub fn text_coverage(region: &Rect, text_boxes: &[Rect]) -> f64 {
    if region.area() <= 0.0 {
        return 0.0;
    }
    let covered: f64 = text_boxes.iter().map(|b| region.intersection(b).area()).sum();
    (covered / region.area()).min(1.0)
}

#[cfg(test)]
/// Converts a normalised top-left rect to integer pixel bounds for a given image size.
pub fn pixel_rect(r: &Rect, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let c = r.clamped();
    let x = (c.x * width as f64).round() as u32;
    let y = (c.y * height as f64).round() as u32;
    let w = ((c.width * width as f64).round() as u32).min(width.saturating_sub(x));
    let h = ((c.height * height as f64).round() as u32).min(height.saturating_sub(y));
    (x, y, w, h)
}

/// Finds and classifies candidate photo regions, best first.
pub fn detect(image: &RgbImage, text_boxes: &[Rect], saliency: &[Rect], source: SourceType) -> Vec<RegionCandidate> {
    let (w, h) = image.dimensions();
    if w < 16 || h < 16 {
        return vec![];
    }
    let gw = GRID_W.min(w);
    let gh = ((gw as f64) * h as f64 / w as f64).round().max(8.0) as u32;
    let small = image::imageops::resize(image, gw, gh, image::imageops::FilterType::Triangle);

    let padded: Vec<Rect> = text_boxes.iter().map(|b| Rect::new(b.x - 0.01, b.y - 0.004, b.width + 0.02, b.height + 0.008)).collect();
    let grid: Vec<Vec<Cell>> = (0..gh)
        .map(|y| {
            (0..gw)
                .map(|x| {
                    let p = small.get_pixel(x, y).0;
                    let rgb = [p[0] as f64, p[1] as f64, p[2] as f64];
                    let (cx, cy) = ((x as f64 + 0.5) / gw as f64, (y as f64 + 0.5) / gh as f64);
                    let text = padded.iter().any(|b| cx >= b.x && cx <= b.max_x() && cy >= b.y && cy <= b.max_y());
                    Cell { rgb, luma: 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2], text }
                })
                .collect()
        })
        .collect();

    // 1. Rows that look photographic: varied colours/detail and little text.
    // A row is excluded when it is mostly text, or flat and grey like app chrome. Smooth but
    // coloured rows (a blue sky) still count as photo content.
    let photographic: Vec<bool> = grid
        .iter()
        .map(|row| {
            let text_frac = row.iter().filter(|c| c.text).count() as f64 / row.len() as f64;
            let cells: Vec<&Cell> = row.iter().filter(|c| !c.text).collect();
            if text_frac > 0.35 || cells.len() < 4 {
                return false;
            }
            let mean = cells.iter().map(|c| c.luma).sum::<f64>() / cells.len() as f64;
            let sd = (cells.iter().map(|c| (c.luma - mean).powi(2)).sum::<f64>() / cells.len() as f64).sqrt();
            let chroma = cells.iter().map(|c| chroma(c.rgb)).sum::<f64>() / cells.len() as f64;
            !(sd < 5.0 && chroma < 25.0)
        })
        .collect();

    // 2. Contiguous vertical bands (tolerating tiny gaps), at least 15% of the height.
    let mut bands: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut gap = 0;
    for (i, &p) in photographic.iter().enumerate() {
        match (p, start) {
            (true, None) => { start = Some(i); gap = 0; }
            (true, Some(_)) => gap = 0,
            (false, Some(s)) => {
                gap += 1;
                if gap > 2 {
                    bands.push((s, i - gap));
                    start = None;
                }
            }
            (false, None) => {}
        }
    }
    if let Some(s) = start {
        bands.push((s, photographic.len() - 1 - gap));
    }
    let min_rows = (gh as f64 * 0.15).ceil() as usize;

    let mut out: Vec<RegionCandidate> = bands
        .into_iter()
        .filter(|(s, e)| e + 1 - s >= min_rows)
        .filter_map(|(s, e)| {
            // 3. Trim flat side margins column by column.
            let col_varied = |x: usize| {
                let lum: Vec<f64> = (s..=e).map(|y| grid[y][x].luma).collect();
                let m = lum.iter().sum::<f64>() / lum.len() as f64;
                (lum.iter().map(|l| (l - m).powi(2)).sum::<f64>() / lum.len() as f64).sqrt() >= 6.0
            };
            let left = (0..gw as usize).find(|&x| col_varied(x))?;
            let right = (0..gw as usize).rev().find(|&x| col_varied(x))?;
            if right <= left {
                return None;
            }
            let rect = Rect::new(
                left as f64 / gw as f64,
                s as f64 / gh as f64,
                (right + 1 - left) as f64 / gw as f64,
                (e + 1 - s) as f64 / gh as f64,
            );
            Some(classify(&grid, (left, right, s, e), rect, text_boxes, saliency, source))
        })
        .filter(|r| r.rect.width >= 0.3 && r.rect.area() >= 0.06)
        .collect();

    out.sort_by(|a, b| (b.confidence * b.rect.area()).total_cmp(&(a.confidence * a.rect.area())));
    out.truncate(3);
    out
}

fn chroma(rgb: [f64; 3]) -> f64 {
    rgb.iter().cloned().fold(0.0, f64::max) - rgb.iter().cloned().fold(255.0, f64::min)
}

fn quantize(rgb: [f64; 3]) -> (u8, u8, u8) {
    ((rgb[0] as u8) >> 5, (rgb[1] as u8) >> 5, (rgb[2] as u8) >> 5)
}

fn classify(grid: &[Vec<Cell>], (l, r, t, b): (usize, usize, usize, usize), rect: Rect, text_boxes: &[Rect], saliency: &[Rect], source: SourceType) -> RegionCandidate {
    let cells: Vec<&Cell> = (t..=b).flat_map(|y| (l..=r).map(move |x| &grid[y][x])).collect();
    let n = cells.len().max(1) as f64;
    let colours: HashSet<(u8, u8, u8)> = cells.iter().map(|c| quantize(c.rgb)).collect();
    let chroma = cells.iter().map(|c| chroma(c.rgb)).sum::<f64>() / n;
    // Sharpness: mean horizontal luminance gradient.
    let mut grad = 0.0;
    let mut count = 0.0;
    for y in t..=b {
        for x in (l + 1)..=r {
            grad += (grid[y][x].luma - grid[y][x - 1].luma).abs();
            count += 1.0;
        }
    }
    let sharpness = if count > 0.0 { grad / count } else { 0.0 };
    let tc = text_coverage(&rect, text_boxes);
    let salient = saliency.iter().map(|s| if s.area() > 0.0 { rect.intersection(s).area() / s.area() } else { 0.0 }).fold(0.0, f64::max);

    let (region_type, confidence) = if tc > 0.25 {
        (RegionType::Text, 0.7)
    } else if source.is_map_app() {
        (RegionType::Map, 0.7)
    } else if colours.len() < 14 {
        (if chroma > 40.0 { RegionType::Illustration } else { RegionType::Interface }, 0.6)
    } else {
        let c = 0.45
            + 0.2 * (colours.len() as f64 / 60.0).min(1.0)
            + 0.15 * (1.0 - tc * 4.0).max(0.0)
            + 0.15 * salient
            + if rect.area() > 0.2 { 0.05 } else { 0.0 };
        (RegionType::Photograph, c.min(0.97))
    };
    let quality = (0.4 * (rect.area() / 0.5).min(1.0) + 0.3 * (sharpness / 25.0).min(1.0) + 0.3 * (chroma / 80.0).min(1.0)).clamp(0.0, 1.0);
    RegionCandidate { rect, region_type, confidence, quality, text_coverage: tc }
}

/// Cosine distance between two feature prints (0 = identical).
pub fn feature_distance(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64).powi(2);
        nb += (*y as f64).powi(2);
    }
    if na == 0.0 || nb == 0.0 { 1.0 } else { 1.0 - dot / (na.sqrt() * nb.sqrt()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic Instagram-like screenshot: UI header, a colourful "photo", caption text below.
    fn synthetic() -> (RgbImage, Vec<Rect>) {
        let (w, h) = (360u32, 780u32);
        let mut img = RgbImage::from_pixel(w, h, image::Rgb([250, 250, 250]));
        for y in 120..520 {
            for x in 0..w {
                // Pseudo-random texture with smooth colour gradients, like a landscape photo.
                let n = ((x * 7919 + y * 104729) % 97) as u8;
                img.put_pixel(x, y, image::Rgb([
                    (40 + (y - 120) / 3) as u8 + n / 2,
                    (90 + x / 4) as u8 + n / 3,
                    (200 - (y - 120) / 4) as u8 - n / 4,
                ]));
            }
        }
        let text = vec![
            Rect::new(0.05, 0.03, 0.4, 0.03),   // username
            Rect::new(0.05, 0.70, 0.8, 0.025),  // caption
            Rect::new(0.05, 0.74, 0.6, 0.025),
        ];
        (img, text)
    }

    #[test]
    fn finds_the_photo_band() {
        let (img, text) = synthetic();
        let regions = detect(&img, &text, &[], SourceType::Instagram);
        let best = &regions[0];
        assert_eq!(best.region_type, RegionType::Photograph);
        assert!(best.confidence >= 0.75, "confidence {}", best.confidence);
        assert!((best.rect.y - 120.0 / 780.0).abs() < 0.03, "top {}", best.rect.y);
        assert!((best.rect.max_y() - 520.0 / 780.0).abs() < 0.03, "bottom {}", best.rect.max_y());
        assert!(best.rect.width > 0.95);
    }

    #[test]
    fn map_apps_are_never_photographs() {
        let (img, text) = synthetic();
        let regions = detect(&img, &text, &[], SourceType::GoogleMaps);
        assert!(regions.iter().all(|r| r.region_type != RegionType::Photograph));
    }

    #[test]
    fn plain_ui_has_no_regions() {
        let img = RgbImage::from_pixel(360, 780, image::Rgb([255, 255, 255]));
        assert!(detect(&img, &[], &[], SourceType::Unknown).is_empty());
    }

    #[test]
    fn geometry_helpers() {
        let r = Rect::new(0.0, 0.0, 0.5, 0.5);
        assert!((text_coverage(&r, &[Rect::new(0.0, 0.0, 0.25, 0.5)]) - 0.5).abs() < 1e-9);
        assert_eq!(pixel_rect(&Rect::new(0.25, 0.5, 0.5, 0.25), 400, 800), (100, 400, 200, 200));
        assert_eq!(pixel_rect(&Rect::new(0.9, 0.9, 0.5, 0.5), 100, 100), (90, 90, 10, 10));
    }

    #[test]
    fn feature_distance_basics() {
        assert!(feature_distance(&[1.0, 0.0], &[1.0, 0.0]) < 1e-9);
        assert!((feature_distance(&[1.0, 0.0], &[0.0, 1.0]) - 1.0).abs() < 1e-9);
        assert_eq!(feature_distance(&[1.0], &[1.0, 2.0]), 1.0);
    }
}
