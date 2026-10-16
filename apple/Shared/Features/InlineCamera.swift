import SwiftUI
#if os(iOS)
import AVFoundation
import UIKit

/// Camera authorization and UI state stay on the main actor. Capture uses one serial queue.
@MainActor @Observable
final class InlineCamera {
    enum State: Equatable { case starting, running, unavailable, denied }
    var state: State = .starting
    private(set) var capturing = false
    private let captureSession = PhotoCaptureSession()
    private var active = false
    var session: AVCaptureSession { captureSession.session }

    func start() async {
        active = true
        var allowed = AVCaptureDevice.authorizationStatus(for: .video) == .authorized
        if AVCaptureDevice.authorizationStatus(for: .video) == .notDetermined {
            allowed = await AVCaptureDevice.requestAccess(for: .video)
        }
        guard active, !Task.isCancelled else { return }
        guard allowed else { state = .denied; return }
        let running = await captureSession.start()
        guard active, !Task.isCancelled else { captureSession.stop(); return }
        state = running ? .running : .unavailable
    }

    func capture() async throws -> Data {
        guard active, state == .running, !capturing else { throw PhotoCaptureSession.Failure.unavailable }
        capturing = true
        defer { capturing = false }
        let bytes = try await captureSession.capture()
        guard active, !Task.isCancelled else { throw CancellationError() }
        return bytes
    }

    func stop() {
        active = false
        captureSession.stop()
    }
}

/// AVFoundation calls every delegate callback on its own queue. All mutable state below
/// is confined to `queue`; the preview layer only receives the session reference.
private final class PhotoCaptureSession: NSObject, AVCapturePhotoCaptureDelegate, @unchecked Sendable {
    enum Failure: LocalizedError {
        case unavailable, empty
        var errorDescription: String? {
            switch self {
            case .unavailable: String(localized: "Camera is unavailable on this device.")
            case .empty: String(localized: "The camera did not return a photo. Please try again.")
            }
        }
    }

    let session = AVCaptureSession()
    private let output = AVCapturePhotoOutput()
    private let queue = DispatchQueue(label: "xyz.tironi.zoen.photo-capture", qos: .userInitiated)
    private var configured = false
    private var pending: [Int64: CheckedContinuation<Data, any Error>] = [:]

    func start() async -> Bool {
        await withCheckedContinuation { continuation in
            queue.async { [self] in
                if !configured {
                    guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
                          let input = try? AVCaptureDeviceInput(device: device), session.canAddInput(input) else {
                        continuation.resume(returning: false)
                        return
                    }
                    session.beginConfiguration()
                    session.sessionPreset = .photo
                    session.addInput(input)
                    guard session.canAddOutput(output) else {
                        session.removeInput(input)
                        session.commitConfiguration()
                        continuation.resume(returning: false)
                        return
                    }
                    session.addOutput(output)
                    let dimensions = device.activeFormat.supportedMaxPhotoDimensions
                    let bounded = dimensions.filter { Int64($0.width) * Int64($0.height) <= 12_000_000 }
                    if let chosen = (bounded.isEmpty ? dimensions : bounded).max(by: {
                        Int64($0.width) * Int64($0.height) < Int64($1.width) * Int64($1.height)
                    }) { output.maxPhotoDimensions = chosen }
                    session.commitConfiguration()
                    configured = true
                }
                session.startRunning()
                continuation.resume(returning: session.isRunning)
            }
        }
    }

    func stop() {
        queue.async { [self] in
            session.stopRunning()
            let interrupted = pending.values
            pending.removeAll()
            for continuation in interrupted { continuation.resume(throwing: CancellationError()) }
        }
    }

    func capture() async throws -> Data {
        try await withCheckedThrowingContinuation { continuation in
            queue.async { [self] in
                guard session.isRunning, pending.isEmpty else {
                    continuation.resume(throwing: Failure.unavailable)
                    return
                }
                let settings = AVCapturePhotoSettings(format: [AVVideoCodecKey: AVVideoCodecType.jpeg])
                settings.photoQualityPrioritization = .balanced
                settings.maxPhotoDimensions = output.maxPhotoDimensions
                if let connection = output.connection(with: .video), connection.isVideoRotationAngleSupported(90) {
                    connection.videoRotationAngle = 90
                }
                pending[settings.uniqueID] = continuation
                output.capturePhoto(with: settings, delegate: self)
            }
        }
    }

    func photoOutput(_ output: AVCapturePhotoOutput, didFinishProcessingPhoto photo: AVCapturePhoto, error: (any Error)?) {
        let id = photo.resolvedSettings.uniqueID
        let bytes = photo.fileDataRepresentation()
        queue.async { [self] in
            guard let continuation = pending.removeValue(forKey: id) else { return }
            if let error { continuation.resume(throwing: error) }
            else if let bytes, !bytes.isEmpty, bytes.count <= AttachmentFiles.maximumBytes {
                continuation.resume(returning: bytes)
            } else { continuation.resume(throwing: Failure.empty) }
        }
    }

    func photoOutput(_ output: AVCapturePhotoOutput, didFinishCaptureFor resolvedSettings: AVCaptureResolvedPhotoSettings,
                     error: (any Error)?) {
        guard let error else { return }
        let id = resolvedSettings.uniqueID
        queue.async { [self] in pending.removeValue(forKey: id)?.resume(throwing: error) }
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
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var onClose: () -> Void
    var onCapture: (Data) -> Void
    @State private var camera = InlineCamera()
    @State private var flash = false
    @State private var failure: String?

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
                    Text(camera.state == .denied ? String(localized: "Camera access is off. You can turn it on in Settings.") : String(localized: "Camera is unavailable on this device."))
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
                Image(systemName: "xmark").font(.footnote.weight(.bold)).frame(width: 44, height: 44)
            }
            .buttonStyle(.plain)
            .foregroundStyle(camera.state == .running || camera.state == .starting ? .white : InkPalette.ink)
            .zoenGlass(radius: 999, slab: true)
            .padding(10)
            .accessibilityLabel("Close camera")
        }
        .overlay(alignment: .bottom) {
            Button {
                if !reduceMotion {
                    withAnimation(.easeOut(duration: 0.08)) { flash = true }
                    withAnimation(.easeIn(duration: 0.3).delay(0.08)) { flash = false }
                }
                Task {
                    do {
                        let bytes = try await camera.capture()
                        Haptics.commit()
                        onCapture(bytes)
                    } catch is CancellationError {
                    } catch { failure = error.localizedDescription }
                }
            } label: {
                ZStack {
                    Circle().strokeBorder(.white, lineWidth: 4).frame(width: 62, height: 62)
                    Circle().fill(.white).frame(width: 50, height: 50)
                }
                .shadow(color: .black.opacity(0.25), radius: 6, y: 2)
            }
            .buttonStyle(.plain)
            .padding(.bottom, 12)
            .disabled(camera.state != .running || camera.capturing)
            .opacity(camera.state == .running && !camera.capturing ? 1 : 0.35)
            .accessibilityLabel("Take photo")
            .accessibilityIdentifier("chat.camera.capture")
        }
        .frame(height: 300)
        .clipShape(.rect(cornerRadius: 28, style: .continuous))
        .task { await camera.start() }
        .onDisappear { camera.stop() }
        .alert("Could not take photo", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) { failure = nil }
        } message: { Text(failure ?? "") }
    }
}
#endif
