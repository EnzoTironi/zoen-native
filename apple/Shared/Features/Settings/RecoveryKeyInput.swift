import Foundation

enum RecoveryKeyInput {
    static func digits(_ value: String) -> String { String(value.filter { $0 >= "0" && $0 <= "9" }) }
    static func valid(_ value: String) -> Bool {
        digits(value).count == 64 && value.unicodeScalars.allSatisfy {
            (48...57).contains($0.value) || [9, 10, 11, 12, 13, 32].contains($0.value)
        }
    }
    static func matches(_ key: String, _ confirmation: String) -> Bool {
        valid(key) && valid(confirmation) && digits(key) == digits(confirmation)
    }
    static func validRelay(_ value: String) -> Bool {
        guard let url = URLComponents(string: value.trimmingCharacters(in: .whitespacesAndNewlines)),
              let host = url.host, url.user == nil, url.password == nil,
              url.query == nil, url.fragment == nil, url.path.isEmpty || url.path == "/" else { return false }
        return url.scheme == "https" || (url.scheme == "http" && ["localhost", "127.0.0.1", "::1", "[::1]"].contains(host))
    }
}
