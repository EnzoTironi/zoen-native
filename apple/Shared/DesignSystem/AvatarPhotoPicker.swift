import SwiftUI
import PhotosUI
#if canImport(UIKit)
import UIKit
#endif

/// Circle photo picker for onboarding / profile: library, camera, clear → monogram.
/// Crops to a circle before saving via `AvatarPhotoStore`.
struct AvatarPhotoPicker: View {
    let personaId: String?
    /// Preview while onboarding (account not created yet).
    @Binding var pending: PlatformImage?
    var size: CGFloat = 96
    var initials: String = "?"
    var tintHex: String = "#6B8F71"
    /// "Your photo, or initials" under the circle; off where the picker sits inline.
    var caption = true
    /// The page colour behind the picker: the badge's ring matches it.
    var ringColor: Color = Palette.background

    @State private var item: PhotosPickerItem?
    @State private var showLibrary = false
    @State private var showCamera = false
    @State private var showActions = false
    @State private var photo: PlatformImage?
    @State private var tick = 0

    var body: some View {
        VStack(spacing: 12) {
            Button { showActions = true } label: {
                face
                    .frame(width: size, height: size)
                    .clipShape(.circle)
                    .overlay(Circle().strokeBorder(Mascot.line.opacity(0.2), lineWidth: 2))
                    .shadow(color: .black.opacity(0.08), radius: 8, y: 3)
                    // The badge lives outside the clip, centred on the circle's edge at 45°
                    // (r·cos45 right and down from the centre), so it is never cut or drifts.
                    .overlay {
                        cameraBadge.offset(x: badgeOffset, y: badgeOffset)
                    }
                    // Room for the part of the badge that pokes past the square, on every side
                    // so the circle stays centred wherever the picker sits.
                    .padding(badgeOverhang)
                    .contentShape(.circle)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(String(localized: "Change photo"))
            .confirmationDialog(String(localized: "Profile photo"), isPresented: $showActions, titleVisibility: .visible) {
                Button(String(localized: "Choose photo")) { showLibrary = true }
                #if os(iOS)
                Button(String(localized: "Take photo")) { showCamera = true }
                #endif
                if photo != nil || pending != nil {
                    Button(String(localized: "Remove photo"), role: .destructive) { clear() }
                }
                Button(String(localized: "Cancel"), role: .cancel) {}
            }

            if caption {
                Text(String(localized: "Your photo — or initials if you skip"))
                    .font(.caption)
                    .foregroundStyle(InkPalette.ink.opacity(0.55))
            }
        }
        .task(id: "\(personaId ?? "")-\(tick)") { reload() }
        .photosPicker(isPresented: $showLibrary, selection: $item, matching: .images)
        .onChange(of: item) { _, new in
            guard let new else { return }
            Task { await apply(pickerItem: new) }
        }
        #if os(iOS)
        .fullScreenCover(isPresented: $showCamera) {
            AvatarCameraCapture { img in
                showCamera = false
                if let img { apply(image: img) }
            }
            .ignoresSafeArea()
        }
        #endif
        .onReceive(NotificationCenter.default.publisher(for: .avatarPhotoDidChange)) { note in
            if let id = personaId, note.object as? String == id { tick &+= 1 }
        }
    }

    private var badgeSize: CGFloat { min(32, max(22, size * 0.3)) }
    private var badgeOffset: CGFloat { size / 2 * 0.7071 }
    private var badgeOverhang: CGFloat { max(0, badgeOffset + badgeSize / 2 + ringWidth - size / 2) }
    private let ringWidth: CGFloat = 2.5

    /// Camera glyph centred in a moss-green dot, with a ring in the page colour so it reads
    /// as sitting on top of the photo.
    private var cameraBadge: some View {
        Image(systemName: "camera.fill")
            .font(.system(size: badgeSize * 0.42, weight: .bold))
            .foregroundStyle(.white)
            .frame(width: badgeSize, height: badgeSize)
            .background(Palette.action, in: .circle)
            .padding(ringWidth)
            .background(ringColor, in: .circle)
            .accessibilityHidden(true)
    }

    @ViewBuilder
    private var face: some View {
        if let img = pending ?? photo {
            #if canImport(UIKit)
            Image(uiImage: img).resizable().scaledToFill()
            #else
            Image(nsImage: img).resizable().scaledToFill()
            #endif
        } else {
            Circle()
                .fill(LinearGradient(
                    colors: [Color(hex: tintHex).mix(with: .white, by: 0.18),
                             Color(hex: tintHex).mix(with: .black, by: 0.22)],
                    startPoint: .topLeading, endPoint: .bottomTrailing))
                .overlay(
                    Text(initials)
                        .font(.system(size: size * 0.36, weight: .semibold, design: .rounded))
                        .foregroundStyle(.white)
                )
        }
    }

    private func reload() {
        if let id = personaId { photo = AvatarPhotoStore.load(id) }
    }

    private func apply(image: PlatformImage) {
        let cropped = AvatarPhotoStore.croppedCircle(image)
        pending = cropped
        if let id = personaId {
            AvatarPhotoStore.save(id, image: cropped)
            photo = cropped
        }
        Haptics.settle()
    }

    private func clear() {
        pending = nil
        photo = nil
        if let id = personaId { AvatarPhotoStore.clear(id) }
        item = nil
    }

    #if canImport(UIKit)
    private func apply(pickerItem: PhotosPickerItem) async {
        guard let data = try? await pickerItem.loadTransferable(type: Data.self),
              let img = UIImage(data: data) else { return }
        await MainActor.run { apply(image: img) }
    }
    #else
    private func apply(pickerItem: PhotosPickerItem) async {}
    #endif
}

#if os(iOS)
/// Thin UIImagePicker wrapper for a front-camera selfie used as an avatar.
private struct AvatarCameraCapture: UIViewControllerRepresentable {
    var onFinish: (UIImage?) -> Void

    func makeCoordinator() -> Coord { Coord(onFinish: onFinish) }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let p = UIImagePickerController()
        p.sourceType = UIImagePickerController.isSourceTypeAvailable(.camera) ? .camera : .photoLibrary
        if p.sourceType == .camera { p.cameraDevice = .front }
        p.allowsEditing = true
        p.delegate = context.coordinator
        return p
    }

    func updateUIViewController(_ uiViewController: UIImagePickerController, context: Context) {}

    final class Coord: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let onFinish: (UIImage?) -> Void
        init(onFinish: @escaping (UIImage?) -> Void) { self.onFinish = onFinish }
        func imagePickerController(_ picker: UIImagePickerController,
                                   didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            let img = (info[.editedImage] ?? info[.originalImage]) as? UIImage
            onFinish(img)
        }
        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) { onFinish(nil) }
    }
}
#endif
