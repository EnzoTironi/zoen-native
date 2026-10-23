import AVFoundation
import Photos
import UIKit

// AttachmentPanel.AttachButtonComponent:326–432. Explicit icon/title geometry
// replaces UIButton.Configuration's unrelated intrinsic image/title spacing.
@MainActor
final class AttachmentCategoryButton: UIButton {
  private let animation = ZoenTelegramAttachmentAnimationView(frame: .zero)
  override init(frame: CGRect) {
    super.init(frame: frame)
    titleLabel?.font = .systemFont(ofSize: 10, weight: .medium)
    titleLabel?.textAlignment = .center
    titleLabel?.lineBreakMode = .byTruncatingTail
    addSubview(animation)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  func update(glyph: ZoenTelegramAttachmentGlyph, tintColor: UIColor, isAnimating: Bool) {
    animation.update(glyph: glyph, tintColor: tintColor, isAnimating: isAnimating)
    setTitleColor(tintColor, for: .normal)
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    let pixel = 1 / max(1, traitCollection.displayScale)
    let top: CGFloat = 9 + pixel
    animation.frame = CGRect(x: floor((bounds.width - 30) / 2), y: top, width: 30, height: 30)
    let titleSize = titleLabel?.sizeThatFits(CGSize(width: 64, height: bounds.height)) ?? .zero
    titleLabel?.frame = CGRect(x: floor((bounds.width - min(64, titleSize.width)) / 2), y: top + 15 + 15 + pixel * 2, width: min(64, titleSize.width), height: titleSize.height)
  }
}

// MediaPickerTitleView:246–280, glass title offset and bundled expand arrow.
@MainActor
final class AttachmentAlbumTitleButton: UIButton {
  let arrow = UIImageView()
  override init(frame: CGRect) {
    super.init(frame: frame)
    arrow.isUserInteractionEnabled = false
    addSubview(arrow)
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  override func layoutSubviews() {
    super.layoutSubviews()
    let pixel = 1 / max(1, traitCollection.displayScale)
    let titleSize = titleLabel?.sizeThatFits(CGSize(width: min(210, max(0, bounds.width - 18)), height: 44)) ?? .zero
    let arrowSize = arrow.isHidden ? .zero : arrow.image?.size ?? .zero
    let spacing: CGFloat = arrow.isHidden ? 0 : 1
    let titleWidth = min(max(0, bounds.width - arrowSize.width - spacing), titleSize.width)
    let titleX = floor((bounds.width - titleWidth - arrowSize.width - spacing) / 2 / pixel) * pixel
    titleLabel?.frame = CGRect(x: titleX, y: floor((bounds.height - titleSize.height) / 2 / pixel) * pixel - 1, width: titleWidth, height: titleSize.height)
    arrow.frame = CGRect(x: titleX + titleWidth + spacing, y: floor((bounds.height - titleSize.height) / 2 / pixel) * pixel + titleSize.height / 2 - arrowSize.height / 2 - pixel, width: arrowSize.width, height: arrowSize.height)
  }
}

@MainActor
final class AttachmentRootView: UIView {
  var onLayout: (() -> Void)?
  override func layoutSubviews() { super.layoutSubviews(); onLayout?() }
  override func didMoveToWindow() { super.didMoveToWindow(); onLayout?() }
}

@MainActor
final class PhotoLayout: UICollectionViewLayout {
  var columns = 3 { didSet { if oldValue != columns { invalidateLayout() } } }
  var camera = true
  private var values: [UICollectionViewLayoutAttributes] = []
  private var contentHeight: CGFloat = 0
  override func prepare() {
    super.prepare()
    guard let collectionView else { return }
    let width = collectionView.bounds.width, scale = max(1, collectionView.traitCollection.displayScale)
    let size = floor((width - CGFloat(columns - 1)) / CGFloat(columns) * scale) / scale
    guard size > 0 else { values = []; contentHeight = 0; return }
    let count = collectionView.numberOfItems(inSection: 0), firstCount = (columns - 1) * 2
    values = (0..<count).map { index in
      let value = UICollectionViewLayoutAttributes(forCellWith: IndexPath(item: index, section: 0))
      if camera && index == 0 { value.frame = CGRect(x: 0, y: 0, width: size, height: size * 2 + 1) }
      else {
        let photo = index - (camera ? 1 : 0)
        let row = camera && photo < firstCount ? photo / (columns - 1) : (camera ? 2 + (photo - firstCount) / columns : photo / columns)
        let column = camera && photo < firstCount ? 1 + photo % (columns - 1) : (camera ? (photo - firstCount) % columns : photo % columns)
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
final class PhotoCell: UICollectionViewCell {
  private let image = AttachmentThumbnailView()
  private let check = UIButton(type: .custom)
  private let badge = UIView()
  private let orderLabel = UILabel()
  private let duration = UILabel()
  private let cameraIcon = UIImageView(image: ZoenTelegramAttachmentGlyph.image(.camera))
  private weak var preview: UIView?
  private var request: PHImageRequestID?
  private weak var manager: PHImageManager?
  private var callback: (() -> Void)?
  private var activation: (() -> Void)?
  private(set) var identifier: String?
  var transitionView: UIView { image }
  override init(frame: CGRect) {
    super.init(frame: frame)
    contentView.clipsToBounds = true
    image.contentMode = .scaleAspectFill; image.clipsToBounds = true
    duration.textColor = .white; duration.font = .systemFont(ofSize: 12, weight: .semibold)
    duration.layer.shadowColor = UIColor.black.cgColor; duration.layer.shadowOpacity = 0.6; duration.layer.shadowRadius = 2
    badge.layer.borderColor = UIColor.white.cgColor; badge.isUserInteractionEnabled = false
    badge.layer.shadowColor = UIColor.black.cgColor; badge.layer.shadowOpacity = 0.3
    badge.layer.shadowRadius = 1; badge.layer.shadowOffset = CGSize(width: 0, height: 1)
    orderLabel.textColor = .white; orderLabel.textAlignment = .center
    badge.addSubview(orderLabel)
    check.addSubview(badge); check.sendSubviewToBack(badge)
    check.addAction(UIAction { [weak self] _ in self?.callback?() }, for: .touchUpInside)
    for child in [image, duration, cameraIcon, check] { contentView.addSubview(child) }
  }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  func camera(label: String, appearance: ZoenKeyboardPanelAppearance?, preview: UIView, activate: @escaping () -> Void) {
    cancel(); identifier = nil; image.image = nil; check.isHidden = true; duration.isHidden = true
    self.preview = preview; contentView.insertSubview(preview, at: 0)
    preview.frame = contentView.bounds; preview.autoresizingMask = [.flexibleWidth, .flexibleHeight]
    cameraIcon.isHidden = false; cameraIcon.tintColor = .white; contentView.backgroundColor = appearance?.separator
    isAccessibilityElement = true; accessibilityLabel = label
    accessibilityIdentifier = "attachment-keyboard-camera"; accessibilityTraits = .button
    activation = activate
  }
  func photo(item: [String: Any], order: Int?, manager: PHImageManager, appearance: ZoenKeyboardPanelAppearance?, loadThumbnail: Bool, preview openPreview: @escaping () -> Void, select: @escaping () -> Void) {
    cancel()
    guard let id = item["id"] as? String else { return }
    identifier = id; self.manager = manager; callback = select
    preview?.removeFromSuperview(); preview = nil; cameraIcon.isHidden = true; check.isHidden = false
    contentView.backgroundColor = appearance?.separator; image.image = nil
    isAccessibilityElement = false; image.isAccessibilityElement = true; image.accessibilityTraits = .button
    image.accessibilityLabel = item["name"] as? String; image.accessibilityIdentifier = "attachment-keyboard-photo-\(id)"
    image.activate = openPreview
    check.accessibilityLabel = item["name"] as? String; check.accessibilityIdentifier = "attachment-keyboard-select-\(id)"
    updateSelection(order: order, appearance: appearance)
    let seconds = max(0, item["duration"] as? Double ?? 0)
    duration.text = "\(Int(seconds) / 60):\(String(format: "%02d", Int(seconds) % 60))"; duration.isHidden = item["type"] as? String != "video"
    guard loadThumbnail, let asset = PHAsset.fetchAssets(withLocalIdentifiers: [String(id.dropFirst(5))], options: nil).firstObject else { return }
    let options = PHImageRequestOptions()
    options.deliveryMode = .opportunistic; options.resizeMode = .fast; options.isNetworkAccessAllowed = false
    request = manager.requestImage(for: asset, targetSize: CGSize(width: 360, height: 360), contentMode: .aspectFill, options: options) { [weak self] image, _ in
      Task { @MainActor [weak self] in guard let self, self.identifier == id else { return }; self.image.image = image }
    }
  }
  func updateSelection(order: Int?, appearance: ZoenKeyboardPanelAppearance?) {
    check.accessibilityTraits = order == nil ? .button : [.button, .selected]
    orderLabel.text = order.map(String.init) ?? ""
    let digits = orderLabel.text?.count ?? 0
    let size: CGFloat = digits < 2 ? 16 : (digits == 2 ? 15 : 13)
    let font = UIFont.systemFont(ofSize: size, weight: .medium)
    orderLabel.font = UIFont(descriptor: font.fontDescriptor.withDesign(.rounded) ?? font.fontDescriptor, size: size)
    badge.backgroundColor = order == nil ? UIColor.black.withAlphaComponent(0.25) : appearance?.accent
    badge.layer.borderWidth = order == nil ? 1.5 : 0
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    image.frame = contentView.bounds
    check.frame = CGRect(x: bounds.width - 44, y: 0, width: 44, height: 44)
    let pixel = 1 / max(1, traitCollection.displayScale)
    let inset = 0.75 + 2 - pixel
    badge.frame = CGRect(x: 12 + inset, y: 3 + inset, width: 29 - inset * 2, height: 29 - inset * 2)
    badge.layer.cornerRadius = badge.bounds.width / 2
    orderLabel.frame = badge.bounds
    cameraIcon.frame = CGRect(x: bounds.width - 36, y: 10, width: 26, height: 24)
    duration.frame = CGRect(x: 4, y: bounds.height - 22, width: max(0, bounds.width - 10), height: 18); duration.textAlignment = .right
  }
  func cancel() {
    if let request { manager?.cancelImageRequest(request) }
    request = nil; callback = nil; activation = nil; image.activate = nil; identifier = nil
  }
  override func accessibilityActivate() -> Bool {
    guard let activation else { return false }
    activation(); return true
  }
  override func prepareForReuse() { super.prepareForReuse(); cancel(); image.image = nil; preview?.removeFromSuperview(); preview = nil }
}

@MainActor
private final class AttachmentThumbnailView: UIImageView {
  var activate: (() -> Void)?
  override func accessibilityActivate() -> Bool {
    guard let activate else { return false }
    activate(); return true
  }
}

@MainActor
final class AttachmentCameraPreview: UIView {
  private let preview = AVCaptureVideoPreviewLayer()
  private let capture = AttachmentPreviewCapture()
  private var retired = false
  private var running = false
  override init(frame: CGRect) {
    super.init(frame: frame)
    preview.videoGravity = .resizeAspectFill; preview.session = capture.session; layer.addSublayer(preview)
  }
  convenience init() { self.init(frame: .zero) }
  required init?(coder: NSCoder) { fatalError("init(coder:) is unavailable") }
  override func layoutSubviews() { super.layoutSubviews(); preview.frame = bounds }
  func setRunning(_ value: Bool) {
    let next = value && !retired && AVCaptureDevice.authorizationStatus(for: .video) == .authorized
    guard next != running else { return }
    running = next; capture.setRunning(next)
  }
  func retire(completion: @escaping () -> Void) { retired = true; running = false; capture.stop(completion: completion) }
  override func didMoveToWindow() { super.didMoveToWindow(); if window == nil { setRunning(false) } }
}

// The capture queue serializes start/stop. Its completed stop, rather than a
// guessed animation duration, precedes dismissal and the external camera handoff.
private final class AttachmentPreviewCapture: @unchecked Sendable {
  let session = AVCaptureSession()
  private let queue = DispatchQueue(label: "xyz.zoen.attachment-camera-preview", qos: .userInitiated)
  private var configured = false
  func setRunning(_ value: Bool) { queue.async { [self] in update(value) } }
  func stop(completion: @escaping @MainActor () -> Void) {
    queue.async { [self] in
      update(false)
      DispatchQueue.main.async { completion() }
    }
  }
  private func update(_ value: Bool) {
    if value, !configured {
      guard AVCaptureDevice.authorizationStatus(for: .video) == .authorized,
            let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
            let input = try? AVCaptureDeviceInput(device: device), session.canAddInput(input) else { return }
      session.beginConfiguration(); session.sessionPreset = .medium; session.addInput(input); session.commitConfiguration(); configured = true
    }
    if value && configured && !session.isRunning { session.startRunning() }
    else if !value && session.isRunning { session.stopRunning() }
  }
}

// UIKit adaptation of MediaPickerGridSelectionGesture. Per-gesture IDs suppress
// duplicate requests only; PhotoSession remains the sole selection authority.
@MainActor
final class GridSelectionGesture: UIPanGestureRecognizer {
  var itemAt: (CGPoint) -> (String, Bool)? = { _ in nil }
  var updateSelection: (String, Bool) -> Void = { _, _ in }
  var updateIsScrollEnabled: (Bool) -> Void = { _ in }
  var began: () -> Void = {}
  var sideInset: CGFloat = 0
  private var processing = false
  private var selecting = false
  private var initialLocation: CGPoint?
  init() { super.init(target: nil, action: nil); maximumNumberOfTouches = 1 }
  override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesBegan(touches, with: event)
    guard let touch = touches.first, numberOfTouches == 1 else { return }
    let location = touch.location(in: view)
    if location.x > sideInset { initialLocation = location } else { state = .failed }
  }
  override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
    super.touchesMoved(touches, with: event)
    guard let touch = touches.first, let initialLocation else { state = .failed; return }
    let location = touch.location(in: view), translation = CGPoint(x: location.x - initialLocation.x, y: location.y - initialLocation.y)
    var additionalLocation: CGPoint?
    if !processing {
      if abs(translation.y) > 5 { state = .failed }
      else if abs(translation.x) > 8 {
        processing = true; updateIsScrollEnabled(false); began()
        if let (_, selected) = itemAt(location) { selecting = !selected }
        additionalLocation = initialLocation
      }
    }
    if processing {
      if let additionalLocation, let (item, selected) = itemAt(additionalLocation), selected != selecting { updateSelection(item, selecting) }
      if let (item, selected) = itemAt(location), selected != selecting { updateSelection(item, selecting) }
    }
  }
  override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { super.touchesEnded(touches, with: event); state = .failed; reset() }
  override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { super.touchesCancelled(touches, with: event); state = .failed; reset() }
  override func reset() { super.reset(); processing = false; initialLocation = nil; updateIsScrollEnabled(true) }
}
