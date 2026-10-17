#if DEBUG
import SwiftUI
#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// Debug extraction of the production Ink paths, never a screenshot replacement.
@MainActor enum NativeWebArtExporter {
    private static var exported = false

    static func exportOnce(appliedFixture: String) throws {
        guard !exported else { return }
        let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("native-web-art", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let marker = root.appendingPathComponent("manifest.json")
        if FileManager.default.fileExists(atPath: marker.path) { try FileManager.default.removeItem(at: marker) }
        let time = 2.4
        let names = MascotPose.allCases.map(\.rawValue) + ["head", "head_smirk", "head_working"]
        var files: [[String: Any]] = []
        for name in names {
            let head = name.hasPrefix("head")
            let dimension: CGFloat = head ? 128 : 384
            let strokes: [InkStroke]
            if name == "head_working" { strokes = Mascot.workingHead(t: time) }
            else if head { strokes = Mascot.head(t: time, mood: name == "head_smirk" ? .smirk : .pout) }
            else { strokes = Mascot.strokes(MascotPose(rawValue: name)!, t: time) }
            let drawing = Canvas { context, size in
                Ink.render(strokes, ctx: context, size: size, seed: head ? 78 : 77,
                           frame: Int(time * 12) % 4, progress: 2, jitter: head ? 0.5 : 0.6)
            }.frame(width: dimension, height: dimension)
            let renderer = ImageRenderer(content: drawing)
            renderer.scale = 3
            renderer.isOpaque = false
            let data: Data?
            #if canImport(UIKit)
            data = renderer.uiImage?.pngData()
            #else
            data = renderer.nsImage?.tiffRepresentation.flatMap { NSBitmapImageRep(data: $0) }?
                .representation(using: .png, properties: [:])
            #endif
            guard let data else { throw ExportError.missingRender(name) }
            let filename = name + ".png"
            try data.write(to: root.appendingPathComponent(filename), options: .atomic)
            files.append(["file": filename, "points": Int(dimension), "pixels": Int(dimension * 3),
                          "scale": 3, "seed": head ? 78 : 77, "jitter": head ? 0.5 : 0.6])
        }
        let manifest: [String: Any] = ["phase_seconds": time, "ink_frame": Int(time * 12) % 4,
            "progress": 2, "transparent": true, "renderer_source": [
                "apple/Shared/DesignSystem/HandDrawn.swift", "apple/Shared/DesignSystem/Mascot.swift"],
            "applied_fixture": appliedFixture, "files": files, "complete": true]
        try JSONSerialization.data(withJSONObject: manifest, options: [.prettyPrinted, .sortedKeys])
            .write(to: marker, options: .atomic)
        exported = true
    }

    private enum ExportError: Error { case missingRender(String) }
}
#endif
