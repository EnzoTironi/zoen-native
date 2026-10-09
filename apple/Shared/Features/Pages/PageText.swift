import SwiftUI
import RodaCore

#if canImport(UIKit)
import UIKit
typealias PlatformFont = UIFont
#else
import AppKit
typealias PlatformFont = NSFont
#endif

// A page in the editor is one attributed string: one paragraph per block. Each paragraph
// carries a `BlockTag` (which block it is and what kind) and inline marks as `zoen.*`
// attributes; fonts and colours are derived from those, never read back. List items,
// checkboxes, quotes, dividers and images start with one attachment character (the
// marker), which is not part of the block's text.

extension NSAttributedString.Key {
    static let zoenBlock = NSAttributedString.Key("zoen.block")
    static let zoenBold = NSAttributedString.Key("zoen.b")
    static let zoenItalic = NSAttributedString.Key("zoen.i")
    static let zoenStrike = NSAttributedString.Key("zoen.s")
    static let zoenCode = NSAttributedString.Key("zoen.c")
    static let zoenLink = NSAttributedString.Key("zoen.a")
    static let zoenHardBreak = NSAttributedString.Key("zoen.hb")
    static let zoenMarker = NSAttributedString.Key("zoen.marker")

    static let markKeys: [(NSAttributedString.Key, String)] = [
        (.zoenBold, "b"), (.zoenItalic, "i"), (.zoenStrike, "s"), (.zoenCode, "c"), (.zoenLink, "a"), (.zoenHardBreak, "hb"),
    ]
}

/// Which block a paragraph is. Immutable: a change makes a new tag.
final class BlockTag: NSObject, @unchecked Sendable {
    let id: String
    let kind: String
    let level: UInt8
    let indent: UInt8
    let number: UInt32
    let checked: Bool
    let lang: String
    let url: String
    let alt: String

    init(id: String = BlockTag.newId(), kind: String, level: UInt8 = 0, indent: UInt8 = 0, number: UInt32 = 0,
         checked: Bool = false, lang: String = "", url: String = "", alt: String = "") {
        self.id = id; self.kind = kind; self.level = level; self.indent = indent; self.number = number
        self.checked = checked; self.lang = lang; self.url = url; self.alt = alt
    }

    convenience init(_ b: PageBlockDto) {
        self.init(id: b.id, kind: b.kind, level: b.level, indent: b.indent, number: b.number,
                  checked: b.checked, lang: b.lang, url: b.url, alt: b.alt)
    }

    static func newId() -> String {
        String(format: "%016llx", UInt64.random(in: 1...UInt64.max))
    }

    func with(id: String? = nil, kind: String? = nil, level: UInt8? = nil, indent: UInt8? = nil,
              number: UInt32? = nil, checked: Bool? = nil) -> BlockTag {
        BlockTag(id: id ?? self.id, kind: kind ?? self.kind, level: level ?? self.level,
                 indent: indent ?? self.indent, number: number ?? self.number, checked: checked ?? self.checked,
                 lang: lang, url: url, alt: alt)
    }

    /// Kinds whose paragraph starts with a marker character.
    var hasMarker: Bool { ["bullet", "numbered", "task", "quote", "divider", "image"].contains(kind) }
    var isListItem: Bool { ["bullet", "numbered", "task"].contains(kind) }
    /// Blocks that are only their marker (no text to type).
    var isAtomic: Bool { kind == "divider" || kind == "image" }

    override func isEqual(_ object: Any?) -> Bool {
        guard let o = object as? BlockTag else { return false }
        return id == o.id && kind == o.kind && level == o.level && indent == o.indent && number == o.number
            && checked == o.checked && lang == o.lang && url == o.url && alt == o.alt
    }
    override var hash: Int { id.hashValue }
}

/// The kinds a person can pick (slash menu, format bar).
struct BlockChoice: Identifiable, Hashable {
    let id: String
    let title: LocalizedStringResource
    let symbol: String
    let kind: String
    var level: UInt8 = 0

    static let all: [BlockChoice] = [
        .init(id: "p", title: "Text", symbol: "text.alignleft", kind: "paragraph"),
        .init(id: "h1", title: "Title", symbol: "textformat.size.larger", kind: "heading", level: 1),
        .init(id: "h2", title: "Heading", symbol: "textformat.size", kind: "heading", level: 2),
        .init(id: "h3", title: "Subheading", symbol: "textformat.size.smaller", kind: "heading", level: 3),
        .init(id: "task", title: "Checklist", symbol: "checklist", kind: "task"),
        .init(id: "bullet", title: "Bulleted list", symbol: "list.bullet", kind: "bullet"),
        .init(id: "numbered", title: "Numbered list", symbol: "list.number", kind: "numbered"),
        .init(id: "quote", title: "Quote", symbol: "text.quote", kind: "quote"),
        .init(id: "code", title: "Code", symbol: "chevron.left.forwardslash.chevron.right", kind: "code"),
        .init(id: "divider", title: "Divider", symbol: "minus", kind: "divider"),
    ]

    var searchKeys: String { "\(String(localized: title)) \(kind) \(id)".lowercased() }
}

// MARK: - Styling

enum PageStyle {
    static let body: CGFloat = 17

    static func font(_ tag: BlockTag, bold: Bool, italic: Bool, code: Bool) -> PlatformFont {
        if tag.kind == "code" || tag.kind == "raw" || code {
            let size: CGFloat = tag.kind == "raw" ? 14 : 15
            return PlatformFont.monospacedSystemFont(ofSize: size, weight: bold ? .semibold : .regular)
        }
        var size = body
        var weight: PlatformFont.Weight = bold ? .semibold : .regular
        if tag.kind == "heading" {
            switch tag.level {
            case 1: size = 30; weight = .bold
            case 2: size = 23; weight = .bold
            case 3: size = 19; weight = .semibold
            default: size = 17; weight = .semibold
            }
        }
        let base = PlatformFont.systemFont(ofSize: size, weight: weight)
        let wantItalic = italic || tag.kind == "quote"
        guard wantItalic else { return base }
        #if canImport(UIKit)
        if let d = base.fontDescriptor.withSymbolicTraits(base.fontDescriptor.symbolicTraits.union(.traitItalic)) {
            return UIFont(descriptor: d, size: size)
        }
        return base
        #else
        let d = base.fontDescriptor.withSymbolicTraits(base.fontDescriptor.symbolicTraits.union(.italic))
        return NSFont(descriptor: d, size: size) ?? base
        #endif
    }

    static func paragraph(_ tag: BlockTag) -> NSParagraphStyle {
        let p = NSMutableParagraphStyle()
        p.lineSpacing = 3
        p.paragraphSpacing = tag.isListItem ? 5 : 10
        let indent = CGFloat(tag.indent) * 24
        switch tag.kind {
        case "heading":
            p.paragraphSpacingBefore = tag.level <= 2 ? 14 : 8
            p.paragraphSpacing = 6
        case "bullet", "numbered", "task":
            p.firstLineHeadIndent = indent
            p.headIndent = indent + 30
        case "quote":
            p.firstLineHeadIndent = 0
            p.headIndent = 18
        case "code", "raw":
            p.firstLineHeadIndent = 12
            p.headIndent = 12
            p.tailIndent = -12
            p.lineSpacing = 2
        default:
            break
        }
        return p
    }

    static func color(_ tag: BlockTag) -> PlatformColor {
        switch tag.kind {
        case "quote", "raw": PlatformColor(Palette.textSecondary)
        case "task" where tag.checked: PlatformColor(Palette.textTertiary)
        default: PlatformColor(Palette.textPrimary)
        }
    }

    /// The attributes for one run of a block's text.
    static func attributes(_ tag: BlockTag, marks: [NSAttributedString.Key: Any]) -> [NSAttributedString.Key: Any] {
        var a = marks
        a[.zoenBlock] = tag
        let code = marks[.zoenCode] != nil
        a[.font] = font(tag, bold: marks[.zoenBold] != nil, italic: marks[.zoenItalic] != nil, code: code)
        a[.paragraphStyle] = paragraph(tag)
        a[.foregroundColor] = color(tag)
        if marks[.zoenStrike] != nil || (tag.kind == "task" && tag.checked) {
            a[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
        } else {
            a[.strikethroughStyle] = nil
        }
        if let url = marks[.zoenLink] as? String {
            a[.foregroundColor] = PlatformColor(Palette.action)
            a[.underlineStyle] = NSUnderlineStyle.single.rawValue
            if let u = URL(string: url) { a[.link] = u }
        } else {
            a[.underlineStyle] = nil
            a[.link] = nil
        }
        if code || tag.kind == "code" {
            a[.backgroundColor] = PlatformColor(Palette.surfaceMuted)
        } else {
            a[.backgroundColor] = nil
        }
        return a
    }
}

// MARK: - Markers

enum PageMarker {
    static let char = "\u{FFFC}"

    @MainActor
    static func attachment(_ tag: BlockTag, number: UInt32) -> NSTextAttachment {
        let a = NSTextAttachment()
        switch tag.kind {
        case "task":
            a.image = symbol(tag.checked ? "checkmark.circle.fill" : "circle",
                             color: tag.checked ? PlatformColor(Palette.action) : PlatformColor(Palette.textTertiary),
                             size: 20)
            a.bounds = CGRect(x: 0, y: -4, width: 22, height: 22)
        case "bullet":
            a.image = symbol(tag.indent % 2 == 0 ? "circle.fill" : "circle", color: PlatformColor(Palette.textSecondary), size: 7)
            a.bounds = CGRect(x: 6, y: 3, width: 7, height: 7)
        case "numbered":
            a.image = textImage("\(number).", size: CGSize(width: 24, height: 20))
            a.bounds = CGRect(x: 0, y: -4, width: 24, height: 20)
        case "quote":
            a.image = bar(height: 20)
            a.bounds = CGRect(x: 0, y: -4, width: 12, height: 22)
        case "divider":
            a.image = rule(width: 600)
            a.bounds = CGRect(x: 0, y: 4, width: 600, height: 2)
        case "image":
            a.image = symbol("photo", color: PlatformColor(Palette.textTertiary), size: 28)
            a.bounds = CGRect(x: 0, y: -6, width: 34, height: 30)
        default:
            break
        }
        return a
    }

    @MainActor
    static func symbol(_ name: String, color: PlatformColor, size: CGFloat) -> PlatformImage? {
        #if canImport(UIKit)
        let cfg = UIImage.SymbolConfiguration(pointSize: size, weight: .medium)
        return UIImage(systemName: name, withConfiguration: cfg)?.withTintColor(color, renderingMode: .alwaysOriginal)
        #else
        let cfg = NSImage.SymbolConfiguration(pointSize: size, weight: .medium).applying(.init(paletteColors: [color]))
        return NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(cfg)
        #endif
    }

    @MainActor
    static func draw(_ size: CGSize, _ body: @escaping (CGContext) -> Void) -> PlatformImage {
        #if canImport(UIKit)
        return UIGraphicsImageRenderer(size: size).image { body($0.cgContext) }
        #else
        return NSImage(size: size, flipped: true) { _ in
            if let ctx = NSGraphicsContext.current?.cgContext { body(ctx) }
            return true
        }
        #endif
    }

    @MainActor
    static func textImage(_ s: String, size: CGSize) -> PlatformImage {
        let str = NSAttributedString(string: s, attributes: [
            .font: PlatformFont.monospacedDigitSystemFont(ofSize: 16, weight: .medium),
            .foregroundColor: PlatformColor(Palette.textSecondary),
        ])
        return draw(size) { _ in str.draw(at: CGPoint(x: 0, y: 0)) }
    }

    @MainActor
    static func bar(height: CGFloat) -> PlatformImage {
        let color = PlatformColor(Palette.action).cgColor
        return draw(CGSize(width: 12, height: height + 2)) { ctx in
            ctx.setFillColor(color)
            ctx.addPath(CGPath(roundedRect: CGRect(x: 1, y: 0, width: 3.5, height: height + 2), cornerWidth: 1.75, cornerHeight: 1.75, transform: nil))
            ctx.fillPath()
        }
    }

    @MainActor
    static func rule(width: CGFloat) -> PlatformImage {
        let color = PlatformColor(Palette.hairline).cgColor
        return draw(CGSize(width: width, height: 2)) { ctx in
            ctx.setFillColor(color)
            ctx.fill(CGRect(x: 0, y: 0, width: width, height: 2))
        }
    }
}

// MARK: - Building and reading back

@MainActor
enum PageText {
    /// The attributed string for a page's blocks.
    static func build(_ blocks: [PageBlockDto]) -> NSMutableAttributedString {
        let out = NSMutableAttributedString()
        var numbers = NumberCounter()
        for (i, b) in blocks.enumerated() {
            let tag = BlockTag(b)
            out.append(paragraph(tag, text: b.text, spans: b.spans, number: numbers.next(tag)))
            if i < blocks.count - 1 {
                out.append(NSAttributedString(string: "\n", attributes: PageStyle.attributes(tag, marks: [:])))
            }
        }
        if blocks.isEmpty {
            let tag = BlockTag(kind: "heading", level: 1)
            out.append(NSAttributedString(string: "", attributes: PageStyle.attributes(tag, marks: [:])))
        }
        return out
    }

    static func paragraph(_ tag: BlockTag, text: String, spans: [TextSpanDto], number: UInt32) -> NSAttributedString {
        let p = NSMutableAttributedString()
        if tag.hasMarker {
            p.append(marker(tag, number: number))
        }
        if !tag.isAtomic {
            let s = NSMutableAttributedString(string: text, attributes: PageStyle.attributes(tag, marks: [:]))
            let len = (text as NSString).length
            for sp in spans {
                let start = Int(sp.start), end = min(Int(sp.end), len)
                guard start < end, let key = NSAttributedString.Key.markKeys.first(where: { $0.1 == sp.key })?.0 else { continue }
                s.addAttribute(key, value: key == .zoenLink ? sp.value : true, range: NSRange(location: start, length: end - start))
            }
            restyle(s, tag: tag, in: NSRange(location: 0, length: s.length))
            p.append(s)
        }
        return p
    }

    static func marker(_ tag: BlockTag, number: UInt32) -> NSAttributedString {
        let m = NSMutableAttributedString(attachment: PageMarker.attachment(tag, number: number))
        var attrs = PageStyle.attributes(tag, marks: [:])
        attrs[.zoenMarker] = number
        m.addAttributes(attrs, range: NSRange(location: 0, length: m.length))
        return m
    }

    /// Re-derives fonts and colours from the `zoen.*` marks in `range` (paragraph text only).
    static func restyle(_ s: NSMutableAttributedString, tag: BlockTag, in range: NSRange) {
        guard range.length > 0 else { return }
        s.enumerateAttributes(in: range) { attrs, r, _ in
            if attrs[.attachment] != nil { return }
            var marks: [NSAttributedString.Key: Any] = [:]
            for (k, _) in NSAttributedString.Key.markKeys where attrs[k] != nil { marks[k] = attrs[k] }
            s.setAttributes(PageStyle.attributes(tag, marks: marks), range: r)
        }
    }

    /// One paragraph read back: its tag, text (without the marker) and marks.
    static func read(_ s: NSAttributedString, paragraph r: NSRange, tag: BlockTag) -> PageBlockDto {
        let ns = s.string as NSString
        var start = r.location
        var end = r.location + r.length
        if end > start, ns.character(at: end - 1) == 10 { end -= 1 }
        if tag.hasMarker, start < end, ns.substring(with: NSRange(location: start, length: 1)) == PageMarker.char {
            start += 1
        }
        let textRange = NSRange(location: start, length: max(0, end - start))
        let text = tag.isAtomic ? "" : ns.substring(with: textRange)
        var spans: [TextSpanDto] = []
        if !tag.isAtomic, textRange.length > 0 {
            for (key, name) in NSAttributedString.Key.markKeys {
                s.enumerateAttribute(key, in: textRange) { v, sub, _ in
                    guard let v else { return }
                    let value = (v as? String) ?? ""
                    let a = UInt32(sub.location - start), b = UInt32(sub.location + sub.length - start)
                    if let last = spans.last, last.key == name, last.end == a, last.value == value {
                        spans[spans.count - 1] = TextSpanDto(start: last.start, end: b, key: name, value: value)
                    } else {
                        spans.append(TextSpanDto(start: a, end: b, key: name, value: value))
                    }
                }
            }
        }
        return PageBlockDto(id: tag.id, kind: tag.kind, level: tag.level, indent: tag.indent, number: tag.number,
                            checked: tag.checked, lang: tag.lang, url: tag.url, alt: tag.alt, text: text, spans: spans)
    }

    /// Every paragraph range of `s` (including its trailing newline).
    static func paragraphs(_ s: NSAttributedString) -> [NSRange] {
        let ns = s.string as NSString
        var out: [NSRange] = []
        var loc = 0
        while loc < ns.length {
            let r = ns.paragraphRange(for: NSRange(location: loc, length: 0))
            out.append(r)
            loc = r.location + r.length
        }
        if ns.length == 0 || ns.character(at: ns.length - 1) == 10 {
            out.append(NSRange(location: ns.length, length: 0))
        }
        return out
    }
}

/// Numbers consecutive numbered items like Markdown does (start + position in the run).
struct NumberCounter {
    private var current: [UInt8: UInt32] = [:]
    mutating func next(_ tag: BlockTag) -> UInt32 {
        guard tag.kind == "numbered" else {
            if !tag.isListItem { current.removeAll() }
            return 0
        }
        current = current.filter { $0.key <= tag.indent }
        let n = current[tag.indent].map { $0 + 1 } ?? max(tag.number, 1)
        current[tag.indent] = n
        return n
    }
}
