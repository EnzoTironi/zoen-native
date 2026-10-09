import SwiftUI

/// Above the keyboard: the block menu while "/" is open, else formatting.
struct FormatBar: View {
    let controller: PageEditorController
    var onLink: () -> Void = {}

    var body: some View {
        ZStack {
            if controller.slashQuery != nil {
                blockMenu.transition(.move(edge: .bottom).combined(with: .opacity))
            } else {
                tools.transition(.opacity)
            }
        }
        .frame(height: 50)
        .padding(.horizontal, 8)
        .padding(.bottom, 4)
        .sensoryFeedback(.selection, trigger: controller.tick)
        .sensoryFeedback(.impact(weight: .medium, intensity: 0.8), trigger: controller.strongTick)
    }

    private var tools: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 4) {
                Menu {
                    ForEach(BlockChoice.all) { c in
                        Button { controller.choose(c) } label: { Label(String(localized: c.title), systemImage: c.symbol) }
                    }
                } label: {
                    Label("Style", systemImage: currentSymbol)
                        .labelStyle(.iconOnly)
                        .font(.system(size: 17, weight: .semibold))
                        .frame(width: 44, height: 40)
                        .contentTransition(.symbolEffect(.replace))
                }
                .accessibilityLabel(Text("Block style"))
                divider
                markButton("b", "bold", label: "Bold")
                markButton("i", "italic", label: "Italic")
                markButton("s", "strikethrough", label: "Strikethrough")
                markButton("c", "chevron.left.forwardslash.chevron.right", label: "Code")
                bar(symbol: "link", label: "Link", on: controller.activeMarks.contains("a")) { onLink() }
                divider
                bar(symbol: "checklist", label: "Checklist", on: controller.currentKind == "task") { pick("task") }
                bar(symbol: "list.bullet", label: "Bulleted list", on: controller.currentKind == "bullet") { pick("bullet") }
                bar(symbol: "list.number", label: "Numbered list", on: controller.currentKind == "numbered") { pick("numbered") }
                bar(symbol: "decrease.indent", label: "Outdent", on: false) { controller.indent(-1) }
                    .disabled(!["bullet", "numbered", "task"].contains(controller.currentKind))
                bar(symbol: "increase.indent", label: "Indent", on: false) { controller.indent(1) }
                    .disabled(!["bullet", "numbered", "task"].contains(controller.currentKind))
            }
            .padding(.horizontal, 6)
        }
        .glassEffect(.regular, in: .capsule)
    }

    private var blockMenu: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(controller.slashChoices) { c in
                    Button { controller.choose(c) } label: {
                        Label(String(localized: c.title), systemImage: c.symbol)
                            .font(.subheadline.weight(.medium))
                            .padding(.horizontal, 12)
                            .frame(height: 38)
                            .background(Palette.surfaceMuted, in: .capsule)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("slash.\(c.id)")
                }
                if controller.slashChoices.isEmpty {
                    Text("No block called that").font(.subheadline).foregroundStyle(Palette.textSecondary).padding(.horizontal, 12)
                }
            }
            .padding(.horizontal, 6)
            .animation(.spring(duration: 0.3), value: controller.slashQuery)
        }
        .glassEffect(.regular, in: .capsule)
    }

    private var currentSymbol: String {
        switch controller.currentKind {
        case "heading": controller.currentLevel <= 1 ? "textformat.size.larger" : (controller.currentLevel == 2 ? "textformat.size" : "textformat.size.smaller")
        case "task": "checklist"
        case "bullet": "list.bullet"
        case "numbered": "list.number"
        case "quote": "text.quote"
        case "code": "chevron.left.forwardslash.chevron.right"
        default: "textformat"
        }
    }

    private var divider: some View {
        Rectangle().fill(Palette.hairline).frame(width: 1, height: 22).padding(.horizontal, 2)
    }

    private func pick(_ id: String) {
        if let c = BlockChoice.all.first(where: { $0.id == id }) { controller.choose(c) }
    }

    private func markButton(_ key: String, _ symbol: String, label: LocalizedStringKey) -> some View {
        bar(symbol: symbol, label: label, on: controller.activeMarks.contains(key)) { controller.toggleMark(key) }
    }

    private func bar(symbol: String, label: LocalizedStringKey, on: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: on ? .bold : .medium))
                .foregroundStyle(on ? Palette.action : Palette.textPrimary)
                .frame(width: 40, height: 40)
                .background(on ? Palette.action.opacity(0.14) : .clear, in: .rect(cornerRadius: 10, style: .continuous))
                .scaleEffect(on ? 1.06 : 1)
                .animation(.spring(duration: 0.25, bounce: 0.4), value: on)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(Text(label))
    }
}
