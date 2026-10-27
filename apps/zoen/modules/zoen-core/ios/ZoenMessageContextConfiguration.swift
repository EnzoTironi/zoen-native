import ExpoModulesCore
import UIKit

struct ZoenMessageContextConfiguration: Decodable {
  struct Action: Decodable {
    let id: String
    let label: String
    var symbol: String? {
      switch id {
      case "reply": "arrowshape.turn.up.left"
      case "thread": "bubble.left.and.bubble.right"
      case "select": "text.cursor"
      case "copy": "doc.on.doc"
      case "edit": "pencil"
      case "recover": "arrow.clockwise"
      case "forward": "arrowshape.turn.up.right"
      case "share": "square.and.arrow.up"
      case "delete": "trash"
      default: nil
      }
    }
  }
  struct Reaction: Decodable { let emoji: String; let label: String; let selected: Bool }
  struct Theme: Decodable {
    let background: String, highlight: String, text: String, secondary: String
    let separator: String, destructive: String, dim: String
    let isDark: Bool
  }
  let actions: [Action]
  let reactions: [Reaction]
  let moreLabel: String?
  let theme: Theme
  let reducedMotion: Bool
  let incoming: Bool
  static let emoji = Set(["👍", "❤️", "😂", "😮", "😢", "🙏"])
  static func decode(_ json: String) throws -> Self {
    guard let data = json.data(using: .utf8), data.count <= 16_384 else { throw ZoenMessageContextSource.Failure.configuration }
    let config = try JSONDecoder().decode(Self.self, from: data)
    guard config.actions.count <= 8, config.reactions.count <= 6,
          !config.actions.isEmpty || !config.reactions.isEmpty,
          Set(config.actions.map(\.id)).count == config.actions.count,
          Set(config.reactions.map(\.emoji)).count == config.reactions.count,
          config.actions.allSatisfy({ $0.symbol != nil && label($0.label) }),
          config.reactions.allSatisfy({ emoji.contains($0.emoji) && label($0.label) }),
          config.moreLabel.map(label) ?? true,
          [config.theme.background, config.theme.highlight, config.theme.text, config.theme.secondary,
           config.theme.separator, config.theme.destructive, config.theme.dim].allSatisfy({ UIColor.contextHex($0) != nil }) else {
      throw ZoenMessageContextSource.Failure.configuration
    }
    return config
  }
  private static func label(_ value: String) -> Bool {
    !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && value.utf8.count <= 256
  }
}


extension CGRect {
  var contextFinite: Bool { [minX, minY, width, height].allSatisfy(\.isFinite) && width > 0 && height > 0 }
}
extension UIColor {
  static func contextHex(_ value: String) -> UIColor? {
    guard value.first == "#", [7, 9].contains(value.count), let number = UInt64(value.dropFirst(), radix: 16) else { return nil }
    let alpha = value.count == 9 ? CGFloat(number & 255) / 255 : 1
    let rgb = value.count == 9 ? number >> 8 : number
    return UIColor(red: CGFloat((rgb >> 16) & 255) / 255, green: CGFloat((rgb >> 8) & 255) / 255, blue: CGFloat(rgb & 255) / 255, alpha: alpha)
  }
}
