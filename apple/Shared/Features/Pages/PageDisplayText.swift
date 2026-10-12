import Foundation

/// Displays one typed block without turning its legacy line breaks into new blocks.
/// The source remains authoritative until the person actually changes the text.
struct PageDisplayText {
    let source: String
    let text: String
    private let sourceBoundaries: [Int]
    private let displayBoundaries: [Int]

    init(_ source: String) {
        self.source = source
        let units = Array(source.utf16)
        var rendered: [UInt16] = []
        var sourceOffsets = Array(repeating: -1, count: units.count + 1)
        var displayOffsets = [0]
        var index = 0
        sourceOffsets[0] = 0
        while index < units.count {
            let unit = units[index]
            let isBreak = [UInt16(10), 13, 0x85, 0x2028, 0x2029].contains(unit)
            let width: Int
            if unit == 13, index + 1 < units.count, units[index + 1] == 10 {
                width = 2
            } else if (0xD800...0xDBFF).contains(unit), index + 1 < units.count,
                      (0xDC00...0xDFFF).contains(units[index + 1]) {
                width = 2
            } else {
                width = 1
            }
            sourceOffsets[index] = rendered.count
            if isBreak {
                rendered.append(0x2028)
                displayOffsets.append(-1)
            } else {
                rendered.append(contentsOf: units[index..<index + width])
                displayOffsets.append(contentsOf: repeatElement(-1, count: width))
            }
            index += width
            sourceOffsets[index] = rendered.count
            displayOffsets[rendered.count] = index
        }
        text = String(decoding: rendered, as: UTF16.self)
        sourceBoundaries = sourceOffsets
        displayBoundaries = displayOffsets
    }

    /// Byte equality matters: Swift's String equality also equates NFC and NFD text.
    func sourceText(afterEditing displayed: String) -> String {
        displayed.utf8.elementsEqual(text.utf8) ? source : displayed
    }

    /// A range splitting CRLF or a surrogate pair cannot be projected losslessly.
    func displayRange(forSource range: NSRange) -> NSRange? {
        Self.map(range, through: sourceBoundaries)
    }

    func sourceRange(forDisplay range: NSRange) -> NSRange? {
        Self.map(range, through: displayBoundaries)
    }

    private static func map(_ range: NSRange, through boundaries: [Int]) -> NSRange? {
        let length = boundaries.count - 1
        guard range.location >= 0, range.length >= 0, range.location <= length,
              range.length <= length - range.location else { return nil }
        let start = boundaries[range.location]
        let end = boundaries[range.location + range.length]
        guard start >= 0, end >= start else { return nil }
        return NSRange(location: start, length: end - start)
    }
}
