import SwiftUI
import RodaCore

#if canImport(UIKit)
import UIKit
#else
import AppKit
#endif

/// What the text view gives the editing logic (UIKit and AppKit differ only here).
@MainActor
protocol PageTextHost: AnyObject {
    var pageStorage: NSTextStorage { get }
    var pageSelection: NSRange { get set }
    var pageTypingAttributes: [NSAttributedString.Key: Any] { get set }
}

/// The editor's state and behaviour, shared by the iPhone and the Mac.
@MainActor
@Observable
final class PageEditorController {
    /// The block kind under the cursor (the format bar highlights it).
    private(set) var currentKind = "paragraph"
    private(set) var currentLevel: UInt32 = 0
    /// Marks active at the cursor or over the whole selection.
    private(set) var activeMarks: Set<String> = []
    /// Text after "/" at the start of an empty block, while the block menu is open.
    private(set) var slashQuery: String?
    private(set) var hasSelection = false
    /// Nothing typed yet (shows the "Title" hint).
    private(set) var isEmpty = false
    var editable = true
    /// Bumped on every change a person can feel (drives haptics in SwiftUI).
    private(set) var tick = 0
    private(set) var strongTick = 0

    @ObservationIgnored weak var host: PageTextHost?
    /// Called (debounced by the screen) whenever the text changed.
    @ObservationIgnored var onEdit: (() -> Void)?
    /// The last paragraph has no character to carry its tag when empty.
    @ObservationIgnored var trailingTag: BlockTag?
    /// Set while we change the storage ourselves (don't treat it as typing).
    @ObservationIgnored private var applying = false

    // MARK: loading and reading

    func load(_ blocks: [PageBlockDto], keepSelection: Bool = false) {
        guard let host else { return }
        let sel = host.pageSelection
        let s = PageText.build(blocks)
        trailingTag = blocks.last.map { BlockTag($0) } ?? BlockTag(kind: "heading", level: 1)
        applying = true
        host.pageStorage.setAttributedString(s)
        applying = false
        let len = host.pageStorage.length
        host.pageSelection = keepSelection
            ? NSRange(location: min(sel.location, len), length: 0)
            : NSRange(location: len, length: 0)
        refreshTyping()
        updateState()
    }

    /// Every block, top to bottom, as the core wants them back.
    func blocks() -> [PageBlockDto] {
        guard let host else { return [] }
        let s = host.pageStorage
        return PageText.paragraphs(s).map { r in PageText.read(s, paragraph: r, tag: tag(at: r)) }
    }

    func tag(at r: NSRange) -> BlockTag {
        guard let host else { return BlockTag(kind: "paragraph") }
        let s = host.pageStorage
        if r.length > 0, let t = s.attribute(.zoenBlock, at: r.location, effectiveRange: nil) as? BlockTag {
            return t
        }
        if r.location == s.length {
            if let t = host.pageTypingAttributes[.zoenBlock] as? BlockTag, host.pageSelection.location == s.length { return t }
            if let t = trailingTag { return t }
        }
        return BlockTag(kind: "paragraph")
    }

    private func paragraphRange(at loc: Int) -> NSRange {
        guard let host else { return NSRange(location: 0, length: 0) }
        let ns = host.pageStorage.string as NSString
        if loc >= ns.length {
            if ns.length == 0 || ns.character(at: ns.length - 1) == 10 {
                return NSRange(location: ns.length, length: 0)
            }
            return ns.paragraphRange(for: NSRange(location: ns.length - 1, length: 0))
        }
        return ns.paragraphRange(for: NSRange(location: loc, length: 0))
    }

    /// The paragraph's text range (after the marker, before the newline).
    private func textRange(_ r: NSRange, tag: BlockTag) -> NSRange {
        guard let host else { return r }
        let ns = host.pageStorage.string as NSString
        var start = r.location, end = r.location + r.length
        if end > start, ns.character(at: end - 1) == 10 { end -= 1 }
        if tag.hasMarker, start < end, ns.substring(with: NSRange(location: start, length: 1)) == PageMarker.char { start += 1 }
        return NSRange(location: start, length: max(0, end - start))
    }

    // MARK: typing

    /// Decides what a keystroke does. Returns false when it was handled here.
    func shouldChange(_ range: NSRange, replacement: String) -> Bool {
        guard let host, editable else { return false }
        let para = paragraphRange(at: range.location)
        let tag = tag(at: para)
        let text = textRange(para, tag: tag)
        let ns = host.pageStorage.string as NSString
        let before = range.location >= text.location
            ? ns.substring(with: NSRange(location: text.location, length: range.location - text.location)) : ""

        if replacement == "\n" {
            return returnKey(range: range, para: para, tag: tag, text: text)
        }
        // Backspace at the start of a styled block: it becomes text first (then merges).
        if replacement.isEmpty, range.length == 1, host.pageSelection.length == 0,
           host.pageSelection.location == text.location, range.location + 1 == text.location {
            if tag.kind != "paragraph" && !tag.isAtomic {
                setKind(BlockChoice.all[0], at: para)
                Haptics.selectionTick()
                return false
            }
        }
        // Markdown as you type: "# ", "- ", "1. ", "[] ", "> ", "```".
        if replacement == " ", range.length == 0, tag.kind == "paragraph", range.location == text.location + (before as NSString).length {
            if let choice = shortcut(before) {
                applying = true
                host.pageStorage.replaceCharacters(in: NSRange(location: text.location, length: (before as NSString).length), with: "")
                applying = false
                host.pageSelection = NSRange(location: text.location, length: 0)
                setKind(choice, at: paragraphRange(at: text.location))
                bump(strong: true)
                return false
            }
        }
        if replacement == "`", tag.kind == "paragraph", before == "``", text.length == 2 {
            applying = true
            host.pageStorage.replaceCharacters(in: text, with: "")
            applying = false
            host.pageSelection = NSRange(location: text.location, length: 0)
            setKind(BlockChoice.all.first { $0.id == "code" }!, at: paragraphRange(at: text.location))
            bump(strong: true)
            return false
        }
        if replacement == "-", tag.kind == "paragraph", before == "--", text.length == 2 {
            applying = true
            host.pageStorage.replaceCharacters(in: text, with: "")
            applying = false
            host.pageSelection = NSRange(location: text.location, length: 0)
            setKind(BlockChoice.all.first { $0.id == "divider" }!, at: paragraphRange(at: text.location))
            bump(strong: true)
            return false
        }
        return true
    }

    private func shortcut(_ prefix: String) -> BlockChoice? {
        let id: String? = switch prefix {
        case "#": "h1"
        case "##": "h2"
        case "###": "h3"
        case "-", "*", "+": "bullet"
        case "1.", "1)": "numbered"
        case "[]", "[ ]", "-[]", "- [ ]": "task"
        case ">": "quote"
        default: nil
        }
        return id.flatMap { i in BlockChoice.all.first { $0.id == i } }
    }

    private func returnKey(range: NSRange, para: NSRange, tag: BlockTag, text: NSRange) -> Bool {
        guard let host else { return true }
        let s = host.pageStorage
        let ns = s.string as NSString
        if tag.kind == "code" {
            // Inside code a return is a new line of code; a return on an empty last line leaves.
            let atEnd = range.location == text.location + text.length
            if atEnd, text.length > 0, ns.character(at: text.location + text.length - 1) == 0x2028 {
                applying = true
                s.replaceCharacters(in: NSRange(location: text.location + text.length - 1, length: 1), with: "")
                applying = false
                host.pageSelection = NSRange(location: text.location + text.length - 1, length: 0)
                insertBlockBreak(at: host.pageSelection.location, newTag: BlockTag(kind: "paragraph"), carry: false)
                return false
            }
            applying = true
            s.replaceCharacters(in: range, with: NSAttributedString(string: "\u{2028}", attributes: PageStyle.attributes(tag, marks: [:])))
            applying = false
            host.pageSelection = NSRange(location: range.location + 1, length: 0)
            edited()
            return false
        }
        if tag.isListItem || tag.kind == "quote", text.length == 0 {
            // Return on an empty item ends the list.
            setKind(BlockChoice.all[0], at: para)
            Haptics.selectionTick()
            return false
        }
        let next: BlockTag
        switch tag.kind {
        case "bullet", "numbered", "quote": next = BlockTag(kind: tag.kind, indent: tag.indent)
        case "task": next = BlockTag(kind: "task", indent: tag.indent, checked: false)
        default: next = BlockTag(kind: "paragraph")
        }
        if range.length > 0 {
            applying = true
            s.replaceCharacters(in: range, with: "")
            applying = false
        }
        insertBlockBreak(at: range.location, newTag: next, carry: true)
        return false
    }

    /// Splits the paragraph at `loc`: the text after it moves into a new block.
    private func insertBlockBreak(at loc: Int, newTag: BlockTag, carry: Bool) {
        guard let host else { return }
        let s = host.pageStorage
        let para = paragraphRange(at: loc)
        let old = tag(at: para)
        let text = textRange(para, tag: old)
        let tailRange = NSRange(location: loc, length: max(0, text.location + text.length - loc))
        let tail = carry ? s.attributedSubstring(from: tailRange) : NSAttributedString()
        applying = true
        s.beginEditing()
        if carry && tailRange.length > 0 { s.replaceCharacters(in: tailRange, with: "") }
        let piece = NSMutableAttributedString(string: "\n", attributes: PageStyle.attributes(old, marks: [:]))
        let number = newTag.kind == "numbered" ? old.number + 1 : 0
        let newPara = NSMutableAttributedString(attributedString: newTag.hasMarker ? PageText.marker(newTag, number: number) : NSAttributedString())
        let moved = NSMutableAttributedString(attributedString: tail)
        moved.addAttribute(.zoenBlock, value: newTag, range: NSRange(location: 0, length: moved.length))
        PageText.restyle(moved, tag: newTag, in: NSRange(location: 0, length: moved.length))
        newPara.append(moved)
        piece.append(newPara)
        s.insert(piece, at: loc)
        s.endEditing()
        applying = false
        let caret = loc + 1 + (newTag.hasMarker ? 1 : 0)
        host.pageSelection = NSRange(location: caret, length: 0)
        if loc + piece.length >= s.length { trailingTag = newTag }
        var typing = PageStyle.attributes(newTag, marks: [:])
        typing[.zoenBlock] = newTag
        host.pageTypingAttributes = typing
        renumber()
        bump(strong: false)
        edited()
    }

    /// After any change: fix tags, markers and styles of the touched paragraphs.
    func textDidChange(edited range: NSRange?) {
        guard let host, !applying else { return }
        normalize(around: range ?? host.pageSelection)
        edited()
    }

    private func normalize(around range: NSRange) {
        guard let host else { return }
        let s = host.pageStorage
        let ns = s.string as NSString
        let from = paragraphRange(at: max(0, range.location - 1))
        let to = paragraphRange(at: min(ns.length, range.location + range.length))
        let span = NSUnionRange(from, to)
        var seen = Set<String>()
        // Ids seen before the edited span keep theirs; a split paragraph gets a new id.
        let before = NSRange(location: 0, length: span.location)
        if before.length > 0 {
            s.enumerateAttribute(.zoenBlock, in: before) { v, _, _ in if let t = v as? BlockTag { seen.insert(t.id) } }
        }
        applying = true
        s.beginEditing()
        var loc = span.location
        while loc <= span.location + span.length {
            let p = paragraphRange(at: loc)
            var t = tag(at: p)
            if seen.contains(t.id) { t = t.with(id: BlockTag.newId()) }
            seen.insert(t.id)
            let first = p.length > 0 ? (s.string as NSString).substring(with: NSRange(location: p.location, length: 1)) : ""
            let isMarker = first == PageMarker.char && s.attribute(.zoenMarker, at: p.location, effectiveRange: nil) != nil
            if t.hasMarker && !isMarker {
                // The marker was deleted: the block becomes plain text.
                t = BlockTag(id: t.id, kind: "paragraph")
            }
            if !t.hasMarker && isMarker {
                s.replaceCharacters(in: NSRange(location: p.location, length: 1), with: "")
            }
            let p2 = paragraphRange(at: p.location)
            if p2.length > 0 {
                s.addAttribute(.zoenBlock, value: t, range: p2)
                PageText.restyle(s, tag: t, in: p2)
            }
            if p2.location + p2.length >= s.length { trailingTag = t }
            if p2.length == 0 || p2.location + p2.length >= s.length { break }
            loc = p2.location + p2.length
        }
        s.endEditing()
        applying = false
        renumber()
        updateState()
    }

    /// Redraws numbered markers after items were added, removed or moved.
    private func renumber() {
        guard let host else { return }
        let s = host.pageStorage
        guard s.length > 0 else { return }
        var counter = NumberCounter()
        applying = true
        s.beginEditing()
        for p in PageText.paragraphs(s) {
            let t = tag(at: p)
            let n = counter.next(t)
            guard t.kind == "numbered", p.length > 0,
                  let old = s.attribute(.zoenMarker, at: p.location, effectiveRange: nil) as? UInt32, old != n
            else { continue }
            s.replaceCharacters(in: NSRange(location: p.location, length: 1), with: PageText.marker(t, number: n))
        }
        s.endEditing()
        applying = false
    }

    private func edited() {
        updateState()
        onEdit?()
    }

    private func bump(strong: Bool) {
        if strong { strongTick += 1 } else { tick += 1 }
    }

    // MARK: selection

    func selectionDidChange() {
        guard let host, !applying else { return }
        // Keep the caret after a marker, never before it.
        let sel = host.pageSelection
        if sel.length == 0 {
            let p = paragraphRange(at: sel.location)
            let t = tag(at: p)
            if t.hasMarker, !t.isAtomic, sel.location == p.location, p.length > 0 {
                host.pageSelection = NSRange(location: p.location + 1, length: 0)
            }
        }
        refreshTyping()
        updateState()
    }

    private func refreshTyping() {
        guard let host else { return }
        let sel = host.pageSelection
        let p = paragraphRange(at: sel.location)
        let t = tag(at: p)
        var marks: [NSAttributedString.Key: Any] = [:]
        if sel.location > p.location, sel.location - 1 < host.pageStorage.length {
            let attrs = host.pageStorage.attributes(at: sel.location - 1, effectiveRange: nil)
            for (k, name) in NSAttributedString.Key.markKeys where attrs[k] != nil && name != "a" && name != "c" && name != "hb" {
                marks[k] = attrs[k]
            }
        }
        for (k, _) in NSAttributedString.Key.markKeys {
            if let v = host.pageTypingAttributes[k], ["zoen.b", "zoen.i", "zoen.s"].contains(k.rawValue), sel.location == p.location + (t.hasMarker ? 1 : 0) {
                marks[k] = v
            }
        }
        var typing = PageStyle.attributes(t, marks: marks)
        typing[.zoenBlock] = t
        host.pageTypingAttributes = typing
    }

    private func updateState() {
        guard let host else { return }
        let sel = host.pageSelection
        let p = paragraphRange(at: sel.location)
        let t = tag(at: p)
        if currentKind != t.kind { currentKind = t.kind }
        if currentLevel != t.level { currentLevel = t.level }
        hasSelection = sel.length > 0
        let empty = host.pageStorage.length == 0
        if empty != isEmpty { isEmpty = empty }
        var marks = Set<String>()
        if sel.length > 0 {
            for (k, name) in NSAttributedString.Key.markKeys {
                var all = true
                host.pageStorage.enumerateAttribute(k, in: sel) { v, _, stop in if v == nil { all = false; stop.pointee = true } }
                if all { marks.insert(name) }
            }
        } else {
            for (k, name) in NSAttributedString.Key.markKeys where host.pageTypingAttributes[k] != nil { marks.insert(name) }
        }
        if marks != activeMarks { activeMarks = marks }
        // The block menu: "/" typed at the start of an empty block.
        let text = textRange(p, tag: t)
        let str = (host.pageStorage.string as NSString).substring(with: text)
        let q: String? = (t.kind == "paragraph" && str.hasPrefix("/") && !str.contains(" ") && sel.location == text.location + text.length)
            ? String(str.dropFirst()) : nil
        if q != slashQuery {
            if q != nil && slashQuery == nil { Haptics.selectionTick() }
            withAnimation(.spring(duration: 0.3, bounce: 0.25)) { slashQuery = q }
        }
    }

    var slashChoices: [BlockChoice] {
        guard let q = slashQuery?.lowercased(), !q.isEmpty else { return BlockChoice.all }
        return BlockChoice.all.filter { $0.searchKeys.contains(q) }
    }

    // MARK: commands (format bar, block menu)

    /// Turns the block under the cursor into `choice` (clearing a "/query" first).
    func choose(_ choice: BlockChoice) {
        guard let host else { return }
        let p = paragraphRange(at: host.pageSelection.location)
        if slashQuery != nil {
            let t = tag(at: p)
            let text = textRange(p, tag: t)
            applying = true
            host.pageStorage.replaceCharacters(in: text, with: "")
            applying = false
            host.pageSelection = NSRange(location: text.location, length: 0)
        }
        let same = currentKind == choice.kind && (choice.kind != "heading" || currentLevel == choice.level)
        setKind(same && choice.kind != "paragraph" ? BlockChoice.all[0] : choice, at: paragraphRange(at: host.pageSelection.location))
        bump(strong: true)
    }

    func setKind(_ choice: BlockChoice, at para: NSRange) {
        guard let host else { return }
        let s = host.pageStorage
        let old = tag(at: para)
        var t = BlockTag(id: old.id, kind: choice.kind, level: choice.level,
                         indent: (choice.kind == old.kind || (old.isListItem && ["bullet", "numbered", "task"].contains(choice.kind))) ? old.indent : 0,
                         number: choice.kind == "numbered" ? max(old.number, 1) : 0, checked: false)
        if choice.kind == "divider" {
            // A divider is its own block; the text (if any) moves below it.
            t = BlockTag(id: old.id, kind: "divider")
        }
        applying = true
        s.beginEditing()
        let ns = s.string as NSString
        let hadMarker = para.length > 0 && ns.substring(with: NSRange(location: para.location, length: 1)) == PageMarker.char
            && s.attribute(.zoenMarker, at: para.location, effectiveRange: nil) != nil
        var caretShift = 0
        if hadMarker {
            s.replaceCharacters(in: NSRange(location: para.location, length: 1), with: "")
            caretShift -= 1
        }
        if t.hasMarker {
            s.insert(PageText.marker(t, number: t.number), at: para.location)
            caretShift += 1
        }
        let p2 = paragraphRange(at: para.location)
        if p2.length > 0 {
            s.addAttribute(.zoenBlock, value: t, range: p2)
            PageText.restyle(s, tag: t, in: p2)
        }
        s.endEditing()
        applying = false
        if p2.location + p2.length >= s.length { trailingTag = t }
        let sel = host.pageSelection
        host.pageSelection = NSRange(location: max(p2.location + (t.hasMarker ? 1 : 0), sel.location + caretShift), length: 0)
        if t.kind == "divider" {
            insertBlockBreak(at: p2.location + 1, newTag: BlockTag(kind: "paragraph"), carry: true)
            return
        }
        refreshTyping()
        renumber()
        edited()
    }

    /// Bold, italic, strikethrough, code: on the selection, or for what's typed next.
    func toggleMark(_ name: String, value: String? = nil) {
        guard let host, let key = NSAttributedString.Key.markKeys.first(where: { $0.1 == name })?.0 else { return }
        let sel = host.pageSelection
        if sel.length == 0 {
            var typing = host.pageTypingAttributes
            if typing[key] != nil { typing[key] = nil } else { typing[key] = value ?? true }
            let t = (typing[.zoenBlock] as? BlockTag) ?? BlockTag(kind: "paragraph")
            var marks: [NSAttributedString.Key: Any] = [:]
            for (k, _) in NSAttributedString.Key.markKeys where typing[k] != nil { marks[k] = typing[k] }
            var next = PageStyle.attributes(t, marks: marks)
            next[.zoenBlock] = t
            host.pageTypingAttributes = next
            updateState()
            bump(strong: false)
            return
        }
        let s = host.pageStorage
        let on = activeMarks.contains(name) && value == nil
        applying = true
        s.beginEditing()
        if on { s.removeAttribute(key, range: sel) } else { s.addAttribute(key, value: value ?? true, range: sel) }
        // Restyle each paragraph the selection touches.
        var loc = sel.location
        while loc < sel.location + sel.length {
            let p = paragraphRange(at: loc)
            let t = tag(at: p)
            let r = NSIntersectionRange(p, sel)
            PageText.restyle(s, tag: t, in: r)
            if p.length == 0 { break }
            loc = p.location + p.length
        }
        s.endEditing()
        applying = false
        host.pageSelection = sel
        bump(strong: false)
        edited()
    }

    func setLink(_ url: String) {
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let host else { return }
        if trimmed.isEmpty {
            if host.pageSelection.length > 0 {
                applying = true
                host.pageStorage.removeAttribute(.zoenLink, range: host.pageSelection)
                PageText.restyle(host.pageStorage, tag: tag(at: paragraphRange(at: host.pageSelection.location)), in: host.pageSelection)
                applying = false
                edited()
            }
            return
        }
        let full = trimmed.contains("://") || trimmed.hasPrefix("mailto:") ? trimmed : "https://\(trimmed)"
        if host.pageSelection.length == 0 {
            // No selection: insert the address itself as a link.
            let t = tag(at: paragraphRange(at: host.pageSelection.location))
            var attrs = PageStyle.attributes(t, marks: [.zoenLink: full])
            attrs[.zoenLink] = full
            applying = true
            host.pageStorage.insert(NSAttributedString(string: trimmed, attributes: attrs), at: host.pageSelection.location)
            applying = false
            host.pageSelection = NSRange(location: host.pageSelection.location + (trimmed as NSString).length, length: 0)
            edited()
            return
        }
        toggleMark("a", value: full)
    }

    /// The link under the selection, to edit it.
    var currentLink: String? {
        guard let host, host.pageStorage.length > 0 else { return nil }
        let loc = min(host.pageSelection.location, host.pageStorage.length - 1)
        return host.pageStorage.attribute(.zoenLink, at: loc, effectiveRange: nil) as? String
    }

    /// Moves list items in or out a level.
    func indent(_ delta: Int) {
        guard let host else { return }
        let p = paragraphRange(at: host.pageSelection.location)
        let t = tag(at: p)
        guard t.isListItem else { return }
        let n = UInt32(max(0, min(6, Int(t.indent) + delta)))
        guard n != t.indent else { return }
        let nt = t.with(indent: n)
        applying = true
        host.pageStorage.beginEditing()
        if p.length > 0 {
            host.pageStorage.replaceCharacters(in: NSRange(location: p.location, length: 1), with: PageText.marker(nt, number: nt.number))
            let p2 = paragraphRange(at: p.location)
            host.pageStorage.addAttribute(.zoenBlock, value: nt, range: p2)
            PageText.restyle(host.pageStorage, tag: nt, in: p2)
        }
        host.pageStorage.endEditing()
        applying = false
        renumber()
        bump(strong: false)
        edited()
    }

    /// A tap on a checkbox. Returns true when it hit one.
    func toggleCheckbox(at index: Int) -> Bool {
        guard let host, editable, index >= 0, index < host.pageStorage.length else { return false }
        let p = paragraphRange(at: index)
        let t = tag(at: p)
        guard t.kind == "task", index == p.location else { return false }
        let nt = t.with(checked: !t.checked)
        applying = true
        host.pageStorage.beginEditing()
        host.pageStorage.replaceCharacters(in: NSRange(location: p.location, length: 1), with: PageText.marker(nt, number: 0))
        let p2 = paragraphRange(at: p.location)
        host.pageStorage.addAttribute(.zoenBlock, value: nt, range: p2)
        PageText.restyle(host.pageStorage, tag: nt, in: p2)
        host.pageStorage.endEditing()
        applying = false
        if nt.checked { Haptics.commit() } else { Haptics.selectionTick() }
        bump(strong: nt.checked)
        edited()
        return true
    }
}
