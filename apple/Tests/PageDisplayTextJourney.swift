import Foundation

private enum JourneyError: Error { case failed(String) }

@main
struct PageDisplayTextJourney {
    static func check(_ condition: Bool, _ message: String) throws {
        if !condition { throw JourneyError.failed(message) }
    }

    static func main() {
        do { try journey() } catch {
            FileHandle.standardError.write(Data("FAIL \(error)\n".utf8))
            exit(1)
        }
    }

    private static func report(_ message: String) {
        FileHandle.standardOutput.write(Data((message + "\n").utf8))
    }

    static func journey() throws {
        let exact = "A\r\nB\nC\rD\u{85}E\u{2029}F\u{2028}G\0👩🏽‍💻 雨 e\u{301}"
        let projection = PageDisplayText(exact)
        let display = "A\u{2028}B\u{2028}C\u{2028}D\u{2028}E\u{2028}F\u{2028}G\0👩🏽‍💻 雨 e\u{301}"
        try check(projection.text.utf8.elementsEqual(display.utf8), "Display changed non-break text")
        try check(projection.sourceText(afterEditing: projection.text).utf8.elementsEqual(exact.utf8),
                  "An unchanged display rewrote legacy text")
        report("PASS mixed legacy breaks, NUL, emoji and combining text keep exact source")

        let input = "Book\r\nCasa Azul 👩🏽‍💻"
        let block = PageDisplayText(input)
        let selected = (input as NSString).range(of: "Casa Azul 👩🏽‍💻")
        let visible = (block.text as NSString).range(of: "Casa Azul 👩🏽‍💻")
        try check(block.displayRange(forSource: selected) == visible, "Selection moved after CRLF")
        try check(block.sourceRange(forDisplay: visible) == selected, "Formatting mapped to another source range")
        try check(block.sourceText(afterEditing: block.text).utf8.elementsEqual(input.utf8),
                  "A formatting-only read changed approval text")
        report("PASS formatting selection crosses display projection without changing approval text")

        try check(block.displayRange(forSource: NSRange(location: 4, length: 2)) == NSRange(location: 4, length: 1),
                  "CRLF was not shown as one line break")
        try check(block.sourceRange(forDisplay: NSRange(location: 4, length: 1)) == NSRange(location: 4, length: 2),
                  "The displayed break lost its exact CRLF")
        try check(block.displayRange(forSource: NSRange(location: 5, length: 0)) == nil,
                  "A selection inside CRLF was silently rounded")
        report("PASS CRLF round trip and refusal of an ambiguous boundary")

        let emoji = (input as NSString).range(of: "👩🏽‍💻")
        try check(emoji.location != NSNotFound, "The emoji selection is missing")
        try check(block.displayRange(forSource: NSRange(location: emoji.location + 1, length: 0)) == nil,
                  "A source range split a surrogate pair")
        let renderedEmoji = (block.text as NSString).range(of: "👩🏽‍💻")
        try check(renderedEmoji.location != NSNotFound, "The displayed emoji selection is missing")
        try check(block.sourceRange(forDisplay: NSRange(location: renderedEmoji.location + 1, length: 0)) == nil,
                  "A displayed range split a surrogate pair")
        report("PASS UTF16 selections cannot split a Unicode scalar")

        let nfd = PageDisplayText("Cafe\u{301}")
        let nfc = "Café"
        try check(nfd.text == nfc && !nfd.text.utf8.elementsEqual(nfc.utf8), "Fixture is not canonically equivalent")
        try check(nfd.sourceText(afterEditing: nfc).utf8.elementsEqual(nfc.utf8),
                  "A real Unicode representation edit was discarded")
        report("PASS actual NFC/NFD edits remain changes")

        let edited = block.text + " on Friday"
        try check(block.sourceText(afterEditing: edited).utf8.elementsEqual(edited.utf8), "A text edit was lost")
        report("PASS changed text becomes the new source")

        for range in [NSRange(location: NSNotFound, length: 1),
                      NSRange(location: 0, length: Int.max), NSRange(location: 500, length: 0)] {
            try check(block.displayRange(forSource: range) == nil && block.sourceRange(forDisplay: range) == nil,
                      "An out-of-bounds selection was accepted")
        }
        let empty = PageDisplayText("")
        try check(empty.sourceRange(forDisplay: NSRange(location: 0, length: 0)) == NSRange(location: 0, length: 0),
                  "An empty block cannot hold a caret")
        report("PASS empty blocks and invalid range refusal")

        let joined = (projection.text + "\nAnother block") as NSString
        var count = 0, location = 0
        while location < joined.length {
            let range = joined.paragraphRange(for: NSRange(location: location, length: 0))
            location = NSMaxRange(range)
            count += 1
        }
        try check(count == 2, "Legacy line breaks created extra editor blocks")
        report("PASS one legacy block stays one TextKit paragraph")
    }
}
