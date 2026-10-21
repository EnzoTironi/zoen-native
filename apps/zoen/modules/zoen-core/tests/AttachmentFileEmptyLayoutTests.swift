import Foundation
import CoreGraphics

/// Compiled with the exact production ZoenAttachmentFileEmptyLayout block.
@main struct AttachmentFileEmptyLayoutTests {
  static func main() {
    // Installed runtime10: the full controller remains 810pt high while its
    // collapsed offset contributes218pt only to the visible footer boundary.
    let captured = ZoenAttachmentFileEmptyLayout.measure(size: CGSize(width: 402, height: 810), textSize: CGSize(width: 362, height: 36), navigationHeight: 68, actionRows: 2, actionHeight: 51, bottomInset: 308, additionalBottomInset: 218, left: 0, right: 0)
    precondition(captured.showsImage && captured.showsText)
    precondition(captured.image.minY == 268, "The source intrinsic inset was replaced by the collapsed inset")
    precondition(captured.image.minY >= 206 && captured.text.maxY <= 502)
    var checked = 0
    for size in [CGSize(width: 320, height: 480), CGSize(width: 375, height: 620), CGSize(width: 402, height: 810), CGSize(width: 430, height: 870), CGSize(width: 844, height: 390), CGSize(width: 874, height: 402)] {
      for rows in [1, 2] {
        for textHeight: CGFloat in [18, 36, 54] {
          for additional: CGFloat in [0, floor(size.height * 0.2488)] {
            for intrinsic: CGFloat in [90, 140, 340] {
              let bottom = additional + intrinsic
              let layout = ZoenAttachmentFileEmptyLayout.measure(size: size, textSize: CGSize(width: max(0, size.width - 80), height: textHeight), navigationHeight: 68, actionRows: rows, actionHeight: 51, bottomInset: bottom, additionalBottomInset: additional, left: 20, right: 20)
              let actionsEnd = 68 + 24 + CGFloat(rows) * 51
              let footerTop = max(0, size.height - bottom)
              if layout.showsText {
                precondition(layout.text.minY >= actionsEnd + 12)
                precondition(layout.text.maxY <= footerTop)
                precondition(layout.text.minX >= 20 && layout.text.maxX <= size.width - 20)
              } else { precondition(!layout.showsImage) }
              if layout.showsImage {
                precondition(size.width < size.height)
                precondition(layout.image.minY >= actionsEnd + 12)
                precondition(layout.image.maxY + 24 <= layout.text.minY)
                precondition(layout.image.width == (size.width == 320 ? 112 : 144))
              }
              checked += 1
            }
          }
        }
      }
    }
    print("Empty-state geometry PASS: runtime10 source anchor + \(checked) actual production layouts; File/Audio, small portrait, landscape, collapsed viewport and keyboard/footer boundaries.")
  }
}
