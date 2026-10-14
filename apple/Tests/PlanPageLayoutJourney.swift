import Foundation
import RodaCore

private struct LayoutFailure: Error { let message: String }

@main
struct PlanPageLayoutJourney {
    static func main() throws {
        var passed = 0
        func expect(_ value: Bool, _ message: String) throws {
            guard value else { throw LayoutFailure(message: message) }
            passed += 1
            print("PASS \(message)")
        }

        let exact = "Cafe\u{301}\r\n👩🏽‍💻 雨\0"
        let mark = TextSpanDto(start: 7, end: 14, key: "b", value: "")
        let blocks: [PlanPageBlockDto] = [
            .init(id: "title", content: .title(budgetCents: Int64.max), text: "Trip", spans: []),
            .init(id: "before-section", content: .line(legacyId: "orphan", costCents: -1, done: false), text: "Keep this row", spans: []),
            .init(id: "section-a", content: .section, text: "Stay", spans: []),
            .init(id: "first-row", content: .line(legacyId: "duplicate", costCents: Int64.min, done: true), text: exact, spans: [mark]),
            .init(id: "second-row", content: .line(legacyId: "duplicate", costCents: Int64.max, done: false), text: "Another item", spans: []),
            .init(id: "section-b", content: .section, text: "Transport", spans: []),
            .init(id: "third-row", content: .line(legacyId: "other", costCents: 1, done: false), text: "One cent", spans: []),
            .init(id: "image", content: .prose(kind: .image(url: "https://example.com/photo.png", alt: exact, title: "Photo title")), text: "Caption", spans: []),
            .init(id: "heading", content: .prose(kind: .heading(level: 2, setext: true)), text: exact, spans: [mark]),
        ]
        let layout = PlanPageLayout(blocks: blocks)
        try expect(layout.blocks.map(\.id) == blocks.map(\.id), "Document order retains both duplicate legacy IDs under their own block IDs")
        try expect(layout.legacySection(for: blocks[3]) == 0 && layout.legacySection(for: blocks[4]) == nil,
                   "The second duplicate cannot edit the first row through the legacy API")
        try expect(layout.legacySection(for: blocks[6]) == 1 && layout.legacySection(for: blocks[1]) == nil,
                   "Section addresses do not attach an orphan row to an unrelated section")
        let line = try unwrap(layout.line(blocks[3]))
        try expect(line.costCents == Int64.min && line.done && line.text.utf8.elementsEqual(exact.utf8),
                   "Imported credit, completion and exact UTF8 text survive presentation")
        try expect(layout.line(blocks[4])?.costCents == Int64.max && layout.budgetCents == Int64.max,
                   "The largest integer cost and budget never pass through Double")
        let text = layout.textBlock(blocks[3])
        try expect(text.id == "first-row" && text.text.utf8.elementsEqual(exact.utf8) && text.spans == [mark],
                   "Rich text retains raw CRLF, combining Unicode, NUL and original UTF16 mark offsets")
        let image = layout.textBlock(blocks[7])
        try expect(image.url == "https://example.com/photo.png" && image.alt.utf8.elementsEqual(exact.utf8)
                   && layout.blocks[7].content == blocks[7].content,
                   "Image display preserves the original title and alt instead of rewriting the typed block")
        try expect(layout.textBlock(blocks[8]).level == 2 && layout.blocks[8].content == blocks[8].content,
                   "A displayed heading keeps its original setext identity in the document")

        let core = try RodaEngine.open(path: ":memory:", locale: "pt-BR")
        _ = try core.seedDemoIfEmpty()
        let result = try core.universalSearch(query: "Paraty", kinds: ["item"], limit: 20)
        let itemId = try unwrap(result.hits.first(where: { $0.kind == "item" })?.refId)
        let page = try core.planPage(itemId: itemId)
        try expect(page.ready && page.source == .documentSession && page.legacyEditAdmitted
                   && !page.unsaved && !page.pendingSync,
                   "The current generated SDK decodes the saved Plan source and actual legacy admission")
        let actual = PlanPageLayout(blocks: page.blocks)
        let detail = try core.item(itemId: itemId)
        let legacyLines = detail.plan?.sections.flatMap(\.lines) ?? []
        let shownLines = actual.blocks.compactMap { actual.line($0) }
        try expect(shownLines.count == legacyLines.count && shownLines.map(\.costCents) == legacyLines.map(\.costCents),
                   "The actual generated Swift/Core read renders every seeded priced row with exact costs")
        try expect(actual.title?.text.utf8.elementsEqual(detail.title.utf8) == true && page.totalCents == detail.plan?.totalCents,
                   "The actual document read agrees with the existing Plan title and total")

        // Journey: the version sheet captures the binding when it opens, the plan changes
        // underneath it, and restoring with the captured hash is refused without appending.
        let sheetHash = try core.planPage(itemId: itemId).contentHash
        let editLine = try unwrap(legacyLines.first)
        _ = try core.editPlanLine(itemId: itemId, lineId: editLine.id, text: "Changed under the sheet",
                                  costCents: editLine.costCents, observedHash: sheetHash)
        let changed = try core.item(itemId: itemId)
        var refusedAsStale = false
        do {
            _ = try core.restorePlanVersion(itemId: itemId, version: 1, observedHash: sheetHash)
        } catch let error as CoreError {
            if case .Stale = error { refusedAsStale = true }
        }
        let after = try core.item(itemId: itemId)
        try expect(refusedAsStale, "Restoring with the hash seen when the sheet opened is refused as stale")
        try expect(after.version == changed.version && after.versions.count == changed.versions.count,
                   "A refused restore appends no version")
        let fresh = try core.planPage(itemId: itemId).contentHash
        let restored = try core.restorePlanVersion(itemId: itemId, version: 1, observedHash: fresh)
        try expect(restored.version == changed.version + 1, "Re-reading the binding gives the person a way forward")
        print("RESULT \(passed) passed; 0 failed")
    }

    private static func unwrap<T>(_ value: T?) throws -> T {
        guard let value else { throw LayoutFailure(message: "Missing journey fixture") }
        return value
    }
}
