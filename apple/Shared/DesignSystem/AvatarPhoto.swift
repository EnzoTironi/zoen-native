import SwiftUI
import UniformTypeIdentifiers
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// Local avatar photos keyed by persona id. Until the core stores photos, this is the
/// source of truth on device (Application Support/Roda/avatars/<id>.jpg).
enum AvatarPhotoStore {
    private static var dir: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
            .appendingPathComponent("Roda/avatars", isDirectory: true)
        try? FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        return base
    }

    static func url(for personaId: String) -> URL {
        dir.appendingPathComponent("\(personaId.replacingOccurrences(of: "/", with: "_")).jpg")
    }

    static func hasPhoto(_ personaId: String) -> Bool {
        FileManager.default.fileExists(atPath: url(for: personaId).path)
    }

    static func load(_ personaId: String) -> PlatformImage? {
        let u = url(for: personaId)
        guard let data = try? Data(contentsOf: u) else { return nil }
        #if canImport(UIKit)
        return UIImage(data: data)
        #else
        return NSImage(data: data)
        #endif
    }

    static func save(_ personaId: String, image: PlatformImage) {
        #if canImport(UIKit)
        guard let data = image.jpegData(compressionQuality: 0.86) else { return }
        #else
        guard let tiff = image.tiffRepresentation,
              let rep = NSBitmapImageRep(data: tiff),
              let data = rep.representation(using: .jpeg, properties: [.compressionFactor: 0.86]) else { return }
        #endif
        try? data.write(to: url(for: personaId), options: .atomic)
        NotificationCenter.default.post(name: .avatarPhotoDidChange, object: personaId)
    }

    static func clear(_ personaId: String) {
        try? FileManager.default.removeItem(at: url(for: personaId))
        NotificationCenter.default.post(name: .avatarPhotoDidChange, object: personaId)
    }

    /// Circle-crop to a square JPEG-friendly bitmap (longest side ≤ 512).
    static func croppedCircle(_ image: PlatformImage) -> PlatformImage {
        #if canImport(UIKit)
        let side = min(image.size.width, image.size.height)
        let scale = min(1, 512 / side)
        let out = side * scale
        let renderer = UIGraphicsImageRenderer(size: CGSize(width: out, height: out))
        return renderer.image { _ in
            let rect = CGRect(x: (image.size.width - side) / 2, y: (image.size.height - side) / 2, width: side, height: side)
            UIBezierPath(ovalIn: CGRect(origin: .zero, size: CGSize(width: out, height: out))).addClip()
            image.draw(in: CGRect(x: -rect.origin.x * scale, y: -rect.origin.y * scale,
                                  width: image.size.width * scale, height: image.size.height * scale))
        }
        #else
        return image
        #endif
    }
}

extension Notification.Name {
    static let avatarPhotoDidChange = Notification.Name("zoen.avatarPhotoDidChange")
}

