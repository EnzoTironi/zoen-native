import UIKit

/// ItemListPeerActionItem:215–224,267,310,385. The .generic/.glass
/// geometry and original icon are retained; UITableView provides reuse/AX.
@MainActor final class RecentActionCell: UITableViewCell {
  private let glyph = UIImageView(), title = UILabel(), separator = UIView()
  override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
    super.init(style: style, reuseIdentifier: reuseIdentifier)
    title.font = .systemFont(ofSize: 17)
    for child in [glyph, title, separator] { contentView.addSubview(child) }
    layer.cornerRadius = 26; layer.cornerCurve = .continuous; clipsToBounds = true
  }
  required init?(coder: NSCoder) { nil }
  func update(title: String, image: UIImage?, appearance: ZoenKeyboardPanelAppearance, enabled: Bool, first: Bool, last: Bool) {
    self.title.text = title; self.title.textColor = appearance.accent
    glyph.image = image; glyph.tintColor = appearance.accent
    backgroundColor = appearance.text.withAlphaComponent(appearance.isDark ? 0.06 : 0.035)
    separator.backgroundColor = appearance.separator; separator.isHidden = last
    layer.maskedCorners = []
    if first { layer.maskedCorners.formUnion([.layerMinXMinYCorner, .layerMaxXMinYCorner]) }
    if last { layer.maskedCorners.formUnion([.layerMinXMaxYCorner, .layerMaxXMaxYCorner]) }
    isUserInteractionEnabled = enabled; contentView.alpha = enabled ? 1 : 0.5
    accessibilityLabel = title; accessibilityTraits = enabled ? [.button] : [.button, .notEnabled]
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    let image = glyph.image?.size ?? .zero, pixel = 1 / max(1, traitCollection.displayScale)
    glyph.frame = CGRect(x: floor((59 - image.width) / 2) + 1, y: floor((bounds.height - image.height) / 2 / pixel) * pixel, width: image.width, height: image.height)
    let height = title.font.lineHeight
    title.frame = CGRect(x: 59, y: floor((bounds.height - height) / 2 / pixel) * pixel + 1, width: max(0, bounds.width - 59 - 16), height: height)
    separator.frame = CGRect(x: 59, y: bounds.height - pixel, width: max(0, bounds.width - 59), height: pixel)
  }
}

@MainActor final class RecentFileCell: UITableViewCell {
  private let extractable = UIView(), body = UIView(), offset = UIView(), extractedBackground = UIView()
  private let icon = UIImageView(), fileExtension = UILabel(), title = UILabel(), subtitle = UILabel(), check = UILabel(), separator = UIView()
  private let previewButton = UIButton(type: .custom)
  var preview: (() -> Void)? { didSet { previewButton.isHidden = preview == nil } }
  var previewLabel: String? { didSet { previewButton.accessibilityLabel = previewLabel } }
  private var selecting = false
  let contextGesture = ZoenAttachmentContextGesture(target: nil, action: nil)
  var openContext: ((ZoenAttachmentContextGesture) -> Bool)?
  var contextMenuLabel: String? {
    didSet {
      guard contextGesture.isEnabled, let label = contextMenuLabel, !label.isEmpty else { accessibilityCustomActions = nil; return }
      accessibilityCustomActions = [UIAccessibilityCustomAction(name: label) { [weak self] _ in
        guard let self, self.contextGesture.isEnabled else { return false }
        return self.openContext?(self.contextGesture) ?? false
      }]
    }
  }
  override init(style: UITableViewCell.CellStyle, reuseIdentifier: String?) {
    super.init(style: style, reuseIdentifier: reuseIdentifier)
    backgroundColor = .clear
    extractable.accessibilityIdentifier = "attachment-file-extraction-source"
    body.accessibilityIdentifier = "attachment-file-extraction-content"
    contentView.addSubview(extractable); extractable.addSubview(body)
    body.addSubview(extractedBackground); body.addSubview(offset)
    extractedBackground.alpha = 0; extractedBackground.layer.cornerRadius = 14
    for child in [icon, fileExtension, title, subtitle] { offset.addSubview(child) }
    contentView.addSubview(check); contentView.addSubview(separator)
    offset.addSubview(previewButton)
    extractable.addGestureRecognizer(contextGesture)
    contextGesture.shouldBegin = { [weak self] _ in self?.openContext != nil }
    contextGesture.activated = { [weak self] gesture, _ in self?.openContext?(gesture) ?? false }
    contextGesture.activationProgress = { [weak self] progress, state in
      guard let self, self.extractable.bounds.width > 0 else { return }
      let minimum = max(0.7, (self.extractable.bounds.width - 15) / self.extractable.bounds.width)
      let scale = 1 * (1 - progress) + minimum * progress
      let transform = CATransform3DMakeScale(scale, scale, 1)
      let previous = self.extractable.layer.sublayerTransform
      self.extractable.layer.sublayerTransform = UIAccessibility.isReduceMotionEnabled ? CATransform3DIdentity : transform
      if case .ended = state, !UIAccessibility.isReduceMotionEnabled {
        self.extractable.layer.animate(from: NSValue(caTransform3D: previous), to: NSValue(caTransform3D: transform),
          keyPath: "sublayerTransform", timingFunction: CAMediaTimingFunctionName.easeOut.rawValue, duration: 0.2)
      }
    }
    previewButton.addAction(UIAction { [weak self] _ in self?.preview?() }, for: .touchUpInside)
    title.font = .systemFont(ofSize: 16, weight: .semibold); title.numberOfLines = 1
    subtitle.font = .systemFont(ofSize: 14); subtitle.numberOfLines = 1; subtitle.lineBreakMode = .byTruncatingMiddle
    check.textAlignment = .center; check.font = .systemFont(ofSize: 15, weight: .medium); check.clipsToBounds = true
    fileExtension.textAlignment = .center; fileExtension.textColor = .white
  }
  required init?(coder: NSCoder) { nil }
  static func rowLayout(width: CGFloat, scale: CGFloat, selection: Bool) -> ZoenTelegramFileRowLayout {
    ZoenTelegramFileRowLayout.measure(width: width, left: 0, right: 0, titleHeight: ceil(UIFont.systemFont(ofSize: 16, weight: .semibold).lineHeight) + 4, subtitleHeight: ceil(UIFont.systemFont(ofSize: 14).lineHeight) + 4, scale: scale, selection: selection)
  }
  func update(row: [String: Any], audio: Bool, selection: Bool, order: Int?, appearance: ZoenKeyboardPanelAppearance, enabled: Bool) {
    selecting = selection
    let name = row["name"] as? String ?? "", ext = (name as NSString).pathExtension.lowercased()
    title.text = name; title.lineBreakMode = audio ? .byTruncatingTail : .byTruncatingMiddle
    icon.image = audio ? ZoenTelegramAttachmentGlyph.image(.play) : ZoenTelegramFileVisuals.documentIcon(ext)
    icon.contentMode = .center; icon.layer.cornerRadius = audio ? 20 : 12; icon.clipsToBounds = true
    fileExtension.text = audio ? "" : ext
    let size: CGFloat = ext.count > 3 ? 14 : 15
    let font = UIFont.systemFont(ofSize: size, weight: .bold)
    fileExtension.font = UIFont(descriptor: font.fontDescriptor.withDesign(.rounded) ?? font.fontDescriptor, size: size)
    fileExtension.adjustsFontSizeToFitWidth = true; fileExtension.minimumScaleFactor = 12 / size
    let bytes = UInt64(row["bytes"] as? String ?? "") ?? 0
    let byteText = ByteCountFormatter.string(fromByteCount: Int64(clamping: bytes), countStyle: .file)
    if audio { subtitle.text = byteText + " • " + (row["author_title"] as? String ?? "") }
    else {
      let milliseconds = Double(row["at_ms"] as? String ?? "") ?? 0
      let date = DateFormatter.localizedString(from: Date(timeIntervalSince1970: milliseconds / 1000), dateStyle: .medium, timeStyle: .none)
      subtitle.text = byteText + " • " + date
    }
    check.isHidden = !selection; check.text = order.map(String.init) ?? ""
    check.backgroundColor = order == nil ? .clear : appearance.accent
    check.layer.borderWidth = order == nil ? 1 : 0; check.layer.borderColor = appearance.secondary.cgColor
    check.textColor = .white
    applyAppearance(appearance)
    icon.backgroundColor = audio ? appearance.accent : .clear; icon.tintColor = .white
    let available = enabled && bytes > 0 && bytes <= 64 << 20
    isUserInteractionEnabled = available; contentView.alpha = available ? 1 : 0.5
    accessibilityLabel = name + ", " + (subtitle.text ?? "")
    accessibilityTraits = available ? [.button] : [.button, .notEnabled]
    if order != nil { accessibilityTraits.insert(.selected) }
    accessibilityIdentifier = "attachment-recent-file-" + (row["id"] as? String ?? "")
    setNeedsLayout()
  }
  func applyAppearance(_ value: ZoenKeyboardPanelAppearance) { title.textColor = value.text; subtitle.textColor = value.secondary; separator.backgroundColor = value.separator; backgroundColor = value.text.withAlphaComponent(value.isDark ? 0.06 : 0.035); extractedBackground.backgroundColor = value.isDark ? UIColor(red: 44 / 255, green: 44 / 255, blue: 46 / 255, alpha: 1) : .white }
  func extractedSource() -> ZoenAttachmentExtractedContent {
    ZoenAttachmentExtractedContent(anchor: extractable, content: body) { [weak self] extracted, transition in
      guard let self else { return }
      transition.setAlpha(view: self.extractedBackground, alpha: extracted ? 1 : 0)
      transition.setFrame(view: self.extractedBackground, frame: extracted ? self.body.bounds : CGRect(x: -16, y: 0, width: self.body.bounds.width + 16, height: self.body.bounds.height))
      transition.setPosition(view: self.offset, position: CGPoint(x: self.body.bounds.midX + (extracted ? 12 : 0), y: self.body.bounds.midY))
    }
  }
  override func prepareForReuse() {
    super.prepareForReuse(); contextGesture.cancel(); openContext = nil
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    extractable.frame = contentView.bounds
    if body.superview === extractable { body.frame = extractable.bounds; offset.frame = body.bounds; extractedBackground.frame = body.bounds }
    let frames = Self.rowLayout(width: bounds.width, scale: traitCollection.displayScale, selection: selecting)
    icon.frame = frames.icon; title.frame = frames.title; subtitle.frame = frames.subtitle
    previewButton.frame = frames.icon.insetBy(dx: -8, dy: -8)
    fileExtension.frame = frames.icon.insetBy(dx: 1, dy: 0).offsetBy(dx: 0, dy: 5)
    check.frame = frames.selection; check.layer.cornerRadius = 13; separator.frame = frames.separator
  }
}

// Empty-state geometry: AttachmentFileEmptyItem:117–160 and
// ItemListControllerNode:857–865. Empty state receives intrinsic panel insets;
// only search receives the collapsed sheet's additional bottom inset.
struct ZoenAttachmentFileEmptyLayout {
  let image: CGRect
  let text: CGRect
  let showsImage: Bool
  let showsText: Bool
  static func measure(size: CGSize, textSize: CGSize, navigationHeight: CGFloat,
                      actionRows: Int, actionHeight: CGFloat, bottomInset: CGFloat,
                      additionalBottomInset: CGFloat, left: CGFloat, right: CGFloat) -> Self {
    let imageSize = CGSize(width: size.width == 320 ? 112 : 144, height: size.width == 320 ? 112 : 144)
    let topInset: CGFloat = size.width == 320 ? -60 : 120
    let intrinsicBottom = max(0, bottomInset - additionalBottomInset)
    let sourceImageHeight: CGFloat = size.width < size.height ? imageSize.height + 12 : 0
    // Source reserves the optional button geometry even for .intro, where the
    // actual button is absent. The Gallery/Files actions remain separate rows.
    let sourceTotal = sourceImageHeight + 12 + textSize.height + 15 + 52 + 33
    var top = topInset + floor((size.height - topInset - intrinsicBottom - sourceTotal) / 2)
    var imageHeight = sourceImageHeight
    let actionsBottom = navigationHeight + 24 + CGFloat(actionRows) * actionHeight
    let contentTop = actionsBottom + 12, contentBottom = max(0, size.height - bottomInset)
    let freeHeight = max(0, contentBottom - contentTop)
    let showsText = freeHeight >= textSize.height
    let preferredTextBottom = top + imageHeight + 12 + textSize.height
    var textY = top + imageHeight + 12
    // Keep the source target whenever it fits. Compact viewports reuse its
    // no-artwork mode rather than painting decorative content over actions or
    // the keyboard/category panel. The remaining text stays in the free area.
    if top < contentTop || preferredTextBottom > contentBottom {
      if imageHeight + 12 + textSize.height > freeHeight { imageHeight = 0 }
      let height = imageHeight > 0 ? imageHeight + 12 + textSize.height : textSize.height
      top = contentTop + floor((freeHeight - height) / 2)
      textY = imageHeight > 0 ? top + imageHeight + 12 : top
    }
    let imageFrame = CGRect(x: floor((size.width - imageSize.width) / 2), y: top, width: imageSize.width, height: imageSize.height)
    let textX = left + floor((size.width - textSize.width - left - right) / 2)
    let textFrame = CGRect(x: textX, y: textY, width: textSize.width, height: textSize.height)
    let showsImage = showsText && imageHeight > 0
    return Self(image: imageFrame, text: textFrame, showsImage: showsImage, showsText: showsText)
  }
}
