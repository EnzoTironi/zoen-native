import AppKit

// Local review aid only. A contact sheet is not a substitute for individual
// application captures or behavioral/parity checks. No media is published.
let arguments = CommandLine.arguments
if arguments.count != 4 {
    fputs("Usage: swift scripts/living-space-contact-sheet.swift <directory> <platform> <theme>\n", stderr)
    exit(2)
}
let directory = URL(fileURLWithPath: arguments[1], isDirectory: true)
let platform = arguments[2]
let theme = arguments[3]
let screens = ["home", "announcements", "page", "notifications", "hackathon", "teams", "submission", "results", "tiers"]
let cell = NSSize(width: 280, height: 590)
let size = NSSize(width: cell.width * 3, height: cell.height * 3)
let sheet = NSImage(size: size)
sheet.lockFocus()
NSColor.white.setFill()
NSRect(origin: .zero, size: size).fill()
for (index, screen) in screens.enumerated() {
    let x = CGFloat(index % 3) * cell.width
    let y = size.height - CGFloat(index / 3 + 1) * cell.height
    let name = "\(platform)-\(theme)-\(screen).png"
    let image = NSImage(contentsOf: directory.appendingPathComponent(name))
    let label = image == nil ? "MISSING: \(screen)" : screen
    (label as NSString).draw(at: NSPoint(x: x + 8, y: y + 568), withAttributes: [
        .font: NSFont.systemFont(ofSize: 14), .foregroundColor: NSColor.black
    ])
    guard let image else { continue }
    let scale = min(264 / image.size.width, 550 / image.size.height)
    let drawn = NSSize(width: image.size.width * scale, height: image.size.height * scale)
    image.draw(in: NSRect(x: x + (280 - drawn.width) / 2, y: y + 8,
                         width: drawn.width, height: drawn.height))
}
sheet.unlockFocus()
guard let tiff = sheet.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else { exit(1) }
try png.write(to: directory.appendingPathComponent("\(platform)-\(theme)-contact-sheet.png"))
