import Photos
import UIKit

/// Native presentation of the existing PhotoSession's exact query and selection.
/// IDs are not a second Photos query/selection authority. Only local thumbnails
/// are requested here; CURRENT bytes/copies remain in the existing JS adapter.
@MainActor
final class ZoenAttachmentKeyboardContent: NSObject, ZoenKeyboardPanelContent, UICollectionViewDataSource, UICollectionViewDelegate {
  let view = UIView()
  private let send: (String, [String: Any]) -> Void
  private let layout = PhotoLayout()
  private lazy var grid = UICollectionView(frame: .zero, collectionViewLayout: layout)
  private let header = UIView()
  private let footer = UIView()
  private let keyboard = UIButton(type: .system)
  private let albums = UIButton(type: .system)
  private let review = UIButton(type: .system)
  private let manage = UIButton(type: .system)
  private let message = UILabel()
  private let settings = UIButton(type: .system)
  private let retry = UIButton(type: .system)
  private let activity = UIActivityIndicatorView(style: .medium)
  private let manager = PHCachingImageManager()
  private var items: [[String: Any]] = []
  private var selected: [String] = []
  private var labels: [String: String] = [:]
  private var buttons: [UIButton] = []
  private var busy = false
  private var retired = false
  private var granted = false
  private var limited = false
  private var loading = false
  private var hasMore = false
  private var hasError = false
  private var hasAudio = false
  private var glass = false
  private var bottomInset: CGFloat = 0
  private var requestedCount: Int?
  private var appearance: ZoenKeyboardPanelAppearance?

  init(send: @escaping (String, [String: Any]) -> Void) {
    self.send = send
    super.init()
    view.accessibilityIdentifier = "attachment-keyboard-panel"
    grid.register(PhotoCell.self, forCellWithReuseIdentifier: "photo")
    grid.dataSource = self; grid.delegate = self
    grid.alwaysBounceVertical = true
    grid.keyboardDismissMode = .none
    grid.accessibilityIdentifier = "attachment-keyboard-photos"
    grid.contentInsetAdjustmentBehavior = .never
    view.addSubview(grid)
    for child in [header, footer, manage, message, settings, retry, activity] { view.addSubview(child) }
    for child in [keyboard, albums, review] { header.addSubview(child) }
    keyboard.setImage(UIImage(systemName: "keyboard"), for: .normal)
    keyboard.accessibilityIdentifier = "attachment-keyboard-system"
    keyboard.addAction(UIAction { [weak self] _ in self?.emit("keyboard") }, for: .touchUpInside)
    review.setImage(UIImage(systemName: "arrow.right.circle.fill"), for: .normal)
    review.accessibilityIdentifier = "attachment-keyboard-review"
    review.addAction(UIAction { [weak self] _ in self?.emit("review") }, for: .touchUpInside)
    albums.showsMenuAsPrimaryAction = true
    albums.titleLabel?.font = .systemFont(ofSize: 17, weight: .semibold)
    manage.titleLabel?.font = .systemFont(ofSize: 14)
    manage.addAction(UIAction { [weak self] _ in self?.emit("manage") }, for: .touchUpInside)
    settings.addAction(UIAction { [weak self] _ in self?.emit("settings") }, for: .touchUpInside)
    retry.titleLabel?.font = .systemFont(ofSize: 13)
    retry.addAction(UIAction { [weak self] _ in self?.emit("retry") }, for: .touchUpInside)
    message.numberOfLines = 0
    message.textAlignment = .center
    message.font = .preferredFont(forTextStyle: .body)
    message.adjustsFontForContentSizeCategory = true
    let container = LayoutView()
    container.layout = { [weak self] in self?.reflow() }
    view.addSubview(container)
    container.isUserInteractionEnabled = false
    container.autoresizingMask = [.flexibleWidth, .flexibleHeight]
  }
  func update(model: [String: Any], appearance: ZoenKeyboardPanelAppearance) throws {
    guard !retired, let items = model["items"] as? [[String: Any]], items.count <= 20_000,
          let selected = model["selected"] as? [String], selected.count <= 100,
          let labels = model["labels"] as? [String: String],
          items.allSatisfy({ ($0["id"] as? String)?.hasPrefix("ph://") == true }),
          Set(items.compactMap { $0["id"] as? String }).count == items.count,
          Set(selected).count == selected.count else { throw Failure.invalid }
    if items.count != self.items.count { requestedCount = nil }
    self.items = items; self.selected = selected; self.labels = labels; self.appearance = appearance
    busy = model["busy"] as? Bool == true
    granted = model["granted"] as? Bool == true
    limited = model["limited"] as? Bool == true
    loading = model["loading"] as? Bool == true
    hasMore = model["hasMore"] as? Bool == true
    hasError = model["error"] as? Bool == true
    hasAudio = model["hasAudio"] as? Bool == true
    glass = model["glass"] as? Bool == true
    bottomInset = appearance.bottomInset
    layout.columns = model["columns"] as? Int == 5 ? 5 : 3
    view.backgroundColor = appearance.background
    grid.backgroundColor = appearance.background
    header.backgroundColor = appearance.background
    footer.backgroundColor = appearance.background
    keyboard.tintColor = appearance.text; keyboard.accessibilityLabel = labels["keyboard"]
    albums.tintColor = appearance.text; albums.setTitle(model["albumTitle"] as? String, for: .normal)
    var albumActions: [UIAction] = []
    albumActions.append(UIAction(title: labels["recents"] ?? "", state: model["albumId"] is NSNull ? .on : .off) { [weak self] _ in self?.emit("album", id: "") })
    for album in model["albums"] as? [[String: String]] ?? [] {
      guard let id = album["id"], let title = album["title"] else { continue }
      albumActions.append(UIAction(title: title, state: model["albumId"] as? String == id ? .on : .off) { [weak self] _ in self?.emit("album", id: id) })
    }
    albums.menu = UIMenu(children: albumActions)
    review.tintColor = appearance.accent; review.accessibilityLabel = labels["review"]
    review.isEnabled = !busy && !selected.isEmpty
    keyboard.isEnabled = !busy; albums.isEnabled = !busy
    manage.setTitle(labels["manage"], for: .normal); manage.tintColor = appearance.accent
    manage.isEnabled = !busy
    message.text = labels["permission"]; message.textColor = appearance.text
    settings.setTitle(labels["settings"], for: .normal); settings.tintColor = appearance.accent
    retry.setTitle(labels["retry"], for: .normal); retry.tintColor = appearance.accent
    activity.color = appearance.secondary
    if loading || busy { activity.startAnimating() } else { activity.stopAnimating() }
    rebuildFooter()
    grid.reloadData()
    layout.invalidateLayout()
    reflow()
  }
  private func rebuildFooter() {
    for button in buttons { button.removeFromSuperview() }
    buttons.removeAll()
    if !selected.isEmpty {
      let button = UIButton(type: .system)
      button.setTitle(labels[busy ? "preparing" : "review"], for: .normal)
      button.titleLabel?.font = .systemFont(ofSize: 17, weight: .semibold)
      button.tintColor = appearance?.accent
      button.accessibilityIdentifier = "attachment-keyboard-review-selection"
      button.isEnabled = !busy
      button.addAction(UIAction { [weak self] _ in self?.emit("review") }, for: .touchUpInside)
      footer.addSubview(button); buttons.append(button)
      return
    }
    let choices = [("photos", "photo.on.rectangle", "photos"), ("document", "doc", "files")] + (hasAudio ? [("audio", "waveform", "audio")] : [])
    for (action, symbol, title) in choices {
      let button = UIButton(type: .system)
      var config = UIButton.Configuration.plain()
      config.image = UIImage(systemName: symbol, withConfiguration: UIImage.SymbolConfiguration(pointSize: 26, weight: .regular))
      config.imagePlacement = .top; config.imagePadding = 3
      config.title = labels[title]
      config.titleTextAttributesTransformer = UIConfigurationTextAttributesTransformer { attributes in
        var result = attributes; result.font = .systemFont(ofSize: 10); return result
      }
      button.configuration = config
      button.tintColor = action == "photos" ? appearance?.accent : appearance?.secondary
      button.accessibilityIdentifier = "attachment-keyboard-\(action)"
      button.isEnabled = !busy
      button.addAction(UIAction { [weak self] _ in if action != "photos" { self?.emit(action) } }, for: .touchUpInside)
      footer.addSubview(button); buttons.append(button)
    }
  }
  private func reflow() {
    let width = view.bounds.width, height = view.bounds.height
    guard width > 0, height > 0 else { return }
    let bar: CGFloat = glass ? 62 : 49
    header.frame = CGRect(x: 0, y: 0, width: width, height: 44)
    keyboard.frame = CGRect(x: 0, y: 0, width: 44, height: 44)
    review.frame = CGRect(x: width - 44, y: 0, width: 44, height: 44)
    albums.frame = CGRect(x: 44, y: 0, width: max(0, width - 88), height: 44)
    let manageHeight: CGFloat = limited ? 36 : 0
    manage.isHidden = !limited
    manage.frame = CGRect(x: 8, y: 44, width: width - 16, height: manageHeight)
    footer.frame = CGRect(x: 0, y: max(44 + manageHeight, height - bar - bottomInset), width: width, height: bar)
    grid.frame = CGRect(x: 0, y: 44 + manageHeight, width: width, height: max(0, footer.frame.minY - 44 - manageHeight))
    grid.isHidden = !granted
    message.isHidden = granted || loading
    settings.isHidden = granted || loading
    message.frame = grid.frame.insetBy(dx: 20, dy: 4)
    message.frame.size.height = max(0, message.frame.height - 44)
    settings.frame = CGRect(x: 20, y: grid.frame.maxY - 44, width: width - 40, height: 44)
    let total = max(1, buttons.count)
    for (index, button) in buttons.enumerated() {
      button.frame = CGRect(x: width * CGFloat(index) / CGFloat(total), y: 0, width: width / CGFloat(total), height: bar)
    }
    retry.isHidden = !hasError
    retry.frame = CGRect(x: 8, y: grid.frame.minY, width: width - 16, height: 44)
    activity.center = CGPoint(x: width / 2, y: grid.frame.midY)
  }
  private func emit(_ action: String, id: String? = nil) {
    guard !retired, !busy else { return }
    send(action, id.map { ["id": $0] } ?? [:])
  }
  func collectionView(_ collectionView: UICollectionView, numberOfItemsInSection section: Int) -> Int { granted ? items.count + 1 : 0 }
  func collectionView(_ collectionView: UICollectionView, cellForItemAt indexPath: IndexPath) -> UICollectionViewCell {
    guard let cell = collectionView.dequeueReusableCell(withReuseIdentifier: "photo", for: indexPath) as? PhotoCell else { return UICollectionViewCell() }
    if indexPath.item == 0 {
      cell.camera(label: labels["camera"] ?? "", appearance: appearance)
    } else {
      let item = items[indexPath.item - 1]
      let id = item["id"] as? String ?? ""
      cell.photo(item: item, order: selected.firstIndex(of: id).map { $0 + 1 }, manager: manager, appearance: appearance) { [weak self, weak cell] in
        guard cell?.identifier == id else { return }; self?.emit("select", id: id)
      }
    }
    return cell
  }
  func collectionView(_ collectionView: UICollectionView, didSelectItemAt indexPath: IndexPath) {
    if indexPath.item == 0 { emit("camera") }
    else if items.indices.contains(indexPath.item - 1), let id = items[indexPath.item - 1]["id"] as? String { emit("preview", id: id) }
  }
  func collectionView(_ collectionView: UICollectionView, didEndDisplaying cell: UICollectionViewCell, forItemAt indexPath: IndexPath) { (cell as? PhotoCell)?.cancel() }
  func scrollViewDidScroll(_ scrollView: UIScrollView) {
    guard hasMore, !loading, !busy, !retired, requestedCount != items.count,
          scrollView.contentOffset.y + scrollView.bounds.height >= scrollView.contentSize.height - scrollView.bounds.height * 0.6 else { return }
    requestedCount = items.count
    emit("more")
  }
  func retire() {
    retired = true
    view.isUserInteractionEnabled = false
    for cell in grid.visibleCells { (cell as? PhotoCell)?.cancel() }
    manager.stopCachingImagesForAllAssets()
    grid.delegate = nil; grid.dataSource = nil
    items.removeAll(); selected.removeAll(); grid.reloadData()
  }
  private enum Failure: Error { case invalid }
}

@MainActor
private final class LayoutView: UIView {
  var layout: (() -> Void)?
  override func layoutSubviews() { super.layoutSubviews(); layout?() }
}
@MainActor
private final class PhotoLayout: UICollectionViewLayout {
  var columns = 3
  private var values: [UICollectionViewLayoutAttributes] = []
  private var contentHeight: CGFloat = 0
  override func prepare() {
    super.prepare()
    guard let collectionView else { return }
    let width = collectionView.bounds.width
    let scale = max(1, collectionView.traitCollection.displayScale)
    let size = floor((width - CGFloat(columns - 1)) / CGFloat(columns) * scale) / scale
    guard size > 0 else { values = []; contentHeight = 0; return }
    let count = collectionView.numberOfItems(inSection: 0), firstCount = (columns - 1) * 2
    values = (0..<count).map { index in
      let value = UICollectionViewLayoutAttributes(forCellWith: IndexPath(item: index, section: 0))
      if index == 0 { value.frame = CGRect(x: 0, y: 0, width: size, height: size * 2 + 1) }
      else {
        let photo = index - 1
        let row = photo < firstCount ? photo / (columns - 1) : 2 + (photo - firstCount) / columns
        let column = photo < firstCount ? 1 + photo % (columns - 1) : (photo - firstCount) % columns
        value.frame = CGRect(x: CGFloat(column) * (size + 1), y: CGFloat(row) * (size + 1), width: size, height: size)
      }
      return value
    }
    contentHeight = values.map(\.frame.maxY).max() ?? 0
  }
  override var collectionViewContentSize: CGSize { CGSize(width: collectionView?.bounds.width ?? 0, height: contentHeight) }
  override func layoutAttributesForElements(in rect: CGRect) -> [UICollectionViewLayoutAttributes]? { values.filter { $0.frame.intersects(rect) } }
  override func layoutAttributesForItem(at indexPath: IndexPath) -> UICollectionViewLayoutAttributes? { values.indices.contains(indexPath.item) ? values[indexPath.item] : nil }
  override func shouldInvalidateLayout(forBoundsChange newBounds: CGRect) -> Bool { newBounds.width != collectionView?.bounds.width }
}

@MainActor
private final class PhotoCell: UICollectionViewCell {
  let image = UIImageView()
  let check = UIButton(type: .custom)
  private let badge = UIView()
  let duration = UILabel()
  private let cameraIcon = UIImageView(image: UIImage(systemName: "camera.fill"))
  private var request: PHImageRequestID?
  private weak var manager: PHImageManager?
  private var callback: (() -> Void)?
  private(set) var identifier: String?
  override init(frame: CGRect) {
    super.init(frame: frame)
    image.contentMode = .scaleAspectFill; image.clipsToBounds = true
    duration.textColor = .white; duration.font = .systemFont(ofSize: 12, weight: .semibold)
    duration.layer.shadowColor = UIColor.black.cgColor; duration.layer.shadowOpacity = 0.6; duration.layer.shadowRadius = 2
    check.titleLabel?.font = .systemFont(ofSize: 14, weight: .semibold)
    badge.layer.borderColor = UIColor.white.cgColor; badge.layer.cornerRadius = 12
    badge.isUserInteractionEnabled = false
    check.addSubview(badge); check.sendSubviewToBack(badge)
    check.addAction(UIAction { [weak self] _ in self?.callback?() }, for: .touchUpInside)
    for child in [image, duration, cameraIcon, check] { contentView.addSubview(child) }
    isAccessibilityElement = true
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  func camera(label: String, appearance: ZoenKeyboardPanelAppearance?) {
    cancel(); identifier = nil; image.image = nil; check.isHidden = true; duration.isHidden = true
    cameraIcon.isHidden = false; cameraIcon.tintColor = appearance?.text
    contentView.backgroundColor = appearance?.separator
    accessibilityLabel = label; accessibilityIdentifier = "attachment-keyboard-camera"
    accessibilityTraits = .button
  }
  func photo(item: [String: Any], order: Int?, manager: PHImageManager, appearance: ZoenKeyboardPanelAppearance?, select: @escaping () -> Void) {
    cancel()
    guard let id = item["id"] as? String else { return }
    identifier = id; self.manager = manager; callback = select
    cameraIcon.isHidden = true; check.isHidden = false
    contentView.backgroundColor = appearance?.separator
    image.image = nil
    accessibilityLabel = item["name"] as? String
    accessibilityIdentifier = "attachment-keyboard-photo-\(id)"
    accessibilityTraits = order == nil ? .button : [.button, .selected]
    check.accessibilityLabel = accessibilityLabel
    check.accessibilityTraits = order == nil ? .button : [.button, .selected]
    check.setTitle(order.map(String.init) ?? "", for: .normal)
    check.setTitleColor(.white, for: .normal)
    badge.backgroundColor = order == nil ? UIColor.black.withAlphaComponent(0.25) : appearance?.accent
    badge.layer.borderWidth = order == nil ? 1.5 : 0
    let seconds = max(0, item["duration"] as? Double ?? 0)
    duration.text = "\(Int(seconds) / 60):\(String(format: "%02d", Int(seconds) % 60))"
    duration.isHidden = item["type"] as? String != "video"
    let local = String(id.dropFirst(5))
    guard let asset = PHAsset.fetchAssets(withLocalIdentifiers: [local], options: nil).firstObject else { return }
    let options = PHImageRequestOptions()
    options.deliveryMode = .opportunistic; options.resizeMode = .fast; options.isNetworkAccessAllowed = false
    request = manager.requestImage(for: asset, targetSize: CGSize(width: 360, height: 360), contentMode: .aspectFill, options: options) { [weak self] image, _ in
      Task { @MainActor [weak self] in guard let self, self.identifier == id else { return }; self.image.image = image }
    }
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    image.frame = contentView.bounds
    check.frame = CGRect(x: bounds.width - 44, y: 0, width: 44, height: 44)
    badge.frame = CGRect(x: 10, y: 10, width: 24, height: 24)
    cameraIcon.frame = CGRect(x: (bounds.width - 32) / 2, y: (bounds.height - 32) / 2, width: 32, height: 32)
    duration.frame = CGRect(x: 4, y: bounds.height - 22, width: max(0, bounds.width - 10), height: 18)
    duration.textAlignment = .right
  }
  func cancel() {
    if let request { manager?.cancelImageRequest(request) }
    request = nil; callback = nil; identifier = nil
  }
  override func prepareForReuse() { super.prepareForReuse(); cancel(); image.image = nil }
}
