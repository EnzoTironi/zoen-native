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
        DispatchQueue.main.async { [weak self] in _ = self?.becomeFirstResponder() }
    }
    var pageStorage: NSTextStorage { textStorage }
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
        controller.host = tv
        controller.editable = editable
        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped(_:)))
        tap.delegate = context.coordinator
        tv.addGestureRecognizer(tap)
        if editable {
            let bar = UIHostingController(rootView: FormatBar(controller: controller, onLink: { onLink?() }))
            bar.view.backgroundColor = .clear
            bar.sizingOptions = [.intrinsicContentSize]
            bar.view.frame = CGRect(x: 0, y: 0, width: 400, height: 58)
            bar.view.autoresizingMask = [.flexibleWidth]
            tv.inputAccessoryView = bar.view
            context.coordinator.bar = bar
        }
        tv.wantsFocus = autofocus
        context.coordinator.focused = autofocus
        return tv
    }

    func updateUIView(_ tv: PageUITextView, context: Context) {
        if tv.isEditable != editable { tv.isEditable = editable }
        if controller.editable != editable { controller.editable = editable }
        // The screen learns it's a new page after loading, so focus can arrive here too (once).
        if autofocus, !context.coordinator.focused {
            context.coordinator.focused = true
            if tv.window != nil { DispatchQueue.main.async { _ = tv.becomeFirstResponder() } } else { tv.wantsFocus = true }
        }
    }

    @MainActor
    final class Coordinator: NSObject, UITextViewDelegate, UIGestureRecognizerDelegate {
        let controller: PageEditorController
        var bar: UIHostingController<FormatBar>?
        var focused = false
        private var pending: NSRange?

        init(controller: PageEditorController) { self.controller = controller }

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
        controller.host = tv
        controller.editable = editable
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.documentView = tv
        if autofocus { DispatchQueue.main.async { tv.window?.makeFirstResponder(tv) } }
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        if let tv = scroll.documentView as? NSTextView, tv.isEditable != editable { tv.isEditable = editable }
        if controller.editable != editable { controller.editable = editable }
        if autofocus, let tv = scroll.documentView as? NSTextView, tv.window?.firstResponder !== tv, tv.string.isEmpty {
            DispatchQueue.main.async { tv.window?.makeFirstResponder(tv) }
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
