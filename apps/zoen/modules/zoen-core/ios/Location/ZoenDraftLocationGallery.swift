import UIKit

/// A read-only map in the same Gallery page and presentation lease as its caption.
/// The retained URI/hash comes from the existing journal, never coordinate props.
@MainActor final class ZoenDraftLocationGallery: UIView {
  private let map = ZoenLocationMap()
  private let error = UILabel()
  private let binding = UUID().uuidString
  private var point: ZoenStaticLocation.Point?
  private var retired = false

  init(url: URL?, source: ZoenDraftLocationSource, errorText: String) {
    super.init(frame: .zero)
    addSubview(map)
    // Gallery owns paging/dismissal gestures. Its map does not steal those gestures
    // or expose editing, GPS/search, another presenter, or any outgoing action.
    map.isUserInteractionEnabled = false
    error.text = errorText
    error.textColor = .white
    error.numberOfLines = 3
    error.textAlignment = .center
    error.accessibilityTraits = .updatesFrequently
    addSubview(error)
    if let url { point = try? source.read(url) }
    error.isHidden = point != nil
    map.isHidden = true
    accessibilityIdentifier = "attachment-gallery-location"
  }
  required init?(coder: NSCoder) { nil }
  override func layoutSubviews() {
    super.layoutSubviews()
    map.frame = bounds
    error.frame = bounds.insetBy(dx: 24, dy: 24)
  }
  func setActive(_ active: Bool) {
    guard !retired else { return }
    guard active, let point, let data = try? JSONEncoder().encode(point),
          let value = try? JSONSerialization.jsonObject(with: data),
          let configuration = try? JSONSerialization.data(withJSONObject: [
            "binding": binding, "editable": false, "point": value
          ] as [String: Any]) else {
      map.retire(); map.isHidden = true; return
    }
    map.configurationJson = String(decoding: configuration, as: UTF8.self)
    map.configure()
    map.isHidden = false
  }
  func retire() {
    retired = true; point = nil; map.retire(); map.isHidden = true; error.text = nil
  }
}
