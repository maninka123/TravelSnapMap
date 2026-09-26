import AppKit
import CoreLocation
import Foundation
import Photos

/// PhotoKit access. Screenshots are enumerated for processing; your own photos only when you ask to attach
/// them to a visited place ("My visit"), and only their id, date and location are read.
enum PhotosService {
    static func describe(_ status: PHAuthorizationStatus) -> String {
        switch status {
        case .authorized: "authorized"
        case .limited: "limited"
        case .denied: "denied"
        case .restricted: "restricted"
        case .notDetermined: "notDetermined"
        @unknown default: "unknown"
        }
    }

    static func authorizationStatus() -> String {
        describe(PHPhotoLibrary.authorizationStatus(for: .readWrite))
    }

    static func requestPermission() async -> String {
        describe(await PHPhotoLibrary.requestAuthorization(for: .readWrite))
    }

    private static func requireAccess() throws {
        let status = PHPhotoLibrary.authorizationStatus(for: .readWrite)
        guard status == .authorized || status == .limited else {
            throw BridgeError.message("Photos access not granted (\(describe(status)))")
        }
    }

    static func listScreenshots(fromYear: Int?, toYear: Int?) throws -> [[String: Any]] {
        try requireAccess()
        var predicates = [NSPredicate(format: "(mediaSubtypes & %d) != 0", PHAssetMediaSubtype.photoScreenshot.rawValue)]
        let calendar = Calendar(identifier: .gregorian)
        if let fromYear, let start = calendar.date(from: DateComponents(year: fromYear, month: 1, day: 1)) {
            predicates.append(NSPredicate(format: "creationDate >= %@", start as NSDate))
        }
        if let toYear, let end = calendar.date(from: DateComponents(year: toYear + 1, month: 1, day: 1)) {
            predicates.append(NSPredicate(format: "creationDate < %@", end as NSDate))
        }
        let options = PHFetchOptions()
        options.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: predicates)
        options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]

        var out: [[String: Any]] = []
        PHAsset.fetchAssets(with: .image, options: options).enumerateObjects { asset, _, _ in
            out.append(summary(asset))
        }
        return out
    }

    static func metadata(id: String) throws -> [String: Any] {
        try requireAccess()
        let asset = try fetch(id)
        var info = summary(asset)
        info["modificationDate"] = asset.modificationDate.map(isoFormatter.string(from:)) ?? NSNull()
        info["isFavorite"] = asset.isFavorite
        if let location = asset.location {
            info["location"] = ["latitude": location.coordinate.latitude, "longitude": location.coordinate.longitude]
        }
        return info
    }

    /// Writes an orientation-corrected, downsized JPEG of the asset (downloading from iCloud if needed).
    static func exportImage(id: String, path: String, maxPixelSize: Int, quality: Double) async throws -> [String: Any] {
        try requireAccess()
        let asset = try fetch(id)
        let options = PHImageRequestOptions()
        options.isNetworkAccessAllowed = true
        options.deliveryMode = .highQualityFormat
        options.version = .current
        let data: Data = try await withCheckedThrowingContinuation { continuation in
            PHImageManager.default().requestImageDataAndOrientation(for: asset, options: options) { data, _, _, info in
                if let data {
                    continuation.resume(returning: data)
                } else {
                    let error = (info?[PHImageErrorKey] as? Error)?.localizedDescription ?? "Image unavailable (it may still be in iCloud)"
                    continuation.resume(throwing: BridgeError.message(error))
                }
            }
        }
        let image = try ImageFiles.load(data: data, maxPixelSize: maxPixelSize)
        try ImageFiles.writeJPEG(image, to: path, quality: quality)
        return ["path": path, "width": image.width, "height": image.height]
    }

    private static func fetch(_ id: String) throws -> PHAsset {
        guard let asset = PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject else {
            throw BridgeError.message("This item no longer exists in Photos")
        }
        return asset
    }

    private static func summary(_ asset: PHAsset) -> [String: Any] {
        [
            "id": asset.localIdentifier,
            "creationDate": asset.creationDate.map(isoFormatter.string(from:)) ?? NSNull(),
            "width": asset.pixelWidth,
            "height": asset.pixelHeight,
        ]
    }

    // MARK: Your own photos (after-trip memories)

    /// Your own photos (not screenshots) taken within `radiusKm` of a place, newest first.
    static func listNear(latitude: Double, longitude: Double, radiusKm: Double, limit: Int) throws -> [[String: Any]] {
        try requireAccess()
        let center = CLLocation(latitude: latitude, longitude: longitude)
        let maxDistance = radiusKm * 1000
        var out: [[String: Any]] = []
        ownPhotos(from: nil, to: nil).enumerateObjects { asset, _, stop in
            guard let location = asset.location, !asset.mediaSubtypes.contains(.photoScreenshot) else { return }
            let distance = location.distance(from: center)
            guard distance <= maxDistance else { return }
            var info = memorySummary(asset)
            info["distanceM"] = distance
            out.append(info)
            if out.count >= limit { stop.pointee = true }
        }
        return out
    }

    /// Your own photos (not screenshots) taken between two ISO dates, newest first.
    static func listBetween(from: String, to: String, limit: Int) throws -> [[String: Any]] {
        try requireAccess()
        guard let start = parseDay(from), let endDay = parseDay(to),
              let end = Calendar.current.date(byAdding: .day, value: 1, to: endDay) else {
            throw BridgeError.message("Invalid date range")
        }
        var out: [[String: Any]] = []
        ownPhotos(from: start, to: end).enumerateObjects { asset, _, stop in
            guard !asset.mediaSubtypes.contains(.photoScreenshot) else { return }
            out.append(memorySummary(asset))
            if out.count >= limit { stop.pointee = true }
        }
        return out
    }

    /// Small JPEG preview for the photo picker (fast: uses Photos' own derivatives, not the original).
    static func thumbnail(id: String, path: String, size: Int) async throws -> [String: Any] {
        try requireAccess()
        let asset = try fetch(id)
        let options = PHImageRequestOptions()
        options.isNetworkAccessAllowed = true
        options.deliveryMode = .highQualityFormat
        options.resizeMode = .fast
        let target = CGSize(width: size, height: size)
        let image: CGImage = try await withCheckedThrowingContinuation { continuation in
            PHImageManager.default().requestImage(for: asset, targetSize: target, contentMode: .aspectFill, options: options) { image, info in
                if let cg = image?.cgImage(forProposedRect: nil, context: nil, hints: nil) {
                    continuation.resume(returning: cg)
                } else {
                    let error = (info?[PHImageErrorKey] as? Error)?.localizedDescription ?? "Preview unavailable"
                    continuation.resume(throwing: BridgeError.message(error))
                }
            }
        }
        try ImageFiles.writeJPEG(image, to: path, quality: 0.8)
        return ["path": path]
    }

    private static func ownPhotos(from: Date?, to: Date?) -> PHFetchResult<PHAsset> {
        let options = PHFetchOptions()
        var predicates: [NSPredicate] = []
        if let from { predicates.append(NSPredicate(format: "creationDate >= %@", from as NSDate)) }
        if let to { predicates.append(NSPredicate(format: "creationDate < %@", to as NSDate)) }
        if !predicates.isEmpty { options.predicate = NSCompoundPredicate(andPredicateWithSubpredicates: predicates) }
        options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
        return PHAsset.fetchAssets(with: .image, options: options)
    }

    private static func memorySummary(_ asset: PHAsset) -> [String: Any] {
        var info = summary(asset)
        if let location = asset.location {
            info["latitude"] = location.coordinate.latitude
            info["longitude"] = location.coordinate.longitude
        }
        return info
    }

    private static func parseDay(_ s: String) -> Date? {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return f.date(from: String(s.prefix(10)))
    }

    // MARK: Change observation

    private static var observer: LibraryObserver?

    static func startObserving(_ onChange: @escaping () -> Void) {
        guard observer == nil else { return }
        let o = LibraryObserver(onChange: onChange)
        PHPhotoLibrary.shared().register(o)
        observer = o
    }

    private final class LibraryObserver: NSObject, PHPhotoLibraryChangeObserver {
        let onChange: () -> Void
        private var pending: DispatchWorkItem?
        init(onChange: @escaping () -> Void) { self.onChange = onChange }

        func photoLibraryDidChange(_ changeInstance: PHChange) {
            // Debounce bursts (e.g. iCloud sync) into a single event.
            pending?.cancel()
            let work = DispatchWorkItem { [onChange] in onChange() }
            pending = work
            DispatchQueue.main.asyncAfter(deadline: .now() + 2, execute: work)
        }
    }
}
