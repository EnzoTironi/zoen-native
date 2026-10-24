import Foundation

struct ZoenLocationMapConfiguration: Decodable {
  let binding: String
  let editable: Bool
  let point: ZoenStaticLocation.Point?
  let accessibilityLabel: String?
  let selectPointLabel: String?
  static func decode(_ json: String) throws -> Self {
    guard json.utf8.count <= 4096 else { throw ZoenStaticLocation.Failure.invalid }
    let value = try JSONDecoder().decode(Self.self, from: Data(json.utf8))
    guard ZoenLocationLifetime.validBinding(value.binding) else { throw ZoenStaticLocation.Failure.invalid }
    for label in [value.accessibilityLabel, value.selectPointLabel].compactMap({ $0 }) {
      guard !label.isEmpty, label.utf8.count <= 256,
            !ZoenStaticLocation.hasControls(label) else { throw ZoenStaticLocation.Failure.invalid }
    }
    _ = try value.point?.validated()
    return value
  }
}
