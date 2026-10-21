import Photos
import UIKit

// UIKit translation of TelegramMessenger/Telegram-iOS (GPL-2.0-or-later,
// GPLv3 selected), f1dd7a2dbd02cbbf513e75d5695d8d36d1cf5838:
// MediaPickerUI/Sources/MediaGroupsContextMenuContent.swift:24–133, 193–210,
// 266–295, 346–347. The PhotoSession supplies authorized rows; this view does
// not enumerate albums, request permission, or change selection authority.
@MainActor
final class ZoenAttachmentAlbumMenu: UIView, UITableViewDataSource, UITableViewDelegate {
  private let shield = UIControl()
  private let surface = ZoenTelegramAttachmentGlassView()
  private let table = UITableView(frame: .zero, style: .plain)
  private let select: (String) -> Void
  private let dismiss: () -> Void
  private let details: (String) -> Void
  private var rows: [[String: Any]] = []
  private var appearance: ZoenKeyboardPanelAppearance?
  private var retired = false
  private var requested = Set<String>()

  var scrollView: UIScrollView { table }
  var animatedViews: [UIView] { [surface, table] }

  init(select: @escaping (String) -> Void, dismiss: @escaping () -> Void, details: @escaping (String) -> Void) {
    self.select = select; self.dismiss = dismiss; self.details = details
    super.init(frame: .zero)
    accessibilityIdentifier = "attachment-album-menu"
    accessibilityViewIsModal = true
    shield.addAction(UIAction { [weak self] _ in
      guard let self, !self.retired else { return }
      self.dismiss()
    }, for: .touchUpInside)
    shield.isAccessibilityElement = true; shield.accessibilityTraits = .button
    addSubview(shield)
    addSubview(surface)
    surface.contentView.addSubview(table)
    surface.clipsToBounds = true
    table.backgroundColor = .clear
    table.separatorStyle = .none
    table.showsVerticalScrollIndicator = false
    table.contentInsetAdjustmentBehavior = .never
    table.rowHeight = 54
    table.estimatedRowHeight = 54
    table.delaysContentTouches = false
    table.scrollsToTop = false
    table.dataSource = self; table.delegate = self
    table.register(AlbumCell.self, forCellReuseIdentifier: "album")
    isHidden = true
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

  func update(rows: [[String: Any]], dismissLabel: String?, appearance: ZoenKeyboardPanelAppearance) throws {
    guard !retired, rows.count <= 2_000, rows.allSatisfy({ row in
      guard let id = row["id"] as? String, id.utf16.count <= 1024, !id.contains("\0"),
            let title = row["title"] as? String, title.utf16.count <= 4096, !title.contains("\0") else { return false }
      if let count = row["count"] as? Int, count < 0 { return false }
      if let preview = row["previewId"] as? String, !preview.hasPrefix("ph://") || preview.utf16.count > 1024 { return false }
      return true
    }), Set(rows.compactMap { $0["id"] as? String }).count == rows.count else { throw Failure.invalid }
    let changed = !NSArray(array: rows).isEqual(to: self.rows)
    self.rows = rows; self.appearance = appearance
    shield.accessibilityLabel = dismissLabel
    surface.update(isDark: appearance.isDark, cornerRadius: 14, isInteractive: false)
    if changed { table.reloadData() }
    else { for case let cell as AlbumCell in table.visibleCells { cell.updateAppearance(appearance) } }
  }

  func updateLayout(anchor: CGRect, safeArea: UIEdgeInsets) {
    shield.frame = bounds
    let available = max(0, bounds.width - safeArea.left - safeArea.right - 16)
    let width = min(190, available)
    let top = min(max(safeArea.top, anchor.maxY), max(safeArea.top, bounds.height - safeArea.bottom - 54))
    let height = min(295, max(0, bounds.height - safeArea.bottom - top - 8), CGFloat(rows.count) * 54)
    surface.frame = CGRect(x: min(max(safeArea.left + 8, floor(anchor.midX - width / 2)), max(safeArea.left + 8, bounds.width - safeArea.right - width - 8)), y: top, width: width, height: height)
    table.frame = surface.bounds
    requestVisibleDetails()
  }

  func tableView(_ tableView: UITableView, numberOfRowsInSection section: Int) -> Int { rows.count }
  func tableView(_ tableView: UITableView, cellForRowAt indexPath: IndexPath) -> UITableViewCell {
    guard let cell = tableView.dequeueReusableCell(withIdentifier: "album", for: indexPath) as? AlbumCell,
          rows.indices.contains(indexPath.row) else { return UITableViewCell() }
    cell.update(row: rows[indexPath.row], isLast: indexPath.row == rows.count - 1, appearance: appearance)
    return cell
  }
  func tableView(_ tableView: UITableView, didSelectRowAt indexPath: IndexPath) {
    tableView.deselectRow(at: indexPath, animated: true)
    guard !retired, rows.indices.contains(indexPath.row), let id = rows[indexPath.row]["id"] as? String else { return }
    select(id)
  }
  func tableView(_ tableView: UITableView, willDisplay cell: UITableViewCell, forRowAt indexPath: IndexPath) {
    requestDetails(at: indexPath)
  }
  private func requestVisibleDetails() {
    for indexPath in table.indexPathsForVisibleRows ?? [] { requestDetails(at: indexPath) }
  }
  private func requestDetails(at indexPath: IndexPath) {
    guard !retired, !isHidden, window != nil, rows.indices.contains(indexPath.row), let id = rows[indexPath.row]["id"] as? String,
          !id.isEmpty, rows[indexPath.row]["count"] == nil, requested.insert(id).inserted else { return }
    details(id)
  }
  func retire() {
    guard !retired else { return }
    retired = true; isUserInteractionEnabled = false
    table.setContentOffset(table.contentOffset, animated: false)
    for case let cell as AlbumCell in table.visibleCells { cell.cancel() }
  }
  override func accessibilityPerformEscape() -> Bool {
    guard !retired else { return false }
    dismiss(); return true
  }
  private enum Failure: Error { case invalid }
}

@MainActor
private final class AlbumCell: UITableViewCell {
  private let title = UILabel()
  private let count = UILabel()
  private let thumbnail = UIImageView()
  private let separator = UIView()
  private let manager = PHImageManager.default()
  private var request: PHImageRequestID?
  private var identifier: String?
  private var previewIdentifier: String?

  override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
    super.init(style: style, reuseIdentifier: reuseIdentifier)
    backgroundColor = .clear
    for view in [title, count, thumbnail, separator] { contentView.addSubview(view) }
    title.font = .systemFont(ofSize: 17)
    count.font = .systemFont(ofSize: 15)
    thumbnail.layer.cornerRadius = 6; thumbnail.clipsToBounds = true; thumbnail.contentMode = .scaleAspectFill
    isAccessibilityElement = true
    accessibilityTraits = .button
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }

  func update(row: [String: Any], isLast: Bool, appearance: ZoenKeyboardPanelAppearance?) {
    let nextIdentifier = row["id"] as? String
    let nextPreview = row["previewId"] as? String
    let changed = nextIdentifier != identifier || nextPreview != previewIdentifier
    if changed { cancel(); identifier = nextIdentifier; previewIdentifier = nextPreview }
    title.text = row["title"] as? String
    count.text = (row["count"] as? Int).map(String.init)
    count.isHidden = count.text == nil
    accessibilityLabel = title.text
    accessibilityValue = count.text
    accessibilityIdentifier = "attachment-album-\(nextIdentifier ?? "recents")"
    separator.isHidden = isLast
    if let appearance { updateAppearance(appearance) }
    if changed {
      thumbnail.image = nil
      if let preview = nextPreview,
         let asset = PHAsset.fetchAssets(withLocalIdentifiers: [String(preview.dropFirst(5))], options: nil).firstObject {
        let options = PHImageRequestOptions()
        options.isNetworkAccessAllowed = false; options.deliveryMode = .opportunistic; options.resizeMode = .fast
        let scale = max(1, traitCollection.displayScale)
        request = manager.requestImage(for: asset, targetSize: CGSize(width: 24 * scale, height: 24 * scale), contentMode: .aspectFill, options: options) { [weak self] image, _ in
          Task { @MainActor [weak self] in
            guard let self, self.identifier == nextIdentifier, self.previewIdentifier == nextPreview else { return }
            self.thumbnail.image = image
          }
        }
      }
    }
    setNeedsLayout()
  }
  func updateAppearance(_ appearance: ZoenKeyboardPanelAppearance) {
    title.textColor = appearance.text; count.textColor = appearance.secondary; separator.backgroundColor = appearance.separator
    let selected = UIView(); selected.backgroundColor = appearance.text.withAlphaComponent(0.08); selectedBackgroundView = selected
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    let textWidth = max(0, bounds.width - 16 - 48)
    let titleSize = title.sizeThatFits(CGSize(width: textWidth, height: 100))
    let countSize = count.isHidden ? .zero : count.sizeThatFits(CGSize(width: textWidth, height: 100))
    let contentHeight = titleSize.height + (count.isHidden ? 0 : 2 + countSize.height)
    title.frame = CGRect(x: 16, y: floor((bounds.height - contentHeight) / 2), width: textWidth, height: titleSize.height)
    count.frame = CGRect(x: 16, y: title.frame.maxY + 2, width: textWidth, height: countSize.height)
    thumbnail.frame = CGRect(x: bounds.width - 40, y: floor((bounds.height - 24) / 2), width: 24, height: 24)
    separator.frame = CGRect(x: 0, y: bounds.height - 1 / max(1, traitCollection.displayScale), width: bounds.width, height: 1 / max(1, traitCollection.displayScale))
  }
  func cancel() { if let request { manager.cancelImageRequest(request) }; request = nil; identifier = nil; previewIdentifier = nil }
  override func prepareForReuse() { super.prepareForReuse(); cancel(); thumbnail.image = nil }
}
