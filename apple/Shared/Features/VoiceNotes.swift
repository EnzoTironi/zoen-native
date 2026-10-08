import SwiftUI
import AVFoundation
import Speech
import RodaCore

// MARK: - Voice notes
//
// Hold the mic to record (AAC, mono), slide left to cancel, slide up to lock, release to
// send. Transcribed on device (Speech, `requiresOnDeviceRecognition`), with word timings
// kept for the editor. A sent note travels in the chat as a compact marker
// (`⟦voice:<id>:<ms>:<levels>⟧` + the transcript); the audio file stays in the app's
// container under VoiceNotes/.

/// One recorded clip, ready to send.
struct VoiceClip: Equatable {
    let id: String
    let url: URL
    let ms: Int
    /// 40 bars, 0…1.
    let levels: [CGFloat]
    /// 160 bars for the editor (empty: use `levels`).
    var detail: [CGFloat] = []
    var wave: [CGFloat] { detail.isEmpty ? levels : detail }
}

/// A transcript with per-word timings (seconds), for "Show transcript" and the editor.
struct VoiceTranscript: Equatable, Sendable {
    struct Word: Equatable, Sendable { let text: String; let start: Double; let duration: Double }
    var text: String
    var words: [Word]
}

/// The marker a voice note is sent as.
struct VoiceNoteRef: Equatable {
    let id: String
    let ms: Int
    let levels: [CGFloat]
    let transcript: String

    static let open = "⟦voice:", close = "⟧"

    var marker: String {
        let hex = levels.map { String(format: "%x", min(15, max(0, Int(($0 * 15).rounded())))) }.joined()
        return "\(Self.open)\(id):\(ms):\(hex)\(Self.close)" + (transcript.isEmpty ? "" : "\n\(transcript)")
    }

    static func parse(_ text: String) -> VoiceNoteRef? {
        guard text.hasPrefix(open), let end = text.range(of: close) else { return nil }
        let body = text[text.index(text.startIndex, offsetBy: open.count)..<end.lowerBound].split(separator: ":")
        guard body.count == 3, let ms = Int(body[1]) else { return nil }
        let levels = body[2].map { CGFloat(Int(String($0), radix: 16) ?? 0) / 15 }
        let rest = text[end.upperBound...].trimmingCharacters(in: .whitespacesAndNewlines)
        return VoiceNoteRef(id: String(body[0]), ms: ms, levels: levels, transcript: rest)
    }

    /// Chat-list preview: "Voice message · 0:07".
    static func preview(_ text: String) -> String? {
        guard text.hasPrefix(open) else { return nil }
        guard let r = parse(text) else { return String(localized: "Voice message") }
        return String(localized: "Voice message · \(VoiceFormat.time(Double(r.ms) / 1000))")
    }
}

enum VoiceFormat {
    static func time(_ s: Double) -> String {
        let t = max(0, Int(s.rounded(.down)))
        return String(format: "%d:%02d", t / 60, t % 60)
    }
}

enum VoiceStore {
    static var dir: URL {
        let d = URL.applicationSupportDirectory.appending(path: "VoiceNotes", directoryHint: .isDirectory)
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        return d
    }
    static func url(_ id: String) -> URL { dir.appending(path: "\(id).m4a") }
    static func delete(_ url: URL) { try? FileManager.default.removeItem(at: url) }

    /// A short, soft synthetic clip for the demo seed (screenshots without a microphone).
    static func makeDemoClip(seconds: Double = 7) -> VoiceClip? {
        let id = "demo-" + UUID().uuidString.prefix(8)
        let u = url(String(id))
        let rate = 22_050.0
        guard let fmt = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1),
              let buf = AVAudioPCMBuffer(pcmFormat: fmt, frameCapacity: AVAudioFrameCount(rate * seconds)) else { return nil }
        buf.frameLength = buf.frameCapacity
        var levels: [CGFloat] = []
        if let ch = buf.floatChannelData?[0] {
            for i in 0..<Int(buf.frameLength) {
                let t = Double(i) / rate
                let quiet = (2.8...3.9).contains(t) ? 0.03 : 1.0  // a pause, for the editor demo
                let syll = quiet * max(0, sin(t * 2 * .pi * 2.3)) * (0.6 + 0.4 * sin(t * 1.7))
                ch[i] = Float(0.18 * syll * sin(t * 2 * .pi * 180) * (sin(t * 2 * .pi * 3) * 0.3 + 0.7))
            }
            for b in 0..<160 {
                let t = (Double(b) + 0.5) / 160 * seconds
                let quiet = (2.8...3.9).contains(t) ? 0.0 : 1.0
                levels.append(CGFloat(0.08 + quiet * 0.85 * max(0, sin(t * 2 * .pi * 2.3)) * (0.6 + 0.4 * sin(t * 1.7))))
            }
        }
        let settings: [String: Any] = [AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: rate, AVNumberOfChannelsKey: 1]
        guard let file = try? AVAudioFile(forWriting: u, settings: settings) else { return nil }
        do { try file.write(from: buf) } catch { return nil }
        return VoiceClip(id: String(id), url: u, ms: Int(seconds * 1000), levels: VoiceRecorder.bars(levels, count: 40), detail: levels)
    }
}

// MARK: Recorder

@MainActor @Observable
final class VoiceRecorder {
    enum Phase: Equatable { case idle, recording, locked }
    var phase: Phase = .idle
    /// Recent input levels, 0…1, newest last (the live waveform).
    var levels: [CGFloat] = []
    var elapsed: TimeInterval = 0
    /// `-RodaVoiceDemo recording|locked`: fake input, no microphone (screenshots).
    var demo = false

    private var recorder: AVAudioRecorder?
    private var tick: Task<Void, Never>?
    private var id = ""
    private var url: URL?
    private var all: [CGFloat] = []

    func start() async -> Bool {
        guard phase == .idle else { return false }
        if demo { begin(); return true }
        #if os(iOS)
        guard await AVAudioApplication.requestRecordPermission() else { return false }
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker])
        try? session.setActive(true)
        #endif
        id = UUID().uuidString
        let u = VoiceStore.url(id)
        let settings: [String: Any] = [AVFormatIDKey: kAudioFormatMPEG4AAC, AVSampleRateKey: 44_100, AVNumberOfChannelsKey: 1,
                                       AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue]
        guard let r = try? AVAudioRecorder(url: u, settings: settings) else { return false }
        r.isMeteringEnabled = true
        guard r.record() else { return false }
        recorder = r
        url = u
        begin()
        return true
    }

    private func begin() {
        phase = .recording
        levels = []
        all = []
        elapsed = 0
        let started = Date()
        tick?.cancel()
        tick = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(50))
                guard let self else { return }
                let level: CGFloat
                if let r = self.recorder {
                    r.updateMeters()
                    let db = r.averagePower(forChannel: 0)
                    level = CGFloat(max(0, min(1, (db + 50) / 50)))
                    self.elapsed = r.currentTime
                } else {
                    let t = Date().timeIntervalSince(started)
                    level = CGFloat(0.2 + 0.75 * max(0, sin(t * 2 * .pi * 2.1)) * (0.6 + 0.4 * sin(t * 1.3)))
                    self.elapsed = t + (self.demo ? 4 : 0)
                }
                self.all.append(level)
                self.levels.append(level)
                if self.levels.count > 64 { self.levels.removeFirst(self.levels.count - 64) }
            }
        }
    }

    func lock() {
        guard phase == .recording else { return }
        phase = .locked
        Haptics.recordLock()
    }

    /// Stops and hands back the clip (nil if it was too short to keep).
    func finish() -> VoiceClip? {
        tick?.cancel()
        let dur = recorder?.currentTime ?? elapsed
        recorder?.stop()
        recorder = nil
        defer { phase = .idle; levels = []; demo = false }
        // Screenshot / UITest demo: no real file — synthesize one that matches elapsed.
        if demo {
            self.url = nil
            return VoiceStore.makeDemoClip(seconds: max(0.8, dur))
        }
        guard let url, dur >= 0.6 else { if let url { VoiceStore.delete(url) }; self.url = nil; return nil }
        self.url = nil
        return VoiceClip(id: id, url: url, ms: Int(dur * 1000), levels: Self.bars(all, count: 40), detail: Self.bars(all, count: 160))
    }

    func cancel() {
        tick?.cancel()
        recorder?.stop()
        recorder = nil
        if let url { VoiceStore.delete(url) }
        url = nil
        phase = .idle
        levels = []
    }

    nonisolated static func bars(_ x: [CGFloat], count: Int) -> [CGFloat] {
        guard !x.isEmpty else { return Array(repeating: 0.15, count: count) }
        return (0..<count).map { i in
            let a = i * x.count / count, b = max(a + 1, (i + 1) * x.count / count)
            let s = x[a..<min(b, x.count)]
            return max(0.1, s.max() ?? 0.1)
        }
    }
}

// MARK: Transcription (on device)

enum VoiceTranscriber {
    /// On-device only: if this device can't transcribe locally, there's no transcript (the
    /// audio is never sent anywhere to be transcribed).
    static func transcribe(_ url: URL, timeout: Double = 6) async -> VoiceTranscript? {
        let status = await withCheckedContinuation { (c: CheckedContinuation<SFSpeechRecognizerAuthorizationStatus, Never>) in
            SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) }
        }
        guard status == .authorized,
              let rec = SFSpeechRecognizer(locale: Locale(identifier: AppLocale.isPortuguese ? "pt-BR" : "en-US")),
              rec.isAvailable, rec.supportsOnDeviceRecognition else { return nil }
        let req = SFSpeechURLRecognitionRequest(url: url)
        req.requiresOnDeviceRecognition = true
        req.shouldReportPartialResults = false
        req.addsPunctuation = true
        let once = Once()
        return await withCheckedContinuation { (c: CheckedContinuation<VoiceTranscript?, Never>) in
            once.task = rec.recognitionTask(with: req) { result, error in
                if let result, result.isFinal {
                    let best = result.bestTranscription
                    let words = best.segments.map { VoiceTranscript.Word(text: $0.substring, start: $0.timestamp, duration: $0.duration) }
                    let t = VoiceTranscript(text: best.formattedString, words: words)
                    if once.claim() { c.resume(returning: t) }
                } else if error != nil {
                    if once.claim() { c.resume(returning: nil) }
                }
            }
            DispatchQueue.main.asyncAfter(deadline: .now() + timeout) {
                if once.claim() { once.cancelTask(); c.resume(returning: nil) }
            }
        }
    }

    private final class Once: @unchecked Sendable {
        private let lock = NSLock()
        private var done = false
        var task: SFSpeechRecognitionTask?
        func cancelTask() { lock.lock(); let t = task; lock.unlock(); t?.cancel() }
        func claim() -> Bool { lock.lock(); defer { lock.unlock() }; if done { return false }; done = true; return true }
    }
}

// MARK: Player

@MainActor @Observable
final class VoicePlayer {
    static let shared = VoicePlayer()
    var currentId: String?
    var isPlaying = false
    var progress: Double = 0
    var rate: Float = 1
    private var player: AVAudioPlayer?
    private var tick: Task<Void, Never>?

    func toggle(_ ref: VoiceNoteRef) {
        if currentId == ref.id, let p = player {
            if p.isPlaying { p.pause(); isPlaying = false } else { p.play(); isPlaying = true; loop() }
            return
        }
        stop()
        #if os(iOS)
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        try? AVAudioSession.sharedInstance().setActive(true)
        #endif
        guard let p = try? AVAudioPlayer(contentsOf: VoiceStore.url(ref.id)) else { currentId = ref.id; return }
        p.enableRate = true
        p.rate = rate
        p.prepareToPlay()
        p.play()
        player = p
        currentId = ref.id
        isPlaying = true
        progress = 0
        loop()
    }

    func seek(_ ref: VoiceNoteRef, to f: Double) {
        if currentId != ref.id { toggle(ref); player?.pause(); isPlaying = false }
        guard let p = player else { return }
        p.currentTime = max(0, min(1, f)) * p.duration
        progress = f
    }

    func cycleRate() {
        rate = rate == 1 ? 1.5 : rate == 1.5 ? 2 : 1
        player?.rate = rate
    }

    func stop() {
        tick?.cancel()
        player?.stop()
        player = nil
        isPlaying = false
        progress = 0
        currentId = nil
    }

    private func loop() {
        tick?.cancel()
        tick = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(40))
                guard let self, let p = self.player else { return }
                self.progress = p.duration > 0 ? p.currentTime / p.duration : 0
                if !p.isPlaying {
                    self.isPlaying = false
                    if self.progress < 0.02 || self.progress > 0.98 { self.progress = 0 }
                    return
                }
            }
        }
    }
}

// MARK: Views

/// Bars for a waveform (live or recorded). `progress` tints the played part.
struct WaveformBars: View {
    var levels: [CGFloat]
    var progress: Double = 1
    var played: Color
    var unplayed: Color
    var barWidth: CGFloat = 2.6
    var spacing: CGFloat = 2

    var body: some View {
        Canvas { ctx, size in
            let n = levels.count
            guard n > 0 else { return }
            let step = barWidth + spacing
            let count = max(1, min(n, Int((size.width + spacing) / step)))
            let slice = Array(levels.suffix(count))
            let x0 = size.width - CGFloat(slice.count) * step + spacing
            for (i, l) in slice.enumerated() {
                let h = max(3, l * size.height)
                let r = CGRect(x: x0 + CGFloat(i) * step, y: (size.height - h) / 2, width: barWidth, height: h)
                let done = Double(i) / Double(max(1, slice.count - 1)) <= progress
                ctx.fill(Path(roundedRect: r, cornerRadius: barWidth / 2), with: .color(done ? played : unplayed))
            }
        }
        .accessibilityHidden(true)
    }
}

/// A sent voice note: play/pause, a scrubbable waveform, time, speed and the transcript.
struct VoiceBubble: View {
    let ref: VoiceNoteRef
    let mine: Bool
    let grouped: Bool
    @State private var showTranscript = false
    @State private var landed = false
    @Environment(\.chatBackdrop) private var backdrop
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private var player: VoicePlayer { VoicePlayer.shared }

    var body: some View {
        let current = player.currentId == ref.id
        let fg = mine ? Palette.myBubbleText : Palette.textPrimary
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 10) {
                Button { player.toggle(ref) } label: {
                    ZoenIcon(current && player.isPlaying ? .pause : .play, size: 18)
                        .foregroundStyle(mine ? Palette.myBubble : .white)
                        .frame(width: 36, height: 36)
                        .background(mine ? Palette.myBubbleText : Palette.action, in: .circle)
                }
                .buttonStyle(IconPressStyle())
                .accessibilityLabel(current && player.isPlaying ? Text("Pause") : Text("Play voice message"))

                GeometryReader { g in
                    WaveformBars(levels: ref.levels, progress: current ? player.progress : 0,
                                 played: fg, unplayed: fg.opacity(0.32))
                        .contentShape(.rect)
                        .gesture(DragGesture(minimumDistance: 0).onChanged { v in
                            player.seek(ref, to: Double(v.location.x / max(1, g.size.width)))
                        })
                }
                .frame(width: 128, height: 26)
                .accessibilityElement()
                .accessibilityLabel(Text("Voice message, \(VoiceFormat.time(Double(ref.ms) / 1000))"))
                .accessibilityAdjustableAction { dir in
                    let p = current ? player.progress : 0
                    player.seek(ref, to: dir == .increment ? p + 0.1 : p - 0.1)
                }

                VStack(alignment: .trailing, spacing: 2) {
                    Text(VoiceFormat.time(current ? player.progress * Double(ref.ms) / 1000 : Double(ref.ms) / 1000))
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(fg.opacity(0.8))
                    Button { player.cycleRate() } label: {
                        Text(player.rate == 1 ? "1×" : player.rate == 1.5 ? "1.5×" : "2×")
                            .font(.caption2.weight(.bold).monospacedDigit())
                            .foregroundStyle(fg)
                            .padding(.horizontal, 6).padding(.vertical, 2)
                            .background(fg.opacity(0.12), in: .capsule)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(Text("Playback speed"))
                }
            }
            if !ref.transcript.isEmpty {
                Button { withAnimation(.snappy) { showTranscript.toggle() } } label: {
                    Text(showTranscript ? "Hide transcript" : "Show transcript")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(mine ? fg.opacity(0.75) : Palette.action)
                }
                .buttonStyle(.plain)
                if showTranscript {
                    Text(ref.transcript)
                        .font(.subheadline)
                        .foregroundStyle(fg)
                        .textSelection(.enabled)
                        .transition(.opacity.combined(with: .move(edge: .top)))
                }
            }
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background {
            if mine {
                UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: 20, bottomTrailingRadius: grouped ? 20 : 4, topTrailingRadius: 20, style: .continuous)
                    .fill(Palette.myBubble)
            } else {
                UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: grouped ? 20 : 4, bottomTrailingRadius: 20, topTrailingRadius: 20, style: .continuous)
                    .fill(backdrop ? Palette.otherBubbleOnBackdrop : Palette.otherBubble)
            }
        }
        .shadow(color: .black.opacity(backdrop ? 0.1 : 0), radius: 1.5, y: 0.5)
        .scaleEffect(landed ? 1 : 0.86)
        .opacity(landed ? 1 : 0.35)
        .offset(y: landed ? 0 : 18)
        .accessibilityIdentifier("voice-message")
        .task {
            if UserDefaults.standard.bool(forKey: "RodaVoiceShowTranscript") { showTranscript = true }
            // +→Zoen send-flight land: bubble drops in where the mic morph handed off.
            if model.pendingVoiceFlight == ref.id {
                model.pendingVoiceFlight = nil
                if reduceMotion {
                    landed = true
                } else {
                    withAnimation(.spring(duration: 0.48, bounce: 0.28)) { landed = true }
                }
            } else {
                landed = true
            }
        }
    }
}

/// The composer row while recording: red dot, timer, live waveform and "slide to cancel"
/// (holding), or delete · timer · waveform once locked.
struct RecordingRow: View {
    let recorder: VoiceRecorder
    var dragX: CGFloat
    var onDelete: () -> Void
    var onStop: () -> Void = {}
    @State private var pulse = false

    var body: some View {
        HStack(spacing: 10) {
            if recorder.phase == .locked {
                Button(action: onDelete) {
                    ZoenIcon(.trash, size: 20).foregroundStyle(Palette.danger).frame(width: 36, height: 36)
                }
                .buttonStyle(IconPressStyle())
                .accessibilityLabel(Text("Delete recording"))
            }
            Circle().fill(Palette.danger).frame(width: 9, height: 9)
                .opacity(pulse ? 0.25 : 1)
                .animation(.easeInOut(duration: 0.6).repeatForever(autoreverses: true), value: pulse)
                .onAppear { pulse = true }
            Text(VoiceFormat.time(recorder.elapsed))
                .font(.body.monospacedDigit())
                .foregroundStyle(Palette.textPrimary)
                .fixedSize()
            WaveformBars(levels: recorder.levels, played: Palette.textPrimary.opacity(0.75), unplayed: Palette.textPrimary.opacity(0.75), barWidth: 2.2, spacing: 1.8)
                .frame(height: 24)
                .frame(maxWidth: recorder.phase == .locked ? .infinity : 96)
            if recorder.phase == .locked {
                Button(action: onStop) {
                    RoundedRectangle(cornerRadius: 3, style: .continuous).fill(Palette.danger)
                        .frame(width: 13, height: 13)
                        .frame(width: 36, height: 36)
                        .overlay(Circle().stroke(Palette.danger.opacity(0.5), lineWidth: 1.5).frame(width: 28, height: 28))
                }
                .buttonStyle(IconPressStyle())
                .accessibilityLabel(Text("Stop and review"))
            }
            if recorder.phase == .recording {
                Spacer(minLength: 0)
                HStack(spacing: 2) {
                    ZoenIcon(.back, size: 13)
                    Text("Slide to cancel").font(.subheadline).lineLimit(1).minimumScaleFactor(0.75)
                }
                .foregroundStyle(Palette.textSecondary)
                .layoutPriority(1)  // the waveform gives way first
                .offset(x: min(0, dragX) * 0.6)
                .opacity(Double(1 + min(0, dragX) / 110))
            }
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 44)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text(recorder.phase == .locked ? "Recording locked, \(VoiceFormat.time(recorder.elapsed))" : "Recording, \(VoiceFormat.time(recorder.elapsed))"))
    }
}

/// The lock hint that rises above the mic while you hold.
struct LockHint: View {
    var dragY: CGFloat
    var body: some View {
        VStack(spacing: 4) {
            ZoenIcon(.lock, size: 17)
            ZoenIcon(.chevron, size: 11).rotationEffect(.degrees(-90))
        }
        .foregroundStyle(Palette.textSecondary)
        .frame(width: 40, height: 76)
        .glassEffect(.regular, in: .capsule)
        .offset(y: max(-30, min(0, dragY)) * 0.5)
        .accessibilityHidden(true)
    }
}
