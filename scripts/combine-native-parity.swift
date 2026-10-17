import AppKit

// swift scripts/combine-native-parity.swift android.png ios.png output.png
// Preserve each full device frame and aspect ratio. Android is always on the left.
let args = CommandLine.arguments
if args.count != 4 { fatalError("Expected Android PNG, iOS PNG, output PNG") }
guard let android = NSImage(contentsOfFile: args[1]), let ios = NSImage(contentsOfFile: args[2]) else {
    fatalError("Both screenshots must be readable images")
}
let height: CGFloat = 1200
let labelHeight: CGFloat = 48
let gap: CGFloat = 24
let widths = [android, ios].map { height * $0.size.width / $0.size.height }
let size = NSSize(width: widths[0] + widths[1] + gap * 3, height: height + labelHeight + gap * 2)
guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(size.width.rounded(.up)), pixelsHigh: Int(size.height), bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
      let context = NSGraphicsContext(bitmapImageRep: bitmap) else { fatalError("Could not allocate combined image") }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
NSColor.white.setFill()
NSBezierPath(rect: NSRect(origin: .zero, size: size)).fill()
context.imageInterpolation = .high
var x = gap
for (index, entry) in [(android, "Android · Material 3"), (ios, "iOS · Liquid Glass")].enumerated() {
    entry.0.draw(in: NSRect(x: x, y: gap, width: widths[index], height: height))
    (entry.1 as NSString).draw(at: NSPoint(x: x, y: height + gap + 14), withAttributes: [
        .font: NSFont.systemFont(ofSize: 22, weight: .semibold), .foregroundColor: NSColor.black,
    ])
    x += widths[index] + gap
}
NSGraphicsContext.restoreGraphicsState()
guard let png = bitmap.representation(using: .png, properties: [:]) else { fatalError("PNG encoding failed") }
try png.write(to: URL(fileURLWithPath: args[3]), options: .atomic)
print(args[3])
