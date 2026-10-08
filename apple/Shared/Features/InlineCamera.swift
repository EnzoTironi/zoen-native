import SwiftUI
#if os(iOS)
import AVFoundation
import UIKit

/// A camera that opens inside the composer (not the system modal). On a device it shows
/// a live preview. The Simulator has no camera, so it shows Zo instead and says so.
/// Captured photos aren't sent yet: attachments don't exist in the core.
@MainActor @Observable
final class InlineCamera {
    enum State: Equatable { case starting, running, unavailable, denied }
    var state: State = .starting
    nonisolated(unsafe) let session = AVCaptureSession()
    private var configured = false

    func start() async {
        guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back) else {
            state = .unavailable
            return
        }
        var allowed = AVCaptureDevice.authorizationStatus(for: .video) == .authorized
        if AVCaptureDevice.authorizationStatus(for: .video) == .notDetermined {
            allowed = await AVCaptureDevice.requestAccess(for: .video)
        }
        guard allowed else { state = .denied; return }
        if !configured {
            guard let input = try? AVCaptureDeviceInput(device: device), session.canAddInput(input) else { state = .unavailable; return }
            session.beginConfiguration()
            session.sessionPreset = .photo
            session.addInput(input)
            session.commitConfiguration()
            configured = true
        }
        let s = session
        await Task.detached { s.startRunning() }.value
        state = .running
    }

    func stop() {
        let s = session
        Task.detached { s.stopRunning() }
    }
}

final class CameraPreviewView: UIView {
    override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
    var previewLayer: AVCaptureVideoPreviewLayer { layer as! AVCaptureVideoPreviewLayer }
}

struct CameraPreview: UIViewRepresentable {
    let session: AVCaptureSession
    func makeUIView(context: Context) -> CameraPreviewView {
        let v = CameraPreviewView()
        v.previewLayer.session = session
        v.previewLayer.videoGravity = .resizeAspectFill
        return v
    }
    func updateUIView(_ uiView: CameraPreviewView, context: Context) {}
}

struct InlineCameraPanel: View {
    var onClose: () -> Void
    var onCapture: () -> Void
    @State private var camera = InlineCamera()
    @State private var flash = false

    var body: some View {
        ZStack {
            switch camera.state {
            case .running:
                CameraPreview(session: camera.session)
            case .starting:
                Color.black.opacity(0.85)
            case .unavailable, .denied:
                LinearGradient(colors: [Mascot.belly.opacity(0.5), InkPalette.paper], startPoint: .top, endPoint: .bottom)
                VStack(spacing: 4) {
                    MascotView(pose: camera.state == .denied ? .shield : .zen).frame(width: 120, height: 120)
                    Text(camera.state == .denied ? String(localized: "Camera access is off. You can turn it on in Settings.") : String(localized: "No camera in the Simulator. On an iPhone, it opens right here."))
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(InkPalette.ink.opacity(0.7))
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 24)
                }
            }
            Color.white.opacity(flash ? 0.9 : 0).allowsHitTesting(false)
        }
        .overlay(alignment: .topTrailing) {
            Button(action: onClose) {
                Image(systemName: "xmark").font(.footnote.weight(.bold)).frame(width: 32, height: 32)
            }
            .buttonStyle(.plain)
            .foregroundStyle(camera.state == .running || camera.state == .starting ? .white : InkPalette.ink)
            .glassEffect(.regular.interactive(), in: .circle)
            .padding(10)
            .accessibilityLabel("Close camera")
        }
        .overlay(alignment: .bottom) {
            Button {
                Haptics.commit()
                withAnimation(.easeOut(duration: 0.08)) { flash = true }
                withAnimation(.easeIn(duration: 0.3).delay(0.08)) { flash = false }
                onCapture()
            } label: {
                ZStack {
                    Circle().strokeBorder(.white, lineWidth: 4).frame(width: 62, height: 62)
                    Circle().fill(.white).frame(width: 50, height: 50)
                }
                .shadow(color: .black.opacity(0.25), radius: 6, y: 2)
            }
            .buttonStyle(.plain)
            .padding(.bottom, 12)
            .disabled(camera.state != .running)
            .opacity(camera.state == .running ? 1 : 0.35)
            .accessibilityLabel("Take photo")
        }
        .frame(height: 300)
        .clipShape(.rect(cornerRadius: 28, style: .continuous))
        .task { await camera.start() }
        .onDisappear { camera.stop() }
    }
}
#endif
