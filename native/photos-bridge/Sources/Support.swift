import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

enum BridgeError: LocalizedError {
    case message(String)
    var errorDescription: String? { if case .message(let m) = self { return m } else { return nil } }
}

/// Serialises writes to stdout so concurrent responses never interleave.
final class OutputWriter {
    private let queue = DispatchQueue(label: "photos-bridge.output")

    func write(_ object: [String: Any]) {
        queue.async {
            guard var data = try? JSONSerialization.data(withJSONObject: object, options: [.withoutEscapingSlashes]) else { return }
            data.append(0x0A)
            FileHandle.standardOutput.write(data)
        }
    }

    func event(_ name: String) { write(["event": name]) }
}

extension Dictionary where Key == String, Value == Any {
    func string(_ key: String) throws -> String {
        guard let v = self[key] as? String, !v.isEmpty else { throw BridgeError.message("Missing parameter '\(key)'") }
        return v
    }

    func rect(_ key: String) throws -> CGRect {
        guard let r = self[key] as? [String: Any] else { throw BridgeError.message("Missing parameter '\(key)'") }
        return CGRect(x: r.double("x"), y: r.double("y"), width: r.double("width"), height: r.double("height"))
    }

    func double(_ key: String) -> Double { (self[key] as? NSNumber)?.doubleValue ?? 0 }
}

/// Normalised rect (top-left origin) as JSON.
func rectJSON(_ r: CGRect) -> [String: Double] {
    ["x": r.minX, "y": r.minY, "width": r.width, "height": r.height]
}

let isoFormatter: ISO8601DateFormatter = {
    let f = ISO8601DateFormatter()
    f.formatOptions = [.withInternetDateTime]
    return f
}()

enum ImageFiles {
    /// Decodes a file applying EXIF orientation, optionally downsampling.
    static func load(_ path: String, maxPixelSize: Int? = nil) throws -> CGImage {
        guard let source = CGImageSourceCreateWithURL(URL(fileURLWithPath: path) as CFURL, nil) else {
            throw BridgeError.message("Cannot open image at \(path)")
        }
        var options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceCreateThumbnailFromImageAlways: true,
        ]
        if let maxPixelSize {
            options[kCGImageSourceThumbnailMaxPixelSize] = maxPixelSize
        } else if let props = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
                  let w = props[kCGImagePropertyPixelWidth] as? Int, let h = props[kCGImagePropertyPixelHeight] as? Int {
            options[kCGImageSourceThumbnailMaxPixelSize] = max(w, h)
        }
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else {
            throw BridgeError.message("Cannot decode image at \(path)")
        }
        return image
    }

    static func load(data: Data, maxPixelSize: Int) throws -> CGImage {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailWithTransform: true,
                  kCGImageSourceCreateThumbnailFromImageAlways: true,
                  kCGImageSourceThumbnailMaxPixelSize: maxPixelSize,
              ] as CFDictionary)
        else { throw BridgeError.message("Cannot decode image data") }
        return image
    }

    static func writeJPEG(_ image: CGImage, to path: String, quality: Double) throws {
        let url = URL(fileURLWithPath: path)
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        guard let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.jpeg.identifier as CFString, 1, nil) else {
            throw BridgeError.message("Cannot write \(path)")
        }
        CGImageDestinationAddImage(dest, image, [kCGImageDestinationLossyCompressionQuality: quality] as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { throw BridgeError.message("Cannot write \(path)") }
    }

    static func pixelRect(_ r: CGRect, _ image: CGImage) -> CGRect {
        CGRect(x: r.minX * Double(image.width), y: r.minY * Double(image.height),
               width: r.width * Double(image.width), height: r.height * Double(image.height)).integral
    }

    /// Crops a normalised rect (top-left origin) out of an image file and saves it as JPEG.
    static func crop(source: String, destination: String, rect: CGRect, maxPixelSize: Int) throws -> [String: Any] {
        let image = try load(source)
        guard let cropped = image.cropping(to: pixelRect(rect, image)) else { throw BridgeError.message("Empty crop") }
        let scale = min(1, Double(maxPixelSize) / Double(max(cropped.width, cropped.height)))
        var final = cropped
        if scale < 1, let ctx = CGContext(data: nil, width: Int(Double(cropped.width) * scale), height: Int(Double(cropped.height) * scale),
                                          bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) {
            ctx.interpolationQuality = .high
            ctx.draw(cropped, in: CGRect(x: 0, y: 0, width: ctx.width, height: ctx.height))
            final = ctx.makeImage() ?? cropped
        }
        try writeJPEG(final, to: destination, quality: 0.88)
        return ["path": destination, "width": final.width, "height": final.height]
    }
}
