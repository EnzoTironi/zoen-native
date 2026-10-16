import SwiftUI
import RodaCore
import AppKit

struct MacSearchCommands: Equatable {
    let scope: UUID
    let query: Binding<String>
    let field: FocusState<Bool>.Binding
    let sidebar: Binding<Bool>
    let reduceMotion: Bool

    static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.scope == rhs.scope && lhs.reduceMotion == rhs.reduceMotion
    }

    @MainActor func focus() {
        withAnimation(reduceMotion ? nil : .easeInOut(duration: 0.22), completionCriteria: .removed) {
            sidebar.wrappedValue = true
        } completion: {
            field.wrappedValue = true
        }
    }

    @MainActor func reset() {
        query.wrappedValue = ""
        field.wrappedValue = false
    }
}

struct MacSearchCommandsKey: FocusedValueKey {
    typealias Value = MacSearchCommands
}

struct MacEscapeHandler: NSViewRepresentable {
    let onEscape: () -> Bool

    func makeNSView(context: Context) -> EscapeView {
        let view = EscapeView()
        view.onEscape = onEscape
        return view
    }

    func updateNSView(_ view: EscapeView, context: Context) {
        view.onEscape = onEscape
    }

    static func dismantleNSView(_ view: EscapeView, coordinator: ()) {
        view.stopMonitoring()
    }

    final class EscapeView: NSView {
        var onEscape: (() -> Bool)?
        private var monitor: Any?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stopMonitoring()
            guard window != nil else { return }
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard let self, let window = self.window,
                      event.window === window, NSApp.keyWindow === window,
                      window.sheetParent == nil, window.attachedSheet == nil,
                      NSApp.modalWindow == nil,
                      event.charactersIgnoringModifiers == "\u{1b}",
                      event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty
                else { return event }
                if let editor = window.firstResponder as? NSTextView, editor.hasMarkedText() {
                    return event
                }
                return self.onEscape?() == true ? nil : event
            }
        }

        func stopMonitoring() {
            if let monitor { NSEvent.removeMonitor(monitor) }
            monitor = nil
        }
    }
}

extension FocusedValues {
    var macSearch: MacSearchCommands? {
        get { self[MacSearchCommandsKey.self] }
        set { self[MacSearchCommandsKey.self] = newValue }
    }
}

@main
struct ZoenMacApp: App {
    @State private var model = AppModel()
    @AppStorage("RodaAppearance") private var appearance = "system"
    @FocusedValue(\.macSearch) private var searchCommands

    private var colorScheme: ColorScheme? {
        switch appearance {
        case "light": .light
        case "dark": .dark
        default: nil
        }
    }

    var body: some Scene {
        WindowGroup("Zoen") {
            ZoenAdaptiveShell()
                .environment(model)
                .tint(Palette.action)
                .preferredColorScheme(colorScheme)
                .frame(minWidth: 1080, minHeight: 680)
                .task { await model.applyLaunchOptions() }
                .onOpenURL { model.captureAcquisition($0) }
        }
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unifiedCompact(showsTitle: false))
        .windowBackgroundDragBehavior(.enabled)
        .defaultSize(width: 1380, height: 860)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("New Chat") { model.newChatOpen = true }
                    .keyboardShortcut("n", modifiers: .command)
                    .disabled(model.sync.account == nil && SyncModel.mode != .demo)
            }
            CommandMenu("Zoen") {
                Button("Search") { searchCommands?.focus() }
                    .keyboardShortcut("f", modifiers: .command)
                    .disabled(searchCommands == nil)
                Button("Back") { _ = model.pop() }
                    .keyboardShortcut("[", modifiers: .command)
                    .disabled(model.path(.conversations).isEmpty)
                Button("Chat with Zoen") { searchCommands?.reset(); if let id = model.zoenSpaceId() { model.go(.space(id)) } }
                    .keyboardShortcut("0", modifiers: .command)
                Button("Activity") { searchCommands?.reset(); model.select(.activity) }
                    .keyboardShortcut("1", modifiers: .command)
                Button("Create and more") {
                    withAnimation(NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? nil : .spring(duration: 0.45, bounce: 0.28)) {
                        model.radialOpen.toggle()
                    }
                }
                    .keyboardShortcut("k", modifiers: .command)
            }
        }
    }
}
