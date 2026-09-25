import CoreGraphics
import Foundation
import Vision

/// Local, on-device Vision requests. Outputs normalised rects with a top-left origin.
enum VisionService {
    /// Vision rects are bottom-left based; the rest of the app uses top-left.
    private static func topLeft(_ r: CGRect) -> CGRect {
        CGRect(x: r.minX, y: 1 - r.maxY, width: r.width, height: r.height)
    }

    static func recognizeText(path: String) async throws -> [String: Any] {
        let image = try ImageFiles.load(path)
        var request = RecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        request.automaticallyDetectsLanguage = true

        let start = Date()
        let observations = try await request.perform(on: image)
        let blocks: [[String: Any]] = observations.compactMap { observation in
            guard let candidate = observation.topCandidates(1).first else { return nil }
            let text = candidate.string.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty else { return nil }
            var block = rectJSON(topLeft(observation.boundingBox.cgRect)) as [String: Any]
            block["text"] = text
            block["confidence"] = Double(candidate.confidence)
            return block
        }
        return [
            "blocks": blocks,
            "imageWidth": image.width,
            "imageHeight": image.height,
            "elapsedMs": Int(Date().timeIntervalSince(start) * 1000),
        ]
    }

    static func saliency(path: String) async throws -> [String: Any] {
        let image = try ImageFiles.load(path, maxPixelSize: 1024)
        let observation = try await GenerateAttentionBasedSaliencyImageRequest().perform(on: image)
        let regions: [[String: Any]] = observation.salientObjects.map { object in
            var r = rectJSON(topLeft(object.boundingBox.cgRect)) as [String: Any]
            r["confidence"] = Double(object.confidence)
            return r
        }
        return ["regions": regions]
    }

    /// Feature print for near-duplicate photo detection. Distances are computed in Rust.
    static func featurePrint(path: String, crop: [String: Any]?) async throws -> [String: Any] {
        var image = try ImageFiles.load(path, maxPixelSize: 1024)
        if let crop {
            let rect = CGRect(x: crop.double("x"), y: crop.double("y"), width: crop.double("width"), height: crop.double("height"))
            if let cropped = image.cropping(to: ImageFiles.pixelRect(rect, image)) { image = cropped }
        }
        let observation = try await GenerateImageFeaturePrintRequest().perform(on: image)
        let vector: [Float] = observation.data.withUnsafeBytes { raw in
            switch observation.elementType {
            case .float: Array(raw.bindMemory(to: Float.self))
            case .double: raw.bindMemory(to: Double.self).map(Float.init)
            default: []
            }
        }
        return ["vector": vector]
    }
}
