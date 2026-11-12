// Benchmark runner for Apple's on-device SpeechAnalyzer / SpeechTranscriber (macOS 26+).
//
// Usage: apple-runner --wav clip.wav --out result.json [--realtime] [--locale en-US]
//
// Without --realtime the clip is fed as fast as the analyzer takes it (accuracy and speed).
// With --realtime 100 ms is fed every 100 ms and every result is stamped with the wall clock,
// which gives true on-screen lag (Apple's engine is asynchronous, so the simulated clock the
// other runners use does not apply).
//
// Volatile results are partials; all volatile results since the last final share one id,
// which the next final closes.

import AVFoundation
import Foundation
import Speech

struct Args {
    var wav = ""
    var out = ""
    var realtime = false
    var locale = "en-US"

    init() {
        var it = CommandLine.arguments.dropFirst().makeIterator()
        while let a = it.next() {
            switch a {
            case "--wav": wav = it.next() ?? ""
            case "--out": out = it.next() ?? ""
            case "--realtime": realtime = true
            case "--locale": locale = it.next() ?? locale
            default: break  // --cache, --threads, --accel are accepted and ignored
            }
        }
    }
}

final class Recorder: @unchecked Sendable {
    private let lock = NSLock()
    private var events: [[String: Any]] = []
    private var nextId = 1
    private var openId: Int? = nil
    let started = Date()

    func add(text: String, isFinal: Bool, start: Double?, end: Double?) {
        lock.lock()
        defer { lock.unlock() }
        let id: Int
        if let open = openId {
            id = open
        } else {
            id = nextId
            nextId += 1
        }
        openId = isFinal ? nil : id
        var e: [String: Any] = [
            "id": String(id), "kind": isFinal ? "final" : "partial", "text": text,
            "at": Date().timeIntervalSince(started),
        ]
        if let start { e["start"] = start }
        if let end { e["end"] = end }
        events.append(e)
    }

    func all() -> [[String: Any]] {
        lock.lock()
        defer { lock.unlock() }
        return events
    }
}

@main
struct AppleRunner {
    static func main() async throws {
        let args = Args()
        let loadStart = Date()

        // Module and on-device model (downloaded by the system on first use).
        let transcriber = SpeechTranscriber(
            locale: Locale(identifier: args.locale),
            transcriptionOptions: [],
            reportingOptions: [.volatileResults],
            attributeOptions: [.audioTimeRange])
        if let request = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
            FileHandle.standardError.write("installing speech assets…\n".data(using: .utf8)!)
            try await request.downloadAndInstall()
        }
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcriber]) else {
            fatalError("no audio format")
        }
        let loadSecs = Date().timeIntervalSince(loadStart)

        // Read the clip and convert it to the analyzer's format.
        let file = try AVAudioFile(forReading: URL(fileURLWithPath: args.wav))
        let total = AVAudioFrameCount(file.length)
        guard let source = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: total) else { fatalError("buffer") }
        try file.read(into: source)
        let audioSecs = Double(total) / file.processingFormat.sampleRate
        guard let converter = AVAudioConverter(from: file.processingFormat, to: format) else { fatalError("converter") }
        let ratio = format.sampleRate / file.processingFormat.sampleRate

        func convert(_ start: AVAudioFramePosition, _ count: AVAudioFrameCount) -> AVAudioPCMBuffer {
            let slice = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: count)!
            slice.frameLength = count
            for ch in 0..<Int(file.processingFormat.channelCount) {
                memcpy(slice.floatChannelData![ch], source.floatChannelData![ch].advanced(by: Int(start)), Int(count) * 4)
            }
            let out = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(Double(count) * ratio) + 32)!
            var fed = false
            var error: NSError?
            converter.convert(to: out, error: &error) { _, status in
                if fed {
                    status.pointee = .noDataNow
                    return nil
                }
                fed = true
                status.pointee = .haveData
                return slice
            }
            return out
        }

        // Collect results as they arrive.
        let recorder = Recorder()
        let collector = Task {
            for try await result in transcriber.results {
                let text = String(result.text.characters)
                let start = result.range.start.seconds
                let end = result.range.end.seconds
                recorder.add(text: text, isFinal: result.isFinal, start: start.isFinite ? start : nil, end: end.isFinite ? end : nil)
            }
        }

        // Feed 100 ms chunks.
        let (stream, input) = AsyncStream<AnalyzerInput>.makeStream()
        try await analyzer.start(inputSequence: stream)
        let chunk = AVAudioFrameCount(file.processingFormat.sampleRate / 10)
        let feedStart = Date()
        var pos: AVAudioFramePosition = 0
        while pos < AVAudioFramePosition(total) {
            let count = min(chunk, AVAudioFrameCount(AVAudioFramePosition(total) - pos))
            if args.realtime {
                let due = Double(pos + AVAudioFramePosition(count)) / file.processingFormat.sampleRate
                let wait = due - Date().timeIntervalSince(feedStart)
                if wait > 0 { try await Task.sleep(nanoseconds: UInt64(wait * 1e9)) }
            }
            input.yield(AnalyzerInput(buffer: convert(pos, count)))
            pos += AVAudioFramePosition(count)
        }
        input.finish()
        try await analyzer.finalizeAndFinishThroughEndOfInput()
        _ = try? await collector.value
        let compute = Date().timeIntervalSince(feedStart)

        // In fast mode "at" is meaningless for lag; report it as an after-the-fact pass.
        var events = recorder.all()
        if !args.realtime {
            events = events.map { var e = $0; e["at"] = audioSecs + compute; return e }
        }
        let result: [String: Any] = [
            "mode": args.realtime ? "stream" : "offline",
            "audio_secs": audioSecs,
            "load_secs": loadSecs,
            // Real time: compute is not separable from waiting, so report it as unknown-ish.
            "compute_secs": args.realtime ? audioSecs : compute,
            "events": events,
        ]
        let data = try JSONSerialization.data(withJSONObject: result, options: [.prettyPrinted])
        try data.write(to: URL(fileURLWithPath: args.out))
    }
}
