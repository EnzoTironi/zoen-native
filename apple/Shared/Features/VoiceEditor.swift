import SwiftUI
import AVFoundation

// MARK: - Voice note review & editor
//
// After a locked recording is stopped: a review sheet (Delete · Edit · Send). Edit opens
// the editor: tap words to strike them out, drag across words to strike a run, trim the
// ends, select a stretch of waveform and cut it, take the filler / long-pause suggestions,
// undo and redo, and preview the result (it skips everything removed).
//
// Sending renders an edit decision list (the kept ranges) into a NEW file with
// AVMutableComposition (15 ms fades at every join) and exports AAC. Only that render and
// the words you kept leave the phone; the original recording is deleted, so removed
// words can't be recovered from what was sent or from the device.

struct VoiceEdit: Equatable {
    var removedWords: Set<Int> = []
    var trimStart: Double = 0
    var trimEnd: Double
    var cuts: [ClosedRange<Double>] = []
    /// Long pauses (indices into `longPauses`) shortened to 0.3 s. Each can be restored alone.
    var shortenedPauses: Set<Int> = []
}

@MainActor @Observable
final class VoiceEditorModel: Identifiable {
    let clip: VoiceClip
    nonisolated var id: String { clip.id }
    private(set) var words: [VoiceTranscript.Word]
    private(set) var fullText: String
    var transcribing = false
    /// Review (false) or the full editor (true).
    var editing = false
    let duration: Double
    private(set) var edit: VoiceEdit
    private var undoStack: [VoiceEdit] = []
    private var redoStack: [VoiceEdit] = []
    /// A waveform selection (original seconds), for Cut.
    var selection: ClosedRange<Double>?
    /// Playhead in original seconds.
    var playhead: Double = 0
    var isPlaying = false
    var rendering = false
    /// UI tests (`-RodaVoiceEditorTest YES`): the rendered file's real duration and the
    /// transcript that would be sent, instead of sending.
    var testResult: String?
    private var player: AVPlayer?
    private var tick: Task<Void, Never>?

    static let fillers: Set<String> = ["uh", "um", "uhm", "umm", "er", "erm", "hmm", "hum", "ahn", "ã", "é", "eh", "tipo"]
    static let longPause = 0.7, keptPause = 0.3, fade = 0.015

    init(clip: VoiceClip, transcript: VoiceTranscript?) {
        self.clip = clip
        self.words = transcript?.words ?? []
        self.fullText = transcript?.text ?? ""
        self.duration = Double(clip.ms) / 1000
        self.edit = VoiceEdit(trimEnd: Double(clip.ms) / 1000)
    }

    /// Word timings come from the on-device transcriber after the sheet opens.
    func loadTranscript() async {
        guard words.isEmpty, fullText.isEmpty else { return }
        transcribing = true
        let t = await VoiceTranscriber.transcribe(clip.url)
        transcribing = false
        words = t?.words ?? []
        fullText = t?.text ?? ""
    }

    // Undo / redo
    var canUndo: Bool { !undoStack.isEmpty }
    var canRedo: Bool { !redoStack.isEmpty }
    func apply(_ change: (inout VoiceEdit) -> Void) {
        var next = edit
        change(&next)
        guard next != edit else { return }
        undoStack.append(edit)
        redoStack.removeAll()
        edit = next
        refreshPreview()
    }
    /// Drags (trim handles) coalesce into one undo step per gesture.
    private var liveKey: String?
    func applyLive(_ key: String, _ change: (inout VoiceEdit) -> Void) {
        if liveKey != key { undoStack.append(edit); redoStack.removeAll(); liveKey = key }
        change(&edit)
    }
    func endLive() { if liveKey != nil { liveKey = nil; refreshPreview() } }
    func undo() { guard let e = undoStack.popLast() else { return }; redoStack.append(edit); edit = e; refreshPreview() }
    func redo() { guard let e = redoStack.popLast() else { return }; undoStack.append(edit); edit = e; refreshPreview() }

    func toggleWord(_ i: Int) { apply { if $0.removedWords.contains(i) { $0.removedWords.remove(i) } else { $0.removedWords.insert(i) } } }
    func setWords(_ ix: [Int], removed: Bool) { apply { for i in ix { if removed { $0.removedWords.insert(i) } else { $0.removedWords.remove(i) } } } }
    func cutSelection() {
        guard let s = selection else { return }
        apply { $0.cuts.append(s) }
        selection = nil
    }

    /// Whether word `i` is removed by anything (a strike, a cut, a trim or a pause).
    func wordRemoved(_ i: Int) -> Bool {
        guard words.indices.contains(i) else { return false }
        return edit.removedWords.contains(i) || isRemoved(words[i].start + words[i].duration / 2)
    }

    /// Restores exactly these words: un-strikes them and carves their span out of any cut or
    /// trim that covers them (other cuts stay).
    func restoreWords(_ ix: [Int]) {
        apply { e in
            for i in ix where words.indices.contains(i) {
                let a = words[i].start, b = words[i].start + words[i].duration
                e.removedWords.remove(i)
                e.cuts = e.cuts.flatMap { Self.subtract(a...b, from: $0) }
                if e.trimStart > a { e.trimStart = max(0, a - 0.02) }
                if e.trimEnd < b { e.trimEnd = min(duration, b + 0.02) }
            }
        }
    }

    /// Tapping a removed stretch of waveform restores that whole stretch (one undo step).
    @discardableResult
    func restoreRegion(at t: Double) -> Bool {
        guard let region = removedRanges.first(where: { $0.contains(t) }) else { return false }
        let pauses = longPauses
        apply { e in
            e.cuts.removeAll { $0.overlaps(region) }
            for i in e.removedWords where words.indices.contains(i) {
                let w = words[i].start...(words[i].start + words[i].duration)
                if w.overlaps(region) { e.removedWords.remove(i) }
            }
            for k in e.shortenedPauses where pauses.indices.contains(k) && pauses[k].overlaps(region) {
                e.shortenedPauses.remove(k)
            }
            if region.lowerBound <= 0.001 { e.trimStart = 0 }
            if region.upperBound >= duration - 0.001 { e.trimEnd = duration }
        }
        return true
    }

    /// Separate removed stretches and the time they take out ("4 cuts · −3.2s").
    var cutCount: Int { removedRanges.count }
    var removedSeconds: Double { max(0, duration - keptDuration) }

    static func subtract(_ hole: ClosedRange<Double>, from r: ClosedRange<Double>) -> [ClosedRange<Double>] {
        guard r.overlaps(hole) else { return [r] }
        var out: [ClosedRange<Double>] = []
        if hole.lowerBound - r.lowerBound > 0.02 { out.append(r.lowerBound...hole.lowerBound) }
        if r.upperBound - hole.upperBound > 0.02 { out.append(hole.upperBound...r.upperBound) }
        return out
    }

    // Suggestions
    var fillerWords: [Int] {
        words.indices.filter { i in
            let w = words[i].text.lowercased().trimmingCharacters(in: .punctuationCharacters.union(.whitespaces))
            return Self.fillers.contains(w) && !edit.removedWords.contains(i)
        }
    }
    /// Gaps between words longer than 0.7 s.
    var longPauses: [ClosedRange<Double>] {
        guard words.count > 1 else { return [] }
        return (1..<words.count).compactMap { i in
            let a = words[i - 1].start + words[i - 1].duration, b = words[i].start
            return b - a > Self.longPause ? a...b : nil
        }
    }

    // The edit decision list
    var removedRanges: [ClosedRange<Double>] {
        var r: [ClosedRange<Double>] = []
        if edit.trimStart > 0 { r.append(0...edit.trimStart) }
        if edit.trimEnd < duration { r.append(edit.trimEnd...duration) }
        for i in edit.removedWords where words.indices.contains(i) {
            r.append(words[i].start...(words[i].start + words[i].duration))
        }
        r += edit.cuts
        for (k, p) in longPauses.enumerated() where edit.shortenedPauses.contains(k) {
            let keepHalf = Self.keptPause / 2
            r.append((p.lowerBound + keepHalf)...(p.upperBound - keepHalf))
        }
        return Self.merge(r)
    }

    var keptRanges: [ClosedRange<Double>] {
        var kept: [ClosedRange<Double>] = []
        var t = 0.0
        for r in removedRanges {
            if r.lowerBound - t >= 0.05 { kept.append(t...r.lowerBound) }
            t = max(t, r.upperBound)
        }
        if duration - t >= 0.05 { kept.append(t...duration) }
        return kept
    }

    var keptDuration: Double { keptRanges.reduce(0) { $0 + $1.upperBound - $1.lowerBound } }

    /// What gets sent as the transcript: the full text if nothing was removed, else only the
    /// words whose middle survives the edit.
    var keptTranscript: String {
        guard !words.isEmpty else { return "" }
        let kept = keptRanges
        let survivors = words.enumerated().filter { i, w in
            let mid = w.start + w.duration / 2
            return !edit.removedWords.contains(i) && kept.contains { $0.contains(mid) }
        }
        if survivors.count == words.count { return fullText }
        return survivors.map(\.element.text).joined(separator: " ")
    }

    func isRemoved(_ t: Double) -> Bool { removedRanges.contains { $0.contains(t) } }

    static func merge(_ x: [ClosedRange<Double>]) -> [ClosedRange<Double>] {
        let s = x.sorted { $0.lowerBound < $1.lowerBound }
        var out: [ClosedRange<Double>] = []
        for r in s {
            if let last = out.last, r.lowerBound <= last.upperBound { out[out.count - 1] = last.lowerBound...max(last.upperBound, r.upperBound) }
            else { out.append(r) }
        }
        return out
    }

    // Time mapping between the original and the edit.
    func editedTime(_ original: Double) -> Double {
        var acc = 0.0
        for r in keptRanges {
            if original < r.lowerBound { return acc }
            if original <= r.upperBound { return acc + original - r.lowerBound }
            acc += r.upperBound - r.lowerBound
        }
        return acc
    }
    func originalTime(_ edited: Double) -> Double {
        var acc = 0.0
        for r in keptRanges {
            let len = r.upperBound - r.lowerBound
            if edited <= acc + len { return r.lowerBound + edited - acc }
            acc += len
        }
        return keptRanges.last?.upperBound ?? 0
    }

    // Composition
    private func composition() async throws -> (AVMutableComposition, AVAudioMix) {
        let asset = AVURLAsset(url: clip.url)
        guard let src = try await asset.loadTracks(withMediaType: .audio).first else { throw CocoaError(.fileReadCorruptFile) }
        let comp = AVMutableComposition()
        guard let track = comp.addMutableTrack(withMediaType: .audio, preferredTrackID: kCMPersistentTrackID_Invalid) else { throw CocoaError(.fileWriteUnknown) }
        let params = AVMutableAudioMixInputParameters(track: track)
        let scale: CMTimeScale = 44_100
        let fade = CMTime(seconds: Self.fade, preferredTimescale: scale)
        var cursor = CMTime.zero
        for r in keptRanges {
            let range = CMTimeRange(start: CMTime(seconds: r.lowerBound, preferredTimescale: scale),
                                    end: CMTime(seconds: r.upperBound, preferredTimescale: scale))
            try track.insertTimeRange(range, of: src, at: cursor)
            let end = cursor + range.duration
            params.setVolumeRamp(fromStartVolume: 0, toEndVolume: 1, timeRange: CMTimeRange(start: cursor, duration: fade))
            params.setVolumeRamp(fromStartVolume: 1, toEndVolume: 0, timeRange: CMTimeRange(start: end - fade, duration: fade))
            cursor = end
        }
        let mix = AVMutableAudioMix()
        mix.inputParameters = [params]
        return (comp, mix)
    }

    /// Renders the kept ranges into a new AAC file and deletes the original.
    func render() async -> (VoiceClip, String)? {
        rendering = true
        defer { rendering = false }
        stopPreview()
        let untouched = keptRanges.count == 1 && keptRanges[0].lowerBound < 0.01 && keptRanges[0].upperBound > duration - 0.01
        if untouched { return (clip, keptTranscript) }
        do {
            let (comp, mix) = try await composition()
            guard let session = AVAssetExportSession(asset: comp, presetName: AVAssetExportPresetAppleM4A) else { return nil }
            session.audioMix = mix
            let id = UUID().uuidString
            let out = VoiceStore.url(id)
            try await session.export(to: out, as: .m4a)
            let ms = Int(keptDuration * 1000)
            let text = keptTranscript
            VoiceStore.delete(clip.url)  // the original (with the removed words) is gone
            return (VoiceClip(id: id, url: out, ms: ms, levels: keptLevels(40), detail: keptLevels(160)), text)
        } catch {
            return nil
        }
    }

    private func keptLevels(_ n: Int) -> [CGFloat] {
        let src = clip.wave
        guard !src.isEmpty, keptDuration > 0 else { return clip.levels }
        return (0..<n).map { i in
            let t = originalTime((Double(i) + 0.5) / Double(n) * keptDuration)
            let k = min(src.count - 1, max(0, Int(t / duration * Double(src.count))))
            return src[k]
        }
    }

    // Preview (plays the edit, so removed parts are skipped)
    func togglePreview() {
        if isPlaying { player?.pause(); isPlaying = false; tick?.cancel(); return }
        Task { await startPreview() }
    }

    private func startPreview() async {
        #if os(iOS)
        try? AVAudioSession.sharedInstance().setCategory(.playback)
        try? AVAudioSession.sharedInstance().setActive(true)
        #endif
        if player == nil {
            guard let (comp, mix) = try? await composition() else { return }
            let item = AVPlayerItem(asset: comp)
            item.audioMix = mix
            player = AVPlayer(playerItem: item)
        }
        var from = editedTime(playhead)
        if from >= keptDuration - 0.05 { from = 0 }
        await player?.seek(to: CMTime(seconds: from, preferredTimescale: 600), toleranceBefore: .zero, toleranceAfter: .zero)
        player?.play()
        isPlaying = true
        tick?.cancel()
        tick = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(30))
                guard let self, let p = self.player else { return }
                let t = p.currentTime().seconds
                self.playhead = self.originalTime(t)
                if t >= self.keptDuration - 0.02 || p.rate == 0 {
                    self.isPlaying = false
                    return
                }
            }
        }
    }

    func seek(_ original: Double) {
        playhead = max(0, min(duration, original))
        if isPlaying { player?.seek(to: CMTime(seconds: editedTime(playhead), preferredTimescale: 600)) }
    }

    func stopPreview() { tick?.cancel(); player?.pause(); isPlaying = false }

    private func refreshPreview() {
        let was = isPlaying
        stopPreview()
        player = nil
        if was { Task { await startPreview() } }
    }

    /// Discard: the original recording is deleted.
    func discard() { stopPreview(); VoiceStore.delete(clip.url) }
}

// MARK: Sheet

struct VoiceReviewSheet: View {
    @State var model: VoiceEditorModel
    var onSend: (VoiceClip, String) -> Void
    @Environment(\.dismiss) private var dismiss
    private var editing: Bool { model.editing }

    init(model: VoiceEditorModel, onSend: @escaping (VoiceClip, String) -> Void) {
        _model = State(initialValue: model)
        self.onSend = onSend
    }
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(spacing: 0) {
            header
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if editing { cutSummary }
                    WaveEditor(model: model, editing: editing)
                        .frame(height: editing ? 86 : 56)
                    if editing { suggestions }
                    transcript
                    if editing {
                        Text("Only the edited audio and the words you keep are sent. The original recording is deleted from this iPhone.")
                            .font(.footnote)
                            .foregroundStyle(Palette.textSecondary)
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 8)
            }
            footer
        }
        .background(Palette.background)
        .presentationDetents(editing ? [.large] : [.medium, .large])
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled(model.rendering)
        .task { await model.loadTranscript() }
        .onDisappear { model.stopPreview() }
    }

    private var header: some View {
        HStack {
            if editing {
                Button { model.undo() } label: { Image(systemName: "arrow.uturn.backward").font(.system(size: 17, weight: .medium)).frame(width: 40, height: 40) }
                    .disabled(!model.canUndo).opacity(model.canUndo ? 1 : 0.35)
                    .accessibilityLabel(Text("Undo"))
                Button { model.redo() } label: { Image(systemName: "arrow.uturn.forward").font(.system(size: 17, weight: .medium)).frame(width: 40, height: 40) }
                    .disabled(!model.canRedo).opacity(model.canRedo ? 1 : 0.35)
                    .accessibilityLabel(Text("Redo"))
            } else {
                Color.clear.frame(width: 40, height: 40)
            }
            Spacer()
            Text(editing ? "Edit voice message" : "Voice message").font(.headline)
            Spacer()
            if editing {
                Button { withAnimation(.snappy) { model.editing = false } } label: { Text("Done").font(.body.weight(.semibold)) }
                    .frame(minWidth: 80, alignment: .trailing)
            } else {
                Color.clear.frame(width: 80, height: 40)
            }
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.textPrimary)
        .padding(.horizontal, 12)
        .padding(.top, 14)
        .padding(.bottom, 6)
    }

    @ViewBuilder private var suggestions: some View {
        let fillers = model.fillerWords
        let pauses = model.longPauses.indices.filter { !model.edit.shortenedPauses.contains($0) }
        if model.selection != nil || !fillers.isEmpty || !pauses.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    if model.selection != nil {
                        Button { Haptics.action(); withAnimation(.snappy) { model.cutSelection() } } label: {
                            HStack(spacing: 6) { ZoenIcon(.close, size: 14); Text("Cut selection").font(.subheadline.weight(.semibold)) }
                                .foregroundStyle(.white)
                                .padding(.horizontal, 12).padding(.vertical, 8)
                                .background(Palette.danger, in: .capsule)
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("cut-selection")
                    }
                    if !fillers.isEmpty {
                        chip(String(localized: "Remove \(fillers.count) fillers"), glyph: .sparkle) { model.setWords(fillers, removed: true) }
                            .accessibilityIdentifier("remove-fillers")
                    }
                    if !pauses.isEmpty {
                        chip(String(localized: "Shorten \(pauses.count) pauses"), glyph: .sun) { model.apply { $0.shortenedPauses.formUnion(pauses) } }
                            .accessibilityIdentifier("shorten-pauses")
                    }
                }
            }
            .scrollClipDisabled()
        }
    }

    /// "4 cuts · −3.2s" and how to restore.
    private var cutSummary: some View {
        HStack(spacing: 6) {
            if model.cutCount > 0 {
                Text(String(localized: "\(model.cutCount) cuts") + " · −" + String(format: "%.1fs", model.removedSeconds))
                    .font(.subheadline.weight(.semibold).monospacedDigit())
                    .foregroundStyle(Palette.danger)
                    .contentTransition(.numericText())
                Text("Tap a cut to restore it")
                    .font(.caption)
                    .foregroundStyle(Palette.textSecondary)
            } else {
                Text("Tap words or drag across the waveform to cut")
                    .font(.caption)
                    .foregroundStyle(Palette.textSecondary)
            }
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("cut-summary")
        .animation(.snappy, value: model.cutCount)
    }

    private func chip(_ text: String, glyph: ZoenGlyph, action: @escaping () -> Void) -> some View {
        Button { Haptics.selectionTick(); withAnimation(.snappy) { action() } } label: {
            HStack(spacing: 6) {
                ZoenIcon(glyph, size: 15)
                Text(text).font(.subheadline.weight(.semibold))
            }
            .foregroundStyle(Palette.action)
            .padding(.horizontal, 12).padding(.vertical, 8)
            .background(Palette.action.opacity(0.12), in: .capsule)
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder private var transcript: some View {
        if model.transcribing {
            HStack(spacing: 8) {
                ProgressView()
                Text("Transcribing on this iPhone…").font(.callout).foregroundStyle(Palette.textSecondary)
            }
        } else if model.words.isEmpty {
            Text(model.fullText.isEmpty ? String(localized: "No transcript. This iPhone couldn’t transcribe the recording on device.") : model.fullText)
                .font(.callout)
                .foregroundStyle(Palette.textSecondary)
        } else if editing {
            WordStrip(model: model)
        } else {
            Text(model.keptTranscript)
                .font(.body)
                .foregroundStyle(Palette.textPrimary)
        }
    }

    private var footer: some View {
        HStack(spacing: 12) {
            Button {
                model.discard()
                Haptics.dismiss()
                dismiss()
            } label: {
                ZoenIcon(.trash, size: 20).foregroundStyle(Palette.danger).frame(width: 48, height: 48)
            }
            .glassEffect(.regular.interactive(), in: .circle)
            .accessibilityLabel(Text("Delete recording"))

            Button { model.togglePreview() } label: {
                HStack(spacing: 8) {
                    ZoenIcon(model.isPlaying ? .pause : .play, size: 17)
                    Text("\(VoiceFormat.time(model.editedTime(model.playhead))) / \(VoiceFormat.time(model.keptDuration))")
                        .font(.subheadline.monospacedDigit())
                }
                .foregroundStyle(Palette.textPrimary)
                .frame(height: 48)
                .padding(.horizontal, 16)
            }
            .glassEffect(.regular.interactive(), in: .capsule)
            .accessibilityLabel(model.isPlaying ? Text("Pause preview") : Text("Play preview"))

            if !editing {
                Button { withAnimation(.snappy) { model.editing = true } } label: {
                    Text("Edit").font(.body.weight(.semibold)).foregroundStyle(Palette.textPrimary).frame(height: 48).padding(.horizontal, 16)
                }
                .glassEffect(.regular.interactive(), in: .capsule)
            }
            Spacer(minLength: 0)
            Button {
                Task {
                    if let (clip, text) = await model.render() {
                        Haptics.commit()
                        if UserDefaults.standard.bool(forKey: "RodaVoiceEditorTest") {
                            // UI test: report the real exported duration + transcript, don't send.
                            let secs = (try? await AVURLAsset(url: clip.url).load(.duration).seconds) ?? -1
                            model.testResult = String(format: "file=%.2f expected=%.2f text=", secs, Double(clip.ms) / 1000) + text
                            return
                        }
                        onSend(clip, text)
                        dismiss()
                    } else {
                        Haptics.warning()
                    }
                }
            } label: {
                Group {
                    if model.rendering { ProgressView().tint(.white) } else { ZoenIcon(.send, size: 21) }
                }
                .foregroundStyle(.white)
                .frame(width: 48, height: 48)
            }
            .glassEffect(.regular.tint(Palette.action).interactive(), in: .circle)
            .disabled(model.rendering || model.keptDuration < 0.3)
            .accessibilityLabel(Text("Send voice message"))
            .accessibilityIdentifier("voice-send")
        }
        .overlay(alignment: .top) {
            if let r = model.testResult {
                Text(r).font(.caption2).accessibilityIdentifier("render-result").offset(y: -24)
            }
        }
        .buttonStyle(IconPressStyle())
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }
}

/// Waveform with trim handles, removed stretches struck through, a playhead, and drag to
/// select a stretch for Cut. A tap seeks.
struct WaveEditor: View {
    let model: VoiceEditorModel
    var editing: Bool
    @State private var dragStart: Double?

    var body: some View {
        GeometryReader { g in
            let w = g.size.width, d = max(0.01, model.duration)
            let x = { (t: Double) in CGFloat(t / d) * w }
            let time = { (px: CGFloat) in max(0, min(d, Double(px / w) * d)) }
            ZStack(alignment: .leading) {
                if let s = model.selection {
                    HighlighterBlob(seed: 11)
                        .fill(InkPalette.butter.opacity(0.55))
                        .frame(width: max(8, x(s.upperBound) - x(s.lowerBound)), height: g.size.height * 0.9)
                        .offset(x: x(s.lowerBound))
                }
                Canvas { ctx, size in
                    let lv = model.clip.wave
                    let n = lv.count
                    guard n > 0 else { return }
                    let step = size.width / CGFloat(n)
                    let bw = max(1.4, step * 0.6)
                    for (i, l) in lv.enumerated() {
                        let t = (Double(i) + 0.5) / Double(n) * d
                        let h = max(3, l * size.height * 0.86)
                        let r = CGRect(x: CGFloat(i) * step + (step - bw) / 2, y: (size.height - h) / 2, width: bw, height: h)
                        let removed = model.isRemoved(t)
                        let played = t <= model.playhead
                        let c: Color = removed ? Palette.textTertiary.opacity(0.35) : (played ? Palette.action : Palette.textPrimary.opacity(0.7))
                        ctx.fill(Path(roundedRect: r, cornerRadius: bw / 2), with: .color(c))
                    }
                    // Ink strike through removed stretches.
                    for r in model.removedRanges {
                        let a = CGFloat(r.lowerBound / d) * size.width, b = CGFloat(r.upperBound / d) * size.width
                        guard b - a > 2 else { continue }
                        var p = Path()
                        p.move(to: CGPoint(x: a, y: size.height * 0.55))
                        p.addQuadCurve(to: CGPoint(x: b, y: size.height * 0.45), control: CGPoint(x: (a + b) / 2, y: size.height * 0.62))
                        ctx.stroke(p, with: .color(Palette.danger.opacity(0.85)), style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    }
                }
                // Playhead
                Capsule().fill(Palette.action).frame(width: 2.5, height: g.size.height)
                    .offset(x: x(model.playhead) - 1.25)
                if editing {
                    TrimHandle().offset(x: x(model.edit.trimStart) - 7)
                        .gesture(DragGesture().onChanged { v in
                            let t = min(time(v.location.x), model.edit.trimEnd - 0.4)
                            model.applyLive("trimStart") { $0.trimStart = max(0, t) }
                        }.onEnded { _ in model.endLive() })
                        .accessibilityLabel(Text("Trim start"))
                    TrimHandle().offset(x: x(model.edit.trimEnd) - 7)
                        .gesture(DragGesture().onChanged { v in
                            let t = max(time(v.location.x), model.edit.trimStart + 0.4)
                            model.applyLive("trimEnd") { $0.trimEnd = min(d, t) }
                        }.onEnded { _ in model.endLive() })
                        .accessibilityLabel(Text("Trim end"))
                }
            }
            .contentShape(.rect)
            .gesture(DragGesture(minimumDistance: 0).onChanged { v in
                if abs(v.translation.width) < 6 { return }
                guard editing else { model.seek(time(v.location.x)); return }
                let a = time(v.startLocation.x), b = time(v.location.x)
                model.selection = min(a, b)...max(a, b)
            }.onEnded { v in
                guard abs(v.translation.width) < 6 else { return }
                model.selection = nil
                let t = time(v.location.x)
                // Tapping a removed stretch restores it; elsewhere, a tap seeks.
                if editing, model.isRemoved(t) {
                    Haptics.selectionTick()
                    withAnimation(.snappy) { _ = model.restoreRegion(at: t) }
                } else {
                    model.seek(t)
                }
            })
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("waveform")
    }
}

private struct TrimHandle: View {
    var body: some View {
        RoundedRectangle(cornerRadius: 4, style: .continuous)
            .fill(InkPalette.butter)
            .overlay(Capsule().fill(.black.opacity(0.45)).frame(width: 2, height: 16))
            .frame(width: 14)
            .frame(maxHeight: .infinity)
            .shadow(color: .black.opacity(0.15), radius: 2, y: 1)
            .contentShape(.rect.inset(by: -10))
    }
}

/// The transcript as tappable words; struck words get an ink line. Drag across words to
/// strike (or restore) a run.
struct WordStrip: View {
    let model: VoiceEditorModel
    @State private var frames: [Int: CGRect] = [:]
    @State private var dragMode: Bool?
    @State private var touched: Set<Int> = []

    var body: some View {
        let fillers = Set(model.fillerWords)
        FlowLayout(spacing: 4, lineSpacing: 8) {
            ForEach(model.words.indices, id: \.self) { i in
                let removed = model.wordRemoved(i)
                Text(model.words[i].text)
                    .font(.body)
                    .foregroundStyle(removed ? Palette.textTertiary : Palette.textPrimary)
                    .padding(.horizontal, 4).padding(.vertical, 3)
                    .background {
                        if fillers.contains(i) {
                            HighlighterBlob(seed: UInt64(i + 3)).fill(InkPalette.butter.opacity(0.5))
                        }
                    }
                    .overlay { if removed { InkStrike().stroke(Palette.danger, style: StrokeStyle(lineWidth: 2, lineCap: .round)) } }
                    .onGeometryChange(for: CGRect.self) { $0.frame(in: .named("words")) } action: { frames[i] = $0 }
                    .accessibilityAddTraits(.isButton)
                    .accessibilityLabel(Text(model.words[i].text))
                    .accessibilityValue(removed ? Text("Removed") : Text(""))
                    .accessibilityAction { removed ? model.restoreWords([i]) : model.toggleWord(i) }
                    .accessibilityIdentifier("word-\(i)")
            }
        }
        .coordinateSpace(.named("words"))
        .contentShape(.rect)
        .gesture(DragGesture(minimumDistance: 0, coordinateSpace: .named("words")).onChanged { v in
            guard let i = frames.first(where: { $0.value.insetBy(dx: -2, dy: -4).contains(v.location) })?.key else { return }
            if dragMode == nil {
                dragMode = !model.wordRemoved(i)
                touched = []
            }
            guard !touched.contains(i) else { return }
            touched.insert(i)
            if abs(v.translation.width) + abs(v.translation.height) > 4 || touched.count == 1 {
                Haptics.selectionTick()
            }
        }.onEnded { _ in
            if let mode = dragMode, !touched.isEmpty {
                // Striking adds word cuts; touching a removed word restores just that word
                // (even when a waveform cut or trim removed it).
                if mode { model.setWords(Array(touched), removed: true) } else { model.restoreWords(Array(touched)) }
            }
            dragMode = nil
            touched = []
            model.seek(model.playhead)
        })
    }
}

/// A slightly wobbly hand-drawn strike line.
struct InkStrike: Shape {
    func path(in r: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: r.minX + 1, y: r.midY + 1.5))
        p.addCurve(to: CGPoint(x: r.maxX - 1, y: r.midY - 1),
                   control1: CGPoint(x: r.minX + r.width * 0.35, y: r.midY - 1.5),
                   control2: CGPoint(x: r.minX + r.width * 0.65, y: r.midY + 2))
        return p
    }
}

/// Wraps children onto lines.
struct FlowLayout: Layout {
    var spacing: CGFloat = 4
    var lineSpacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let maxW = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, lineH: CGFloat = 0, widest: CGFloat = 0
        for s in subviews {
            let z = s.sizeThatFits(.unspecified)
            if x > 0 && x + z.width > maxW { y += lineH + lineSpacing; x = 0; lineH = 0 }
            x += z.width + spacing
            lineH = max(lineH, z.height)
            widest = max(widest, x - spacing)
        }
        return CGSize(width: proposal.width ?? widest, height: y + lineH)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, lineH: CGFloat = 0
        for s in subviews {
            let z = s.sizeThatFits(.unspecified)
            if x > bounds.minX && x + z.width > bounds.maxX { y += lineH + lineSpacing; x = bounds.minX; lineH = 0 }
            s.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(z))
            x += z.width + spacing
            lineH = max(lineH, z.height)
        }
    }
}

extension VoiceTranscript {
    /// Demo words for the editor screenshots (`-RodaVoiceEditor`), timed over a 7 s clip with
    /// one long pause.
    static func demo(seconds: Double) -> VoiceTranscript {
        let en = "So um I think we leave at eight, uh, and bring the snacks and the good thermos"
        let pt = "Então hum acho que a gente sai às oito, tipo, e leva os lanches e a garrafa térmica boa"
        let text = AppLocale.isPortuguese ? pt : en
        let tokens = text.split(separator: " ").map(String.init)
        var words: [Word] = []
        var t = 0.25
        let per = (seconds - 0.5 - 1.2) / Double(tokens.count)
        for (i, w) in tokens.enumerated() {
            if i == tokens.count / 2 { t += 1.2 }  // a long pause in the middle
            words.append(Word(text: w, start: t, duration: per * 0.82))
            t += per
        }
        return VoiceTranscript(text: text, words: words)
    }
}
