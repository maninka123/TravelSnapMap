// photos-bridge: a tiny macOS helper exposing PhotoKit, Vision and MapKit to the Tauri backend.
//
// Protocol: newline-delimited JSON over stdin/stdout.
//   request  {"id": 1, "method": "vision.ocr", "params": {...}}
//   response {"id": 1, "result": ...}   or   {"id": 1, "error": "message"}
//   event    {"event": "photosLibraryChanged"}
//
// No business logic lives here: no database, no AI, no scoring. Only Apple frameworks.

import Foundation

let output = OutputWriter()

func handle(_ method: String, _ params: [String: Any]) async throws -> Any {
    switch method {
    case "ping":
        return ["ok": true, "version": 1]
    case "photos.authorizationStatus":
        return PhotosService.authorizationStatus()
    case "photos.requestPermission":
        return await PhotosService.requestPermission()
    case "photos.listScreenshots":
        return try PhotosService.listScreenshots(fromYear: params["fromYear"] as? Int, toYear: params["toYear"] as? Int)
    case "photos.metadata":
        return try PhotosService.metadata(id: try params.string("id"))
    case "photos.exportImage":
        return try await PhotosService.exportImage(
            id: try params.string("id"), path: try params.string("path"),
            maxPixelSize: params["maxPixelSize"] as? Int ?? 2048, quality: params["quality"] as? Double ?? 0.85)
    case "photos.observe":
        PhotosService.startObserving { output.event("photosLibraryChanged") }
        return ["observing": true]
    case "image.crop":
        return try ImageFiles.crop(
            source: try params.string("path"), destination: try params.string("outPath"),
            rect: try params.rect("rect"), maxPixelSize: params["maxPixelSize"] as? Int ?? 1600)
    case "vision.ocr":
        return try await VisionService.recognizeText(path: try params.string("path"))
    case "vision.saliency":
        return try await VisionService.saliency(path: try params.string("path"))
    case "vision.featurePrint":
        return try await VisionService.featurePrint(path: try params.string("path"), crop: params["rect"] as? [String: Any])
    case "media.extractAudio":
        return try await MediaService.extractAudio(path: try params.string("path"), outPath: try params.string("outPath"))
    case "speech.authorization":
        return await MediaService.speechAuthorization()
    case "speech.transcribe":
        return try await MediaService.transcribe(path: try params.string("path"), locale: params["locale"] as? String ?? "en-US")
    case "video.keyframes":
        return try await MediaService.keyframes(
            path: try params.string("path"), outDir: try params.string("outDir"),
            maxFrames: params["maxFrames"] as? Int ?? 8, interval: params["interval"] as? Double ?? 1.0)
    case "photos.listVideos":
        return try MediaService.listVideos(limit: params["limit"] as? Int ?? 100)
    case "photos.exportVideo":
        return try await MediaService.exportVideo(id: try params.string("id"), outPath: try params.string("outPath"))
    case "maps.search":
        return try await MapsService.search(query: try params.string("query"), limit: params["limit"] as? Int ?? 8)
    default:
        throw BridgeError.message("Unknown method \(method)")
    }
}

// Read requests on a background thread; each runs concurrently in its own task.
Thread.detachNewThread {
    while let line = readLine(strippingNewline: true) {
        guard let data = line.data(using: .utf8),
              let request = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let id = request["id"], let method = request["method"] as? String
        else {
            output.write(["error": "Malformed request"])
            continue
        }
        let params = request["params"] as? [String: Any] ?? [:]
        Task {
            do {
                let result = try await handle(method, params)
                output.write(["id": id, "result": result])
            } catch {
                output.write(["id": id, "error": (error as? LocalizedError)?.errorDescription ?? "\(error)"])
            }
        }
    }
    exit(0) // stdin closed: the app quit.
}

// MapKit and PhotoKit deliver callbacks on the main run loop.
RunLoop.main.run()
