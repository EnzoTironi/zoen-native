import Foundation
import RodaCore

/// The document order and CRDT identity stay separate from the old mutation API's row IDs.
struct PlanPageLayout {
    let blocks: [PlanPageBlockDto]
    private(set) var sections: [String: UInt32] = [:]
    private(set) var firstLineBlocks: [String: String] = [:]

    init(blocks: [PlanPageBlockDto]) {
        self.blocks = blocks
        var nextSection: UInt32 = 0
        var section: UInt32?
        for block in blocks {
            switch block.content {
            case .section:
                section = nextSection
                nextSection += 1
                sections[block.id] = section
            case .line(let legacyId, _, _):
                if firstLineBlocks[legacyId] == nil { firstLineBlocks[legacyId] = block.id }
                sections[block.id] = section
            default: break
            }
        }
    }

    var title: PlanPageBlockDto? {
        blocks.first { if case .title = $0.content { true } else { false } }
    }

    var budgetCents: Int64? {
        guard let title, case .title(let budget) = title.content else { return nil }
        return budget
    }

    func line(_ block: PlanPageBlockDto) -> PlanLineDto? {
        guard case .line(let id, let cost, let done) = block.content else { return nil }
        return PlanLineDto(id: id, text: block.text, costCents: cost, done: done)
    }

    func legacySection(for block: PlanPageBlockDto) -> UInt32? {
        guard case .line(let id, _, _) = block.content, firstLineBlocks[id] == block.id else { return nil }
        return sections[block.id]
    }

    func request(for block: PlanPageBlockDto, in item: ItemDetail) -> AgentRequestDto? {
        guard let line = line(block), firstLineBlocks[line.id] == block.id,
              let saved = item.plan?.sections.lazy.flatMap(\.lines).first(where: { $0.id == line.id }),
              saved.costCents == line.costCents, saved.text.utf8.elementsEqual(line.text.utf8) else { return nil }
        return item.linkedRequests.first { $0.lineId == line.id }
    }

    func textBlock(_ block: PlanPageBlockDto) -> PageBlockDto {
        var kind = "paragraph"
        var level: UInt32 = 0, indent: UInt32 = 0, number: UInt32 = 0
        var checked = false
        var lang = "", url = "", alt = ""
        switch block.content {
        case .title: kind = "heading"; level = 1
        case .section: kind = "heading"; level = 2
        case .summary, .line: break
        case .prose(let prose):
            switch prose {
            case .paragraph: break
            case .heading(let value, _): kind = "heading"; level = value
            case .bullet(let value): kind = "bullet"; indent = value
            case .numbered(let value, let start): kind = "numbered"; indent = value; number = start
            case .task(let value, let done): kind = "task"; indent = value; checked = done
            case .quote: kind = "quote"
            case .code(let language): kind = "code"; lang = language
            case .divider: kind = "divider"
            case .image(let address, let description, _): kind = "image"; url = address; alt = description
            case .raw: kind = "raw"
            }
        }
        return PageBlockDto(id: block.id, kind: kind, level: level, indent: indent, number: number,
                            checked: checked, lang: lang, url: url, alt: alt, text: block.text, spans: block.spans)
    }
}
