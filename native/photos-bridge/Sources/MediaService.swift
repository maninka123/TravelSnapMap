import AVFoundation
import CoreGraphics
import Foundation
import Photos
import Speech

/// Video/audio capabilities for Reel import. Returns raw data only (words with timestamps,
/// sampled frames); grouping and selection policy live in Rust.
enum MediaService {
    // MARK: Audio

    /// Extracts the audio track to an .m4a file (kept as the Reel's saved voice audio).
    static func extractAudio(path: String, outPath: String) async throws -> [String: Any] {
        let asset = AVURLAsset(url: URL(fileURLWithPath: path))
        let duration = try await asset.load(.duration).seconds
        guard try await !asset.loadTracks(withMediaType: .audio).isEmpty else {
            return ["path": NSNull(), "durationSec": duration, "hasAudio": false]
        }
        let out = URL(fileURLWithPath: outPath)
        try? FileManager.default.removeItem(at: out)
        try FileManager.default.createDirectory(at: out.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard let export = AVAssetExportSession(asset: asset, presetName: AVAssetExportPresetAppleM4A) else {
            throw BridgeError.message("Audio export is not available for this video")
        }
        try await export.export(to: out, as: .m4a)
        return ["path": outPath, "durationSec": duration, "hasAudio": true]
    }

    // MARK: Speech

    static func speechAuthorization() async -> String {
        let status: SFSpeechRecognizerAuthorizationStatus = await withCheckedContinuation { c in
            SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) }
        }
        switch status {
        case .authorized: return "authorized"
        case .denied: return "denied"
        case .restricted: return "restricted"
        case .notDetermined: return "notDetermined"
        @unknown default: return "unknown"
        }
    }

    /// Word-level transcript with timestamps. On-device recognition is required when the
    /// locale supports it, so audio never leaves the Mac.
    static func transcribe(path: String, locale: String) async throws -> [String: Any] {
        // Only the classic recogniser needs this permission (the on-device SpeechTranscriber does not).
        guard await speechAuthorization() == "authorized" else {
            throw BridgeError.message("Speech recognition permission not granted")
        }
        guard let recognizer = SFSpeechRecognizer(locale: Locale(identifier: locale)), recognizer.isAvailable else {
            throw BridgeError.message("Speech recognition is unavailable for \(locale)")
        }
        let request = SFSpeechURLRecognitionRequest(url: URL(fileURLWithPath: path))
        request.shouldReportPartialResults = false
        request.addsPunctuation = true
        let onDevice = recognizer.supportsOnDeviceRecognition
        request.requiresOnDeviceRecognition = onDevice

        let words: [[String: Any]] = try await withCheckedThrowingContinuation { continuation in
            var finished = false
            recognizer.recognitionTask(with: request) { result, error in
                guard !finished else { return }
                if let result, result.isFinal {
                    finished = true
                    let words: [[String: Any]] = result.bestTranscription.segments.map { segment in
                        let word: [String: Any] = [
                            "text": segment.substring, "start": segment.timestamp,
                            "duration": segment.duration, "confidence": Double(segment.confidence),
                        ]
                        return word
                    }
                    continuation.resume(returning: words)
                } else if let error {
                    finished = true
                    // "No speech detected" is a normal outcome for music-only Reels.
                    let nsError = error as NSError
                    if nsError.code == 1110 || nsError.localizedDescription.lowercased().contains("no speech") {
                        continuation.resume(returning: [])
                    } else {
                        continuation.resume(throwing: BridgeError.message("Transcription failed: \(error.localizedDescription)"))
                    }
                }
            }
        }
        return ["words": words, "onDevice": onDevice, "locale": locale]
    }

    // MARK: Keyframes

    /// Samples the video every `interval` seconds and keeps frames that differ visibly from the
    /// previously kept one (scene changes / new on-screen text), capped at `maxFrames`.
    static func keyframes(path: String, outDir: String, maxFrames: Int, interval: Double) async throws -> [String: Any] {
        let asset = AVURLAsset(url: URL(fileURLWithPath: path))
        let duration = try await asset.load(.duration).seconds
        let tolerance = CMTime(seconds: 0.25, preferredTimescale: 600)

        // Pass 1: find scene changes on tiny thumbnails, requested as one batch (fast).
        let sampler = AVAssetImageGenerator(asset: asset)
        sampler.appliesPreferredTrackTransform = true
        sampler.maximumSize = CGSize(width: 96, height: 170)
        sampler.requestedTimeToleranceBefore = tolerance
        sampler.requestedTimeToleranceAfter = tolerance
        var times: [CMTime] = []
        var t = min(0.5, duration / 2)
        while t < duration {
            times.append(CMTime(seconds: t, preferredTimescale: 600))
            t += interval
        }
        var candidates: [(time: Double, change: Double)] = []
        var last: [Double]?
        for await result in sampler.images(for: times) {
            guard let image = try? result.image else { continue }
            let time = result.requestedTime.seconds
            let sig = signature(image)
            let change: Double = last.map { difference($0, sig) } ?? 1
            let sinceLast = time - (candidates.last?.time ?? -10)
            if change > 0.035 || sinceLast >= 3 {
                candidates.append((time, max(change, sinceLast >= 3 ? 0.035 : 0)))
                last = sig
            }
        }
        // Keep the most distinct frames, then restore chronological order.
        let kept = candidates.sorted { $0.change > $1.change }.prefix(max(1, maxFrames)).map(\.time).sorted()

        // Pass 2: render only the kept frames at full size for OCR and snapshots.
        let renderer = AVAssetImageGenerator(asset: asset)
        renderer.appliesPreferredTrackTransform = true
        renderer.maximumSize = CGSize(width: 1080, height: 1920)
        renderer.requestedTimeToleranceBefore = tolerance
        renderer.requestedTimeToleranceAfter = tolerance
        try FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)
        var frames: [[String: Any]] = []
        for await result in renderer.images(for: kept.map { CMTime(seconds: $0, preferredTimescale: 600) }) {
            guard let image = try? result.image else { continue }
            let time = result.requestedTime.seconds
            let file = (outDir as NSString).appendingPathComponent(String(format: "frame-%06.2f.jpg", time))
            try ImageFiles.writeJPEG(image, to: file, quality: 0.82)
            frames.append(["timeSec": time, "path": file, "width": image.width, "height": image.height])
        }
        return ["durationSec": duration, "frames": frames]
    }

    private static func difference(_ a: [Double], _ b: [Double]) -> Double {
        guard !a.isEmpty, a.count == b.count else { return 1 }
        var total = 0.0
        for i in 0..<a.count { total += abs(a[i] - b[i]) }
        return total / Double(a.count)
    }

    /// 16×16 grayscale thumbnail used to detect visual change between frames.
    private static func signature(_ image: CGImage) -> [Double] {
        let side = 16
        var pixels = [UInt8](repeating: 0, count: side * side)
        guard let ctx = CGContext(data: &pixels, width: side, height: side, bitsPerComponent: 8, bytesPerRow: side,
                                  space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue) else { return [] }
        ctx.interpolationQuality = .low
        ctx.draw(image, in: CGRect(x: 0, y: 0, width: side, height: side))
        return pixels.map { Double($0) / 255 }
    }

    // MARK: Photos videos (fallback when Instagram media isn't available)

    static func listVideos(limit: Int) throws -> [[String: Any]] {
        let status = PHPhotoLibrary.authorizationStatus(for: .readWrite)
        guard status == .authorized || status == .limited else { throw BridgeError.message("Photos access not granted") }
        let options = PHFetchOptions()
        options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
        options.fetchLimit = limit
        var out: [[String: Any]] = []
        PHAsset.fetchAssets(with: .video, options: options).enumerateObjects { asset, _, _ in
            out.append([
                "id": asset.localIdentifier,
                "creationDate": asset.creationDate.map(isoFormatter.string(from:)) ?? NSNull(),
                "durationSec": asset.duration,
                "width": asset.pixelWidth,
                "height": asset.pixelHeight,
            ])
        }
        return out
    }

    static func exportVideo(id: String, outPath: String) async throws -> [String: Any] {
        guard let asset = PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject else {
            throw BridgeError.message("Video no longer exists in Photos")
        }
        let options = PHVideoRequestOptions()
        options.isNetworkAccessAllowed = true
        options.deliveryMode = .mediumQualityFormat
        let session: AVAssetExportSession = try await withCheckedThrowingContinuation { c in
            PHImageManager.default().requestExportSession(forVideo: asset, options: options, exportPreset: AVAssetExportPresetMediumQuality) { session, info in
                if let session { c.resume(returning: session) }
                else { c.resume(throwing: BridgeError.message((info?[PHImageErrorKey] as? Error)?.localizedDescription ?? "Video unavailable")) }
            }
        }
        let out = URL(fileURLWithPath: outPath)
        try? FileManager.default.removeItem(at: out)
        try FileManager.default.createDirectory(at: out.deletingLastPathComponent(), withIntermediateDirectories: true)
        try await session.export(to: out, as: .mp4)
        return ["path": outPath, "durationSec": asset.duration, "creationDate": asset.creationDate.map(isoFormatter.string(from:)) ?? NSNull()]
    }
}

import NaturalLanguage

extension MediaService {
    /// Locales Apple speech recognition supports on this Mac, and whether each runs on-device.
    static func supportedLocales() -> [[String: Any]] {
        SFSpeechRecognizer.supportedLocales()
            .map { locale -> [String: Any] in
                let recognizer = SFSpeechRecognizer(locale: locale)
                return [
                    "id": locale.identifier.replacingOccurrences(of: "_", with: "-"),
                    "name": Locale(identifier: "en_US").localizedString(forIdentifier: locale.identifier) ?? locale.identifier,
                    "onDevice": recognizer?.supportsOnDeviceRecognition ?? false,
                ]
            }
            .sorted { ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "") }
    }

    /// Dominant languages of a text (used as a hint for which speech locale to try first).
    static func detectLanguage(text: String) -> [[String: Any]] {
        let recognizer = NLLanguageRecognizer()
        recognizer.processString(text)
        return recognizer.languageHypotheses(withMaximum: 3)
            .sorted { $0.value > $1.value }
            .map { ["language": $0.key.rawValue, "probability": $0.value] }
    }
}
