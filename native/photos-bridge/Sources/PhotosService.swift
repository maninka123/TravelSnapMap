import Foundation
import Photos

/// PhotoKit access. Only assets Photos tags as screenshots are ever enumerated.
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
            throw BridgeError.message("Screenshot no longer exists in Photos")
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
