import SwiftUI

/// Long-hold on the radial + : fan collapses, + morphs into mic, records with the same
/// gesture language as the chat composer, then sends to Zoen and opens that chat.
struct PlusVoiceCapture: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var scheme
    var triggerSize: CGFloat = 56
    /// Bumped by RadialMenu when the finger that armed the hold lifts — finishes the take.
    var fingerUp: Int = 0
    var onFinished: () -> Void

    @State private var recorder = VoiceRecorder()
    @State private var drag: CGSize = .zero
    @State private var review: VoiceEditorModel?
    @State private var morphMic = false
    @Namespace private var glassNS

    var body: some View {
        ZStack(alignment: .bottomTrailing) {
            if recorder.phase == .recording {
                LockHint(dragY: drag.height)
                    .offset(x: -4, y: -triggerSize - 56)
                    .transition(.opacity.combined(with: .move(edge: .bottom)))
            }
            HStack(alignment: .bottom, spacing: 10) {
                if recorder.phase != .idle {
                    RecordingRow(recorder: recorder, dragX: drag.width,
                                 onDelete: cancel, onStop: stopForReview)
                        .frame(width: 300)
                        // Opaque under the glass: the row sits over the tab bar's own content.
                        .background(Palette.surface, in: .rect(cornerRadius: 22, style: .continuous))
                        .glassEffect(.regular, in: .rect(cornerRadius: 22, style: .continuous))
                        .transition(.opacity.combined(with: .scale(scale: 0.96, anchor: .trailing)))
                }
                micButton
            }
            .fixedSize()
        }
        // Sits in the +'s slot: keep the mic there and let the row extend to the left.
        .frame(width: triggerSize, height: triggerSize, alignment: .bottomTrailing)
        .animation(.spring(duration: 0.35, bounce: 0.25), value: recorder.phase)
        .sheet(item: $review) { m in
            VoiceReviewSheet(model: m) { clip, text in
                review = nil
                Task { await deliver(clip, transcript: text) }
            }
        }
        .task {
            // UITest / screenshots: `-RodaVoiceDemo plus` (or any value) fakes input (no mic permission).
            if UserDefaults.standard.string(forKey: "RodaVoiceDemo") != nil {
                recorder.demo = true
            }
            withAnimation(reduceMotion ? .easeInOut(duration: 0.2) : .spring(duration: 0.42, bounce: 0.28)) {
                morphMic = true
            }
            Haptics.recordLock()
            if await recorder.start() {
                // ready
            } else {
                Haptics.warning()
                onFinished()
            }
        }
        .onChange(of: fingerUp) { _, _ in
            // Finger that armed the long-hold lifted while we still own the take.
            if recorder.phase == .recording { finishAndSend() }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(String(localized: "Recording for Zoen"))
    }

    private var micButton: some View {
        let recording = recorder.phase == .recording
        return ZStack {
            ZoenIcon(morphMic || recording ? .mic : .plus, size: morphMic ? 21 : 25)
                .foregroundStyle(scheme == .dark ? Color(hex: "#10200A") : .white)
        }
        .frame(width: triggerSize, height: triggerSize)
        .glassEffect(
            recording
                ? .regular.tint(Palette.danger).interactive()
                : .regular.tint(scheme == .dark ? Palette.action : Color(hex: "#1D2418").opacity(0.92)).interactive(),
            in: .circle
        )
        .glassEffectID("plus-mic", in: glassNS)
        .scaleEffect(recording ? 1.18 : 1)
        .offset(x: recording ? min(0, drag.width) * 0.35 : 0,
                y: recording ? min(0, drag.height) * 0.35 : 0)
        .gesture(holdGesture)
        .accessibilityLabel(String(localized: "Hold to record for Zoen"))
    }

    private var holdGesture: some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { v in
                guard recorder.phase == .recording else { return }
                drag = v.translation
                if v.translation.width < -110 {
                    cancel()
                } else if v.translation.height < -80 {
                    Haptics.recordLock()
                    withAnimation(.spring(duration: 0.35, bounce: 0.25)) { recorder.lock() }
                    drag = .zero
                }
            }
            .onEnded { _ in
                drag = .zero
                if recorder.phase == .recording {
                    finishAndSend()
                }
            }
    }

    private func finishAndSend() {
        guard let clip = recorder.finish() else { Haptics.warning(); onFinished(); return }
        Task { await deliver(clip, transcript: nil) }
    }

    private func stopForReview() {
        guard let clip = recorder.finish() else { Haptics.warning(); return }
        Haptics.tap()
        review = VoiceEditorModel(clip: clip, transcript: nil)
    }

    private func cancel() {
        Haptics.warning()
        withAnimation(.snappy) { recorder.cancel() }
        onFinished()
    }

    private func deliver(_ clip: VoiceClip, transcript: String?) async {
        guard let zid = model.zoenSpaceId() else {
            Haptics.warning()
            onFinished()
            return
        }
        // Mark the clip so Zoen's chat can play the land / send-flight on the bubble.
        model.pendingVoiceFlight = clip.id
        await model.sendVoice(clip, in: zid, transcript: transcript)
        Haptics.commit()
        withAnimation(reduceMotion ? .easeOut(duration: 0.2) : .spring(duration: 0.4, bounce: 0.22)) {
            morphMic = false
        }
        // Navigate into Zoen so the bubble (and reply) are visible — no toast.
        model.openZoenChat()
        onFinished()
    }
}
