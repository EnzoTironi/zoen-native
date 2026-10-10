import SwiftUI

@MainActor
func exportAndroidMascotArt() {
    let root = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0].appendingPathComponent("android-mascot-export")
    try! FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let names = MascotPose.allCases.map { $0.rawValue } + ["head", "head_smirk", "head_working"]
    for name in names {
        let folder = root.appendingPathComponent(name)
        try! FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let head = name.hasPrefix("head")
        let dimension: CGFloat = head ? 128 : 384
        for index in 0..<96 {
            autoreleasepool {
                let time = 2.3 + Double(index) / 12
                let strokes: [InkStroke]
                if name == "head_working" { strokes = Mascot.workingHead(t: time) }
                else if head { strokes = Mascot.head(t: time, mood: name == "head_smirk" ? .smirk : .pout) }
                else { strokes = Mascot.strokes(MascotPose(rawValue: name)!, t: time) }
                let drawing = Canvas { context, size in
                    Ink.render(strokes, ctx: context, size: size, seed: head ? 78 : 77, frame: Int(time * 12) % 4, progress: 2, jitter: head ? 0.5 : 0.6)
                }.frame(width: dimension, height: dimension)
                let renderer = ImageRenderer(content: drawing)
                renderer.scale = 1
                guard let data = renderer.uiImage?.pngData() else { fatalError("Missing original mascot render") }
                try! data.write(to: folder.appendingPathComponent(String(format: "%03d.png", index)))
            }
        }
    }
    try! "Original SwiftUI Ink renderer, time 2.3 + frame/12, transparent 384px poses and 128px heads, 96 frames.\n".write(to: root.appendingPathComponent("complete.txt"), atomically: true, encoding: .utf8)
}
