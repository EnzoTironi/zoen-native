import AVFoundation
import Photos
import UIKit

/// Uses the same pinned Telegram player/scrubber as Edit, with trimming and
/// export controls absent. This is a cancelable display lease over the current
/// Photos asset or journal URI; it never creates files or accepts media edits.
@MainActor final class ZoenAttachmentGalleryVideo: UIView {
  private let item: GalleryItem
  private let manager = PHImageManager()
  private let activity = UIActivityIndicatorView(style: .large)
  private let retry = UIButton(type: .system)
  private var request: PHImageRequestID?
  private var preparation: Task<Void, Never>?
  private var content: ZoenTelegramVideoEditingContent?
  private var generation = UUID()
  private var active = false
  private var retired = false
  private var chromeVisible = true
  private var foregroundObserver: NSObjectProtocol?
  private var safeArea = UIEdgeInsets.zero
  private var captionFrame = CGRect.zero

  init(item: GalleryItem) {
    self.item = item
    super.init(frame: .zero)
    activity.color = .white
    retry.setTitle(NSLocalizedString("Try Again", comment: "Retry requested video preview"), for: .normal)
    retry.tintColor = .white
    retry.accessibilityIdentifier = "attachment-gallery-video-retry"
    retry.accessibilityHint = NSLocalizedString("Video preview unavailable", comment: "Failed requested video preview")
    retry.isHidden = true
    retry.addAction(UIAction { [weak self] _ in self?.reload() }, for: .touchUpInside)
    addSubview(activity); addSubview(retry)
    foregroundObserver = NotificationCenter.default.addObserver(
      forName: UIApplication.didBecomeActiveNotification, object: nil, queue: .main
    ) { [weak self] _ in
      MainActor.assumeIsolated { self?.prepare() }
    }
  }
  required init?(coder: NSCoder) { nil }
  deinit {
    if let foregroundObserver { NotificationCenter.default.removeObserver(foregroundObserver) }
  }
  override func didMoveToWindow() {
    super.didMoveToWindow()
    if window == nil { suspend() } else if active { prepare() }
  }
  func setActive(_ value: Bool) {
    guard !retired else { return }
    active = value
    if value { prepare() } else { suspend() }
  }
  func updatePresentation(visible: Bool, safeArea: UIEdgeInsets, captionFrame: CGRect) {
    chromeVisible = visible; self.safeArea = safeArea; self.captionFrame = captionFrame
    content?.headerView.isHidden = !visible
    if !visible { content?.pause() }
    setNeedsLayout()
  }
  func pause() { content?.pause() }
  var controlFrame: CGRect { content?.headerView.isHidden == false ? content?.headerView.frame ?? .zero : .zero }

  private func prepare() {
    guard active, !retired, window != nil else { return }
    if let content {
      let ticket = generation
      content.failed = { [weak self] _ in self?.fail(ticket) }
      content.setVisible(true); return
    }
    guard request == nil, preparation == nil, retry.isHidden else { return }
    activity.startAnimating()
    let ticket = generation
    if let url = item.uri { install(asset: AVURLAsset(url: url), ticket: ticket) }
    else if let id = item.photoId, let asset = PHAsset.fetchAssets(withLocalIdentifiers: [id], options: nil).firstObject {
      let options = PHVideoRequestOptions(); options.isNetworkAccessAllowed = true
      request = manager.requestAVAsset(forVideo: asset, options: options) { [weak self] asset, _, _ in
        Task { @MainActor [weak self] in
          guard let self, current(ticket) else { return }
          request = nil
          if let asset { install(asset: asset, ticket: ticket) } else { fail(ticket) }
        }
      }
    } else { fail(ticket) }
  }
  private func install(asset: AVAsset, ticket: UUID) {
    preparation = Task { @MainActor [weak self] in
      guard let self, current(ticket) else { return }
      do {
        let glyphs = try ZoenTelegramVideoGlyphs.bundled()
        let value = try await ZoenTelegramVideoEditingContent.prepare(asset: asset,
          allowsTrimming: false, glyphs: glyphs)
        guard !Task.isCancelled, current(ticket) else { value.retire(sensitive: true); return }
        preparation = nil; content = value
        // Sound/trim output changes remain explicit Edit -> Save -> journal CAS.
        value.muteButton.isHidden = true
        value.failed = { [weak self] _ in self?.fail(ticket) }
        insertSubview(value.view, at: 0); addSubview(value.headerView)
        activity.stopAnimating(); value.setVisible(true)
        setNeedsLayout()
      } catch {
        guard current(ticket), !Task.isCancelled else { return }
        preparation = nil; fail(ticket)
      }
    }
  }
  private func current(_ ticket: UUID) -> Bool { !retired && active && window != nil && generation == ticket }
  private func fail(_ ticket: UUID) {
    guard current(ticket) else { return }
    content?.pause(); activity.stopAnimating(); retry.isHidden = false
    bringSubviewToFront(retry)
  }
  private func reload() {
    guard active, !retired else { return }
    suspend(); content?.retire(sensitive: true)
    content?.view.removeFromSuperview(); content?.headerView.removeFromSuperview(); content = nil
    retry.isHidden = true; prepare()
  }
  private func suspend() {
    generation = UUID(); preparation?.cancel(); preparation = nil
    if let request { manager.cancelImageRequest(request) }; request = nil
    content?.setVisible(false); activity.stopAnimating()
  }
  override func layoutSubviews() {
    super.layoutSubviews()
    activity.center = CGPoint(x: bounds.midX, y: bounds.midY)
    retry.frame = CGRect(x: bounds.midX - 80, y: bounds.midY - 22, width: 160, height: 44)
    guard let content else { return }
    content.view.frame = bounds
    // GalleryInterface:2188. Its72pt header sits above caption/49pt toolbar.
    let landscape = bounds.width > bounds.height
    let y = landscape ? safeArea.top : max(safeArea.top, captionFrame.minY - 64)
    content.headerView.frame = CGRect(x: safeArea.left, y: y,
      width: max(0, bounds.width - safeArea.left - safeArea.right), height: landscape ? 64 : 72)
    content.headerView.isHidden = !chromeVisible
    content.updateLayout(safeArea: .zero)
  }
  func retire() {
    guard !retired else { return }
    suspend(); retired = true; active = false
    content?.retire(sensitive: true); content?.view.removeFromSuperview()
    content?.headerView.removeFromSuperview(); content = nil; isUserInteractionEnabled = false
  }
}
