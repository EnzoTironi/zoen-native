import Foundation

@main
struct RecoveryKeyInputJourney {
    static func main() {
        // Deliberately non-secret fixtures, never a generated recovery key.
        let key = String(repeating: "1", count: 64)
        let other = String(repeating: "2", count: 64)
        precondition(RecoveryKeyInput.valid(key))
        precondition(RecoveryKeyInput.matches(key, key.chunked()))
        precondition(!RecoveryKeyInput.matches(key, other))
        for bad in ["", String(key.dropLast()), key + "1", key + "x", key + "-", String(repeating: "١", count: 64)] {
            precondition(!RecoveryKeyInput.valid(bad))
            precondition(!RecoveryKeyInput.matches(key, bad))
        }
        precondition(RecoveryKeyInput.validRelay("https://relay.example"))
        precondition(RecoveryKeyInput.validRelay("http://127.0.0.1:8787"))
        for bad in ["http://relay.example", "https://user:pass@relay.example", "https://relay.example/path", "https://relay.example?key=value"] {
            precondition(!RecoveryKeyInput.validRelay(bad))
        }
        print("PASS: confirmation, malformed/wrong keys, relay validation")
    }
}

private extension String {
    func chunked() -> String {
        stride(from: 0, to: count, by: 4).map { offset in
            let start = index(startIndex, offsetBy: offset)
            return String(self[start..<index(start, offsetBy: 4)])
        }.joined(separator: " ")
    }
}
