import SwiftUI
import RodaCore

#if canImport(UIKit)
import UIKit

/// The page's text view (TextKit 2).
final class PageUITextView: UITextView, PageTextHost {
    /// Take the keyboard once on screen (a new page starts typing right away).
    var wantsFocus = false
    override func didMoveToWindow() {
        super.didMoveToWindow()
        guard wantsFocus, window != nil else { return }
        wantsFocus = false
        DispatchQueue.main.async { [weak self] in
            guard let self, isEditable else { return }
            _ = becomeFirstResponder()
        }
    }
    var pageStorage: NSTextStorage { textStorage }
    var pageHasMarkedText: Bool { markedTextRange != nil }
    func pageEndComposition() { unmarkText() }
    func pageViewport() -> PageViewport? {
        guard let position = closestPosition(to: CGPoint(x: textContainerInset.left + 4, y: contentOffset.y + 4)) else { return nil }
        let rect = caretRect(for: position)
        return PageViewport(index: offset(from: beginningOfDocument, to: position),
                            offset: CGPoint(x: contentOffset.x, y: contentOffset.y - rect.minY))
    }
    func pageRestoreViewport(_ viewport: PageViewport) {
        guard let position = position(from: beginningOfDocument, offset: viewport.index) else { return }
        let y = caretRect(for: position).minY + viewport.offset.y
        let minimum = -adjustedContentInset.top
        let maximum = max(minimum, contentSize.height - bounds.height + adjustedContentInset.bottom)
        setContentOffset(CGPoint(x: viewport.offset.x, y: min(max(y, minimum), maximum)), animated: false)
    }
    var pageSelection: NSRange {
        get { selectedRange }
        set { selectedRange = newValue }
    }
    var pageTypingAttributes: [NSAttributedString.Key: Any] {
        get { typingAttributes }
        set { typingAttributes = newValue }
    }
}

struct PageTextView: UIViewRepresentable {
    let controller: PageEditorController
    var editable = true
    var autofocus = false
    var onLink: (() -> Void)?

    func makeCoordinator() -> Coordinator { Coordinator(controller: controller) }

    func makeUIView(context: Context) -> PageUITextView {
        let tv = PageUITextView(usingTextLayoutManager: true)
        tv.backgroundColor = .clear
        tv.isScrollEnabled = true
        tv.alwaysBounceVertical = true
        tv.keyboardDismissMode = .interactive
        tv.textContainerInset = UIEdgeInsets(top: 8, left: 18, bottom: 120, right: 18)
        tv.adjustsFontForContentSizeCategory = true
        tv.autocorrectionType = .default
        tv.smartDashesType = .no
        tv.smartQuotesType = .no
        tv.linkTextAttributes = [:]
        tv.delegate = context.coordinator
        tv.isEditable = editable
        tv.accessibilityIdentifier = "page.editor"
        controller.editable = editable
        controller.host = tv
        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped(_:)))
        tap.delegate = context.coordinator
        tv.addGestureRecognizer(tap)
        context.coordinator.updateAccessory(tv, editable: editable, onLink: onLink)
        tv.wantsFocus = autofocus
        context.coordinator.focused = autofocus
        return tv
    }

    func updateUIView(_ tv: PageUITextView, context: Context) {
        if tv.isEditable != editable { tv.isEditable = editable }
        if controller.editable != editable { controller.editable = editable }
        context.coordinator.updateAccessory(tv, editable: editable, onLink: onLink)
        if !editable { tv.wantsFocus = false }
        // The screen learns it's a new page after loading, so focus can arrive here too (once).
        if autofocus, !context.coordinator.focused {
            context.coordinator.focused = true
            if tv.window != nil {
                DispatchQueue.main.async { if tv.isEditable { _ = tv.becomeFirstResponder() } }
            } else { tv.wantsFocus = true }
        }
    }

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate, UIGestureRecognizerDelegate {
        let controller: PageEditorController
        var bar: UIHostingController<FormatBar>?
        var focused = false
        private var pending: NSRange?

        init(controller: PageEditorController) { self.controller = controller }

        func updateAccessory(_ tv: PageUITextView, editable: Bool, onLink: (() -> Void)?) {
            if editable {
                let content = FormatBar(controller: controller, onLink: { onLink?() })
                let bar: UIHostingController<FormatBar>
                if let existing = self.bar {
                    bar = existing
                    bar.rootView = content
                } else {
                    bar = UIHostingController(rootView: content)
                    bar.view.backgroundColor = .clear
                    bar.sizingOptions = [.intrinsicContentSize]
                    bar.view.frame = CGRect(x: 0, y: 0, width: 400, height: 58)
                    bar.view.autoresizingMask = [.flexibleWidth]
                    self.bar = bar
                }
                if tv.inputAccessoryView !== bar.view {
                    tv.inputAccessoryView = bar.view
                    tv.reloadInputViews()
                }
            } else if tv.inputAccessoryView != nil {
                tv.inputAccessoryView = nil
                bar = nil
                tv.reloadInputViews()
            }
        }

        func textView(_ textView: UITextView, shouldChangeTextIn range: NSRange, replacementText text: String) -> Bool {
            let ok = controller.shouldChange(range, replacement: text)
            pending = ok ? NSRange(location: range.location, length: (text as NSString).length) : nil
            return ok
        }

        func textViewDidChange(_ textView: UITextView) {
            controller.textDidChange(edited: pending)
            pending = nil
        }

        func textViewDidChangeSelection(_ textView: UITextView) {
            controller.selectionDidChange()
        }

        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

        @objc func tapped(_ g: UITapGestureRecognizer) {
            guard let tv = g.view as? UITextView, g.state == .ended else { return }
            let pt = g.location(in: tv)
            // A tap below the last line continues writing at the end.
            if tv.isEditable, pt.y > tv.caretRect(for: tv.endOfDocument).maxY + 8 {
                if !tv.isFirstResponder { _ = tv.becomeFirstResponder() }
                tv.selectedRange = NSRange(location: tv.textStorage.length, length: 0)
                // UITextView's own tap may place the caret after this one; end wins.
                DispatchQueue.main.async { tv.selectedRange = NSRange(location: tv.textStorage.length, length: 0) }
                return
            }
            guard let pos = tv.closestPosition(to: pt) else { return }
            let idx = tv.offset(from: tv.beginningOfDocument, to: pos)
            // A tap left of the text, on the marker.
            for i in [idx, idx - 1] where i >= 0 && i < tv.textStorage.length {
                guard let start = tv.position(from: tv.beginningOfDocument, offset: i),
                      let end = tv.position(from: start, offset: 1),
                      let range = tv.textRange(from: start, to: end) else { continue }
                let rect = tv.firstRect(for: range).insetBy(dx: -10, dy: -8)
                if rect.contains(pt), controller.toggleCheckbox(at: i) {
                    return
                }
            }
        }
    }
}

#else
import AppKit

final class PageNSTextView: NSTextView, PageTextHost {
    weak var controller: PageEditorController?
    var pageStorage: NSTextStorage { textStorage ?? NSTextStorage() }
    var pageHasMarkedText: Bool { hasMarkedText() }
    func pageEndComposition() { unmarkText() }
    private func pageCaretY(_ index: Int) -> CGFloat? {
        guard let window else { return nil }
        let rect = firstRect(forCharacterRange: NSRange(location: index, length: 0), actualRange: nil)
        return convert(window.convertFromScreen(rect), from: nil).minY
    }
    func pageViewport() -> PageViewport? {
        let index = min(characterIndexForInsertion(at: visibleRect.origin), pageStorage.length)
        guard let y = pageCaretY(index) else { return nil }
        return PageViewport(index: index, offset: CGPoint(x: visibleRect.minX, y: visibleRect.minY - y))
    }
    func pageRestoreViewport(_ viewport: PageViewport) {
        guard let scroll = enclosingScrollView, let y = pageCaretY(viewport.index) else { return }
        scroll.contentView.scroll(to: CGPoint(x: viewport.offset.x, y: y + viewport.offset.y))
        scroll.reflectScrolledClipView(scroll.contentView)
    }
    var pageSelection: NSRange {
        get { selectedRange() }
        set { setSelectedRange(newValue) }
    }
    var pageTypingAttributes: [NSAttributedString.Key: Any] {
        get { typingAttributes }
        set { typingAttributes = newValue }
    }

    override func mouseDown(with event: NSEvent) {
        let pt = convert(event.locationInWindow, from: nil)
        let idx = characterIndexForInsertion(at: pt)
        for i in [idx, idx - 1] where i >= 0 && i < pageStorage.length {
            let r = firstRect(forCharacterRange: NSRange(location: i, length: 1), actualRange: nil)
            let local = window.map { convert($0.convertFromScreen(r).origin, from: nil) } ?? .zero
            let hit = CGRect(origin: CGPoint(x: local.x - 10, y: local.y - r.height - 8), size: CGSize(width: r.width + 20, height: r.height + 16))
            if hit.contains(pt), controller?.toggleCheckbox(at: i) == true { return }
        }
        super.mouseDown(with: event)
    }
}

struct PageTextView: NSViewRepresentable {
    let controller: PageEditorController
    var editable = true
    var autofocus = false
    var onLink: (() -> Void)?

    func makeCoordinator() -> Coordinator { Coordinator(controller: controller) }

    func makeNSView(context: Context) -> NSScrollView {
        let tv = PageNSTextView(usingTextLayoutManager: true)
        tv.controller = controller
        tv.drawsBackground = false
        tv.isRichText = true
        tv.allowsUndo = true
        tv.isAutomaticQuoteSubstitutionEnabled = false
        tv.isAutomaticDashSubstitutionEnabled = false
        tv.textContainerInset = NSSize(width: 18, height: 12)
        tv.isVerticallyResizable = true
        tv.isHorizontallyResizable = false
        tv.autoresizingMask = [.width]
        tv.textContainer?.widthTracksTextView = true
        tv.linkTextAttributes = [:]
        tv.delegate = context.coordinator
        tv.isEditable = editable
        tv.setAccessibilityIdentifier("page.editor")
        controller.editable = editable
        controller.host = tv
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.documentView = tv
        if autofocus { DispatchQueue.main.async { if tv.isEditable { tv.window?.makeFirstResponder(tv) } } }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        if let tv = scroll.documentView as? NSTextView, tv.isEditable != editable { tv.isEditable = editable }
        if controller.editable != editable { controller.editable = editable }
        if autofocus, let tv = scroll.documentView as? NSTextView, tv.window?.firstResponder !== tv, tv.string.isEmpty {
            DispatchQueue.main.async { if tv.isEditable { tv.window?.makeFirstResponder(tv) } }
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        let controller: PageEditorController
        private var pending: NSRange?
        init(controller: PageEditorController) { self.controller = controller }

        func textView(_ textView: NSTextView, shouldChangeTextIn range: NSRange, replacementString text: String?) -> Bool {
            let t = text ?? ""
            let ok = controller.shouldChange(range, replacement: t)
            pending = ok ? NSRange(location: range.location, length: (t as NSString).length) : nil
            return ok
        }

        func textDidChange(_ notification: Notification) {
            controller.textDidChange(edited: pending)
            pending = nil
        }

        func textViewDidChangeSelection(_ notification: Notification) {
            controller.selectionDidChange()
        }
    }
}
#endif
