import AVFoundation
import Foundation
import Speech

/// Speech transcription. Primary engine: SpeechAnalyzer/SpeechTranscriber (macOS 26, fully on-device, no
/// permission prompt). Fallback: SFSpeechRecognizer for locales only it supports (may use Apple's servers).
enum SpeechService {
    static func bcp47(_ locale: Locale) -> String { locale.identifier(.bcp47) }

    /// Finds the modern-engine locale matching an identifier (exact, else same language).
    static func modernLocale(for identifier: String) async -> Locale? {
        let wanted = Locale(identifier: identifier)
        let supported = await SpeechTranscriber.supportedLocales
        if let exact = supported.first(where: { bcp47($0).lowercased() == bcp47(wanted).lowercased() }) { return exact }
        let lang = wanted.language.languageCode?.identifier
        return supported.first { $0.language.languageCode?.identifier == lang }
    }

    static func supportedLocales() async -> [[String: Any]] {
        var out: [String: [String: Any]] = [:]
        let english = Locale(identifier: "en_US")
        for locale in SFSpeechRecognizer.supportedLocales() {
            let id = locale.identifier.replacingOccurrences(of: "_", with: "-")
            out[id] = ["id": id, "name": english.localizedString(forIdentifier: locale.identifier) ?? id,
                       "onDevice": SFSpeechRecognizer(locale: locale)?.supportsOnDeviceRecognition ?? false, "engine": "classic"]
        }
        let installed = Set(await SpeechTranscriber.installedLocales.map(bcp47))
        for locale in await SpeechTranscriber.supportedLocales {
            let id = bcp47(locale)
            out[id] = ["id": id, "name": english.localizedString(forIdentifier: id) ?? id, "onDevice": true,
                       "engine": "SpeechTranscriber", "installed": installed.contains(id)]
        }
        return out.values.sorted { ($0["name"] as? String ?? "") < ($1["name"] as? String ?? "") }
    }

    static func transcribe(path: String, locale identifier: String) async throws -> [String: Any] {
        if let locale = await modernLocale(for: identifier) {
            return try await transcribeOnDevice(path: path, locale: locale)
        }
        return try await transcribeClassic(path: path, locale: identifier)
    }

    private static func transcribeOnDevice(path: String, locale: Locale) async throws -> [String: Any] {
        let transcriber = SpeechTranscriber(locale: locale, transcriptionOptions: [], reportingOptions: [],
                                            attributeOptions: [.audioTimeRange, .transcriptionConfidence])
        // First use of a language downloads Apple's on-device model for it.
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            try await request.downloadAndInstall()
        }
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        let file = try AVAudioFile(forReading: URL(fileURLWithPath: path))
        let collector = Task { () -> [[String: Any]] in
            var words: [[String: Any]] = []
            for try await result in transcriber.results where result.isFinal {
                for run in result.text.runs {
                    let text = String(result.text[run.range].characters).trimmingCharacters(in: .whitespaces)
                    guard !text.isEmpty, let range = run.audioTimeRange else { continue }
                    words.append(["text": text, "start": range.start.seconds, "duration": range.duration.seconds,
                                  "confidence": run.transcriptionConfidence ?? 0])
                }
            }
            return words
        }
        if let last = try await analyzer.analyzeSequence(from: file) {
            try await analyzer.finalizeAndFinish(through: last)
        } else {
            await analyzer.cancelAndFinishNow()
        }
        let words = try await collector.value
        return ["words": words, "onDevice": true, "engine": "SpeechTranscriber", "locale": bcp47(locale)]
    }

    private static func transcribeClassic(path: String, locale: String) async throws -> [String: Any] {
        guard SFSpeechRecognizer.supportedLocales().contains(where: { $0.identifier.replacingOccurrences(of: "_", with: "-") == locale }) else {
            throw BridgeError.message("Speech in \(locale) isn't supported by Apple on this Mac")
        }
        var result = try await MediaService.transcribe(path: path, locale: locale)
        result["engine"] = "classic"
        return result
    }
}
