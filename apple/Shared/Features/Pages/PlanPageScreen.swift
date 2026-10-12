import SwiftUI
import RodaCore

struct PlanItemView: View {
    @Environment(AppModel.self) private var model
    @ScaledMetric(relativeTo: .body) private var textScale: CGFloat = 1
    let itemId: String
    var onClose: (() -> Void)? = nil

    @State private var page: PlanPageDto?
    @State private var item: ItemDetail?
    @State private var editing: EditingLine?
    @State private var editingVersion: UInt32?
    @State private var editingHash: String?
    @State private var showVersions = false
    @State private var loadFailed = false
    @State private var reaction: String?
    @State private var bump = 0

    struct EditingLine: Identifiable {
        var id: String { lineId ?? "new-\(section)" }
        var lineId: String?
        var section: UInt32
        var text: String
        var amount: String
    }

    var body: some View {
        ScrollView {
            if let page {
                let layout = PlanPageLayout(blocks: page.blocks)
                VStack(alignment: .leading, spacing: 0) {
                    if let item { LivePageMetadata(item: item, version: page.version) }
                    LazyVStack(alignment: .leading, spacing: 12) {
                        if !page.ready {
                            Label("Waiting for page content", systemImage: "icloud.and.arrow.down")
                                .font(.callout).foregroundStyle(Palette.textSecondary)
                        }
                        ForEach(layout.blocks, id: \.id) { block in
                            blockView(block, layout: layout, page: page)
                            if block.id == layout.title?.id { budget(page, layout: layout) }
                        }
                        if let reaction, let agent = model.agents.first(where: { $0.persona.handle == "financeiro" })?.persona ?? model.zoen {
                            AgentReaction(agent: agent, text: reaction)
                        }
                        if let error = page.saveError {
                            Label(error, systemImage: "exclamationmark.circle")
                                .font(.callout).foregroundStyle(Palette.danger)
                                .accessibilityIdentifier("plan-page.saveError")
                        }
                    }
                    .padding(.horizontal, 22).padding(.top, 18).padding(.bottom, 28)
                }
                .livePageSurface()
            } else if loadFailed {
                ContentUnavailableView {
                    Label("Couldn't open this page", systemImage: "doc.richtext")
                } actions: { Button("Try again") { load() } }
            } else { ProgressView().padding(28) }
        }
        .background(Palette.background.ignoresSafeArea())
        .navigationTitle(item?.title ?? page.flatMap { PlanPageLayout(blocks: $0.blocks).title?.text } ?? String(localized: "Page"))
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
        .livePageChrome(onClose: onClose, status: {
            status.font(.caption).foregroundStyle(Palette.textSecondary)
        }) {
            if item != nil {
                Button { showVersions = true } label: { Label("Versions", systemImage: "clock.arrow.circlepath") }
                    .accessibilityIdentifier("plan-page.versions")
            }
            if let page, page.ready {
                ShareLink(item: shareText(page), preview: SharePreview(item?.title ?? String(localized: "Page"))) {
                    Label("Share", systemImage: "square.and.arrow.up")
                }
            }
        }
        .sheet(item: $editing) { value in
            NavigationStack {
                LineEditor(editing: value, onSave: { save($0) },
                           onRemove: value.lineId.map { id in { remove(lineId: id, observedVersion: editingVersion, observedHash: editingHash) } },
                           allowsChanges: page?.legacyEditAdmitted == true)
            }
            .zoenSheet([.height(value.lineId == nil ? 320 : 390)])
        }
        .sheet(isPresented: $showVersions) {
            if let item {
                NavigationStack { VersionsView(item: item, allowsRestore: page?.legacyEditAdmitted == true) }
                    .zoenSheet([.medium, .large])
            }
        }
        .task(id: model.revision) { load() }
        .sensoryFeedback(.success, trigger: bump)
    }

    @ViewBuilder private var status: some View {
        if let page {
            if page.unsaved {
                Label("Draft on this device", systemImage: "pencil")
                    .accessibilityIdentifier("plan-page.privateDraft")
            } else if page.pendingSync {
                Label("Saved on this device", systemImage: "icloud.and.arrow.up")
                    .accessibilityIdentifier("plan-page.pendingSync")
            } else if !page.legacyEditAdmitted {
                Label("Read only", systemImage: "lock")
            }
        }
    }

    @ViewBuilder private func blockView(_ block: PlanPageBlockDto, layout: PlanPageLayout, page: PlanPageDto) -> some View {
        switch block.content {
        case .title:
            richText(block, layout: layout).accessibilityAddTraits(.isHeader)
        case .summary:
            richText(block, layout: layout).foregroundStyle(Palette.textSecondary)
        case .section:
            HStack(alignment: .firstTextBaseline) {
                richText(block, layout: layout).accessibilityAddTraits(.isHeader)
                Spacer()
                if page.legacyEditAdmitted, let section = layout.sections[block.id] {
                    Button { beginEditing(.init(lineId: nil, section: section, text: "", amount: MoneyInput.text(for: 0, locale: AppLocale.locale))) } label: {
                        Label("Add", systemImage: "plus")
                    }
                    .font(.callout).buttonStyle(.borderless)
                    .accessibilityLabel(String(localized: "Add item to \(block.text)"))
                }
            }
            .padding(.top, 14)
        case .line:
            if let line = layout.line(block) {
                lineView(block, line: line, section: layout.legacySection(for: block), layout: layout, page: page)
            }
        case .prose(let kind):
            switch kind {
            case .divider: Divider().padding(.vertical, 8)
            case .heading: richText(block, layout: layout).accessibilityAddTraits(.isHeader).padding(.top, 8)
            case .bullet(let indent):
                markedText(block, layout: layout, marker: "•").padding(.leading, CGFloat(indent) * 24)
            case .numbered(let indent, let number):
                markedText(block, layout: layout, marker: "\(number).").padding(.leading, CGFloat(indent) * 24)
            case .task(let indent, let checked):
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: checked ? "checkmark.square.fill" : "square")
                        .foregroundStyle(checked ? Palette.action : Palette.textTertiary)
                        .accessibilityLabel(checked ? String(localized: "Completed") : String(localized: "Not completed"))
                    richText(block, layout: layout)
                }
                .padding(.leading, CGFloat(indent) * 24)
            case .quote:
                HStack(alignment: .top, spacing: 12) {
                    RoundedRectangle(cornerRadius: 2).fill(Palette.action).frame(width: 3)
                    richText(block, layout: layout)
                }
                .fixedSize(horizontal: false, vertical: true)
            case .code, .raw:
                ScrollView(.horizontal) { richText(block, layout: layout).fixedSize(horizontal: true, vertical: false) }
                    .padding(12).background(Palette.surfaceMuted, in: .rect(cornerRadius: 10))
            case .image(let address, let alt, let title):
                VStack(alignment: .leading, spacing: 6) {
                    if let url = URL(string: address), ["https", "http"].contains(url.scheme?.lowercased() ?? "") {
                        Link(destination: url) { Label(alt.isEmpty ? String(localized: "Image") : alt, systemImage: "photo") }
                    } else {
                        Label(alt.isEmpty ? String(localized: "Image") : alt, systemImage: "photo")
                    }
                    if !title.isEmpty { Text(title).font(.caption).foregroundStyle(Palette.textSecondary) }
                    if !block.text.isEmpty { Text(block.text).font(.caption) }
                }
            case .paragraph: richText(block, layout: layout)
            }
        }
    }

    private func richText(_ block: PlanPageBlockDto, layout: PlanPageLayout) -> Text {
        let display = layout.textBlock(block)
        let tag = BlockTag(display)
        let value = NSMutableAttributedString(attributedString: PageText.paragraph(tag, text: display.text, spans: display.spans, number: display.number))
        if tag.hasMarker && value.length > 0 { value.deleteCharacters(in: NSRange(location: 0, length: 1)) }
        value.enumerateAttribute(.font, in: NSRange(location: 0, length: value.length)) { attribute, range, _ in
            guard let font = attribute as? PlatformFont else { return }
            #if canImport(UIKit)
            let scaled = PlatformFont(descriptor: font.fontDescriptor, size: font.pointSize * textScale)
            #else
            let scaled = PlatformFont(descriptor: font.fontDescriptor, size: font.pointSize * textScale) ?? font
            #endif
            value.addAttribute(.font, value: scaled, range: range)
        }
        #if os(macOS)
        let attributed = try? AttributedString(value, including: \.appKit)
        #else
        let attributed = try? AttributedString(value, including: \.uiKit)
        #endif
        return Text(attributed ?? AttributedString(block.text))
    }

    private func markedText(_ block: PlanPageBlockDto, layout: PlanPageLayout, marker: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Text(marker).foregroundStyle(Palette.textSecondary).frame(minWidth: 18, alignment: .trailing)
            richText(block, layout: layout)
        }
    }

    private func lineView(_ block: PlanPageBlockDto, line: PlanLineDto, section: UInt32?, layout: PlanPageLayout, page: PlanPageDto) -> some View {
        let editable = page.legacyEditAdmitted && section != nil
        return HStack(alignment: .top, spacing: 8) {
            Group {
                if editable {
                    Button {
                        if admitted(version: page.version, contentHash: page.contentHash) {
                            apply(model.perform { try model.core.togglePlanLine(itemId: itemId, lineId: line.id) })
                        }
                    } label: { Image(systemName: line.done ? "checkmark.circle.fill" : "circle").font(.title3) }
                        .buttonStyle(.plain).frame(minWidth: 44, minHeight: 44)
                        .accessibilityLabel(line.done ? String(localized: "Reopen") : String(localized: "Complete"))
                } else {
                    Image(systemName: line.done ? "checkmark.circle.fill" : "circle")
                        .font(.title3).frame(minWidth: 44, minHeight: 44)
                        .accessibilityLabel(line.done ? String(localized: "Completed") : String(localized: "Not completed"))
                }
            }
            .foregroundStyle(line.done ? Palette.success : Palette.textTertiary)
            Group {
                if editable, let section {
                    Button {
                        beginEditing(.init(lineId: line.id, section: section, text: line.text,
                                           amount: MoneyInput.text(for: line.costCents, locale: AppLocale.locale)))
                    } label: { lineContent(block, line: line, layout: layout) }
                    .buttonStyle(.plain).accessibilityHint("Edit line")
                } else { lineContent(block, line: line, layout: layout) }
            }
            .frame(maxWidth: .infinity, alignment: .leading).frame(minHeight: 44)
        }
        .accessibilityIdentifier("plan-page.block.\(block.id)")
    }

    private func lineContent(_ block: PlanPageBlockDto, line: PlanLineDto, layout: PlanPageLayout) -> some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .firstTextBaseline, spacing: 14) {
                lineText(block, line: line, layout: layout)
                Spacer(minLength: 8)
                Text(Money.format(line.costCents)).monospacedDigit()
            }
            VStack(alignment: .leading, spacing: 6) {
                lineText(block, line: line, layout: layout)
                Text(Money.format(line.costCents)).monospacedDigit()
            }
        }
        .foregroundStyle(Palette.textPrimary)
    }

    private func lineText(_ block: PlanPageBlockDto, line: PlanLineDto, layout: PlanPageLayout) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            richText(block, layout: layout).strikethrough(line.done).multilineTextAlignment(.leading)
            if let item, page?.source == .documentSession, page?.unsaved != true,
               let request = layout.request(for: block, in: item) { RequestBadge(request: request) }
        }
    }

    @ViewBuilder private func budget(_ page: PlanPageDto, layout: PlanPageLayout) -> some View {
        HStack(spacing: 16) {
            if let total = page.totalCents, let limit = layout.budgetCents, limit > 0, total >= 0 {
                BudgetRing(spent: total, limit: limit, size: 60, lineWidth: 6, showsLabel: true)
            }
            VStack(alignment: .leading, spacing: 4) {
                if let total = page.totalCents {
                    Text(Money.format(total)).font(.title.weight(.bold)).monospacedDigit()
                    if let limit = layout.budgetCents {
                        let over = total > limit
                        if let difference = MoneyInput.differenceCents(over ? total : limit, over ? limit : total) {
                            Text(over ? String(localized: "\(Money.format(difference)) over the \(Money.format(limit)) cap") : String(localized: "of \(Money.format(limit)) · \(Money.format(difference)) left"))
                                .font(.callout).foregroundStyle(over ? Palette.danger : Palette.textSecondary)
                        } else { Text("of \(Money.format(limit))").font(.callout).foregroundStyle(Palette.textSecondary) }
                    }
                } else { Text("total unavailable").font(.title3).foregroundStyle(Palette.textSecondary) }
            }
            Spacer()
        }
        .padding(16).background(Palette.surfaceMuted, in: .rect(cornerRadius: 14))
        .padding(.vertical, 8).accessibilityIdentifier("plan-page.budget")
    }

    private func load() {
        do {
            page = try model.core.planPage(itemId: itemId)
            item = try? model.core.item(itemId: itemId)
            loadFailed = false
        } catch {
            page = nil
            item = nil
            loadFailed = true
        }
    }

    private func beginEditing(_ value: EditingLine) {
        guard let page, admitted(version: page.version, contentHash: page.contentHash) else { return }
        editingVersion = page.version
        editingHash = page.contentHash
        editing = value
    }

    private func admitted(version: UInt32?, contentHash: String?) -> Bool {
        guard let current = try? model.core.planPage(itemId: itemId), current.legacyEditAdmitted,
              version == current.version, contentHash == current.contentHash else {
            model.show(.init(kind: .info, text: String(localized: "The page changed. Review it before editing.")), seconds: 4)
            load()
            return false
        }
        return true
    }

    private func save(_ value: EditingLine) -> Bool {
        guard admitted(version: editingVersion, contentHash: editingHash), let cents = MoneyInput.cents(from: value.amount, locale: AppLocale.locale), cents >= 0 else { return false }
        let out: EditOutcome?
        if let id = value.lineId {
            out = model.perform { try model.core.editPlanLine(itemId: itemId, lineId: id, text: value.text, costCents: cents) }
        } else {
            out = model.perform { try model.core.addPlanLine(itemId: itemId, sectionIndex: value.section, text: value.text, costCents: cents) }
        }
        apply(out)
        return out != nil
    }

    private func remove(lineId: String, observedVersion: UInt32?, observedHash: String?) -> Bool {
        guard admitted(version: observedVersion, contentHash: observedHash) else { return false }
        let out = model.perform { try model.core.removePlanLine(itemId: itemId, lineId: lineId) }
        apply(out)
        return out != nil
    }

    private func apply(_ out: EditOutcome?) {
        guard let out else { return }
        bump += 1
        if let message = model.handleEdit(out) { reaction = message }
        load()
    }

    private func shareText(_ page: PlanPageDto) -> String {
        page.blocks.map { block in
            if case .line(_, let cents, let done) = block.content {
                return "\(done ? "✓" : "•") \(block.text) — \(Money.format(cents))"
            }
            return block.text
        }.joined(separator: "\n")
    }
}
