import AVKit
import ImageIO
import UIKit

/// Read-only visual presentation under the existing ZoenFilePreview lease/session.
/// Editing and PDFs continue through Quick Look and its canonical capture path.
@MainActor final class ZoenMediaGalleryPreview: NSObject, ZoenFilePreviewPresentation {
  nonisolated static func supports(mime: String) -> Bool {
    ["image/jpeg", "image/png", "image/heic", "image/heif", "image/webp", "video/mp4", "video/quicktime", "video/x-m4v"].contains(mime)
  }
  private weak var presenter: UIViewController?
  private let mime: String
  private var controller: Gallery?
  private var ended: (@MainActor (Error?) -> Void)?
  private var presenting = false
  private var dismissing = false
  private var waiting = false
  private var didEnd = false
  private var completions: [@MainActor () -> Void] = []

  init(presenter: UIViewController?, mime: String) { self.presenter = presenter; self.mime = mime }

  func show(file: URL, title: String, ended: @escaping @MainActor (Error?) -> Void) throws {
    guard controller == nil, let presenter,
          presenter.viewIfLoaded?.window != nil,
          !presenter.isBeingPresented, !presenter.isBeingDismissed,
          presenter.transitionCoordinator == nil, presenter.presentedViewController == nil else {
      throw ZoenFilePreview.Failure.unavailable
    }
    let gallery = try Gallery(file: file, title: title, video: mime.hasPrefix("video/"))
    self.ended = ended
    controller = gallery
    gallery.closed = { [weak self] in self?.dismiss { [weak self] in self?.notifyEnded(nil) } }
    gallery.disappeared = { [weak self] in
      guard let self, !dismissing, !presenting else { return }
      notifyEnded(nil)
    }
    presenting = true
    presenter.present(gallery, animated: !UIAccessibility.isReduceMotionEnabled) { [self] in
      presenting = false
      if gallery.presentingViewController == nil { notifyEnded(ZoenFilePreview.Failure.unavailable) }
      drainDismissal()
    }
  }

  func dismiss(completion: @escaping @MainActor () -> Void) {
    completions.append(completion)
    controller?.stop()
    drainDismissal()
  }

  private func drainDismissal() {
    guard !presenting, !dismissing, !waiting, !completions.isEmpty else { return }
    guard let controller else { finishDismissal(); return }
    // Wait for an in-progress Share/Files child transition before removing the
    // entire hierarchy. The existing owner may erase while that child presents.
    if let transition = controller.transitionCoordinator ?? controller.presentedViewController?.transitionCoordinator {
      waiting = true
      let registered = transition.animate(alongsideTransition: nil) { [weak self] _ in
        Task { @MainActor [weak self] in
          guard let self else { return }
          waiting = false
          drainDismissal()
        }
      }
      if registered { return }
      waiting = false
    }
    dismissing = true
    if let owner = controller.presentingViewController {
      owner.dismiss(animated: false) { [self] in finishDismissal() }
    } else { finishDismissal() }
  }

  private func finishDismissal() {
    let callbacks = completions
    completions = []
    controller?.stop()
    controller = nil
    dismissing = false
    for callback in callbacks { callback() }
  }

  private func notifyEnded(_ error: Error?) {
    guard !didEnd else { return }
    didEnd = true
    controller?.stop()
    let callback = ended
    ended = nil
    callback?(error)
  }

  @MainActor private final class Gallery: UIViewController, UIScrollViewDelegate, UIGestureRecognizerDelegate {
    let file: URL
    let fileTitle: String
    let photo: UIImage?
    let video: Bool
    let canvas = UIView()
    let scroll = UIScrollView()
    let image = UIImageView()
    let topBar = UIView()
    let bottomBar = UIView()
    let titleLabel = UILabel()
    let closeButton = UIButton(type: .system)
    let shareButton = UIButton(type: .system)
    var playerController: AVPlayerViewController?
    var videoObservation: NSKeyValueObservation?
    let loading = UIActivityIndicatorView(style: .medium)
    let videoError = UILabel()
    let retryButton = UIButton(type: .system)
    var backgroundObserver: NSObjectProtocol?
    var closed: (() -> Void)?
    var disappeared: (() -> Void)?
    var controlsHidden = false
    var retiring = false
    var previousSize = CGSize.zero

    init(file: URL, title: String, video: Bool) throws {
      self.file = file; fileTitle = title; self.video = video
      if video { photo = nil }
      else {
        // Decode a bounded display proxy; the original signed bytes remain
        // untouched and are still the source used by Share.
        guard let source = CGImageSourceCreateWithURL(file as CFURL, nil),
              let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: 4096,
                kCGImageSourceShouldCacheImmediately: true,
              ] as CFDictionary) else { throw ZoenFilePreview.Failure.unavailable }
        photo = UIImage(cgImage: thumbnail)
      }
      super.init(nibName: nil, bundle: nil)
      modalPresentationStyle = .overFullScreen
      modalPresentationCapturesStatusBarAppearance = true
      modalTransitionStyle = .crossDissolve
    }
    required init?(coder: NSCoder) { fatalError("init(coder:) has not been implemented") }
    override var prefersStatusBarHidden: Bool { true }
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .allButUpsideDown }
    override func viewDidLoad() {
      super.viewDidLoad()
      view.backgroundColor = .black
      canvas.backgroundColor = .clear
      canvas.accessibilityIdentifier = "media-gallery-canvas"
      view.addSubview(canvas)
      if video {
        let player = AVPlayerViewController()
        player.player = AVPlayer()
        player.player?.allowsExternalPlayback = false
        player.allowsPictureInPicturePlayback = false
        addChild(player); canvas.addSubview(player.view); player.didMove(toParent: self)
        playerController = player
        loading.color = .white
        canvas.addSubview(loading)
        videoError.text = NSLocalizedString("This video could not be played.", comment: "Media gallery video failed")
        videoError.textColor = .white
        videoError.font = .systemFont(ofSize: 17)
        videoError.textAlignment = .center
        videoError.numberOfLines = 0
        videoError.accessibilityTraits = .staticText
        videoError.isHidden = true
        retryButton.setTitle(NSLocalizedString("Try again", comment: "Retry gallery video"), for: .normal)
        retryButton.tintColor = .white
        retryButton.accessibilityIdentifier = "media-gallery-video-retry"
        retryButton.isHidden = true
        retryButton.addTarget(self, action: #selector(reloadVideo), for: .touchUpInside)
        canvas.addSubview(videoError); canvas.addSubview(retryButton)
        reloadVideo()
      } else {
        scroll.delegate = self
        scroll.contentInsetAdjustmentBehavior = .never
        scroll.showsHorizontalScrollIndicator = false
        scroll.showsVerticalScrollIndicator = false
        scroll.scrollsToTop = false
        scroll.delaysContentTouches = false
        scroll.bouncesZoom = true
        image.image = photo
        image.isAccessibilityElement = true
        image.accessibilityLabel = fileTitle
        image.accessibilityIdentifier = "media-gallery-image"
        image.accessibilityTraits = .image
        canvas.addSubview(scroll); scroll.addSubview(image)
        let doubleTap = UITapGestureRecognizer(target: self, action: #selector(zoom(_:)))
        doubleTap.numberOfTapsRequired = 2
        let tap = UITapGestureRecognizer(target: self, action: #selector(toggleControls))
        tap.require(toFail: doubleTap)
        scroll.addGestureRecognizer(tap); scroll.addGestureRecognizer(doubleTap)
      }
      let pan = UIPanGestureRecognizer(target: self, action: #selector(pan(_:)))
      pan.maximumNumberOfTouches = 1
      pan.delegate = self
      canvas.addGestureRecognizer(pan)
      view.addSubview(topBar); view.addSubview(bottomBar)
      configure(closeButton, symbol: "chevron.left", label: NSLocalizedString("Close", comment: "Close media gallery"), action: #selector(close))
      configure(shareButton, symbol: "square.and.arrow.up", label: NSLocalizedString("Share", comment: "Share media gallery file"), action: #selector(share))
      closeButton.accessibilityIdentifier = "media-gallery-close"
      shareButton.accessibilityIdentifier = "media-gallery-share"
      topBar.addSubview(closeButton); bottomBar.addSubview(shareButton)
      titleLabel.text = fileTitle
      titleLabel.textColor = .white
      titleLabel.font = .systemFont(ofSize: 17, weight: .semibold)
      titleLabel.textAlignment = .center
      titleLabel.numberOfLines = 1
      topBar.addSubview(titleLabel)
      backgroundObserver = NotificationCenter.default.addObserver(forName: UIApplication.willResignActiveNotification, object: nil, queue: .main) { [weak self] _ in
        Task { @MainActor [weak self] in self?.playerController?.player?.pause() }
      }
    }
    private func configure(_ button: UIButton, symbol: String, label: String, action: Selector) {
      button.setImage(UIImage(systemName: symbol, withConfiguration: UIImage.SymbolConfiguration(pointSize: 22, weight: .regular)), for: .normal)
      button.tintColor = .white
      button.accessibilityLabel = label
      button.addTarget(self, action: action, for: .touchUpInside)
    }
    override func viewDidLayoutSubviews() {
      super.viewDidLayoutSubviews()
      let size = view.bounds.size
      canvas.frame = view.bounds
      topBar.frame = CGRect(x: view.safeAreaInsets.left, y: view.safeAreaInsets.top, width: max(0, size.width - view.safeAreaInsets.left - view.safeAreaInsets.right), height: 44)
      closeButton.frame = CGRect(x: 0, y: 0, width: 44, height: 44)
      titleLabel.frame = CGRect(x: 52, y: 0, width: max(0, topBar.bounds.width - 104), height: 44)
      bottomBar.frame = CGRect(x: view.safeAreaInsets.left, y: size.height - view.safeAreaInsets.bottom - 44, width: max(0, size.width - view.safeAreaInsets.left - view.safeAreaInsets.right), height: 44)
      shareButton.frame = CGRect(x: 0, y: 0, width: 44, height: 44)
      playerController?.view.frame = canvas.bounds
      loading.center = CGPoint(x: size.width / 2, y: size.height / 2)
      videoError.frame = CGRect(x: 24, y: size.height / 2 - 44, width: max(0, size.width - 48), height: 60)
      retryButton.frame = CGRect(x: size.width / 2 - 70, y: size.height / 2 + 20, width: 140, height: 44)
      scroll.frame = canvas.bounds
      if let photo, size != previousSize, size.width > 0, size.height > 0 {
        previousSize = size
        scroll.minimumZoomScale = 1
        scroll.maximumZoomScale = 1
        scroll.zoomScale = 1
        image.transform = .identity
        image.frame = CGRect(origin: .zero, size: photo.size)
        scroll.contentSize = photo.size
        let fit = min(size.width / photo.size.width, size.height / photo.size.height)
        scroll.minimumZoomScale = fit
        scroll.maximumZoomScale = max(max(size.width / photo.size.width, size.height / photo.size.height), fit * 3)
        scroll.zoomScale = fit
        centerImage()
      }
    }
    override func viewDidDisappear(_ animated: Bool) {
      super.viewDidDisappear(animated)
      if isBeingDismissed || presentingViewController == nil { stop(); disappeared?() }
    }
    func stop() {
      retiring = true
      videoObservation = nil
      playerController?.player?.pause()
      playerController?.player?.replaceCurrentItem(with: nil)
      if let backgroundObserver { NotificationCenter.default.removeObserver(backgroundObserver); self.backgroundObserver = nil }
    }
    @objc private func reloadVideo() {
      guard video, !retiring, let player = playerController?.player else { return }
      videoObservation = nil
      videoError.isHidden = true; retryButton.isHidden = true; loading.startAnimating()
      let item = AVPlayerItem(url: file)
      player.replaceCurrentItem(with: item)
      videoObservation = item.observe(\.status, options: [.initial, .new]) { [weak self] _, _ in
        Task { @MainActor [weak self] in self?.updateVideoStatus() }
      }
    }
    private func updateVideoStatus() {
      guard !retiring, let status = playerController?.player?.currentItem?.status else { return }
      switch status {
      case .readyToPlay: loading.stopAnimating()
      case .failed:
        loading.stopAnimating(); videoError.isHidden = false; retryButton.isHidden = false
      default: break
      }
    }
    func viewForZooming(in scrollView: UIScrollView) -> UIView? { image }
    func scrollViewDidZoom(_ scrollView: UIScrollView) {
      centerImage()
      scrollView.isScrollEnabled = abs(scrollView.zoomScale - scrollView.minimumZoomScale) > 0.001
    }
    private func centerImage() {
      image.center = CGPoint(x: max(scroll.bounds.width, scroll.contentSize.width) / 2, y: max(scroll.bounds.height, scroll.contentSize.height) / 2)
    }
    @objc private func zoom(_ recognizer: UITapGestureRecognizer) {
      guard !retiring else { return }
      if scroll.zoomScale > scroll.minimumZoomScale + 0.001 {
        scroll.setZoomScale(scroll.minimumZoomScale, animated: !UIAccessibility.isReduceMotionEnabled)
      } else {
        let at = recognizer.location(in: image)
        let width = scroll.bounds.width / scroll.maximumZoomScale
        let height = scroll.bounds.height / scroll.maximumZoomScale
        scroll.zoom(to: CGRect(x: at.x - width / 2, y: at.y - height / 2, width: width, height: height), animated: !UIAccessibility.isReduceMotionEnabled)
      }
    }
    @objc private func toggleControls() {
      guard !retiring else { return }
      controlsHidden.toggle()
      let changes = { self.topBar.alpha = self.controlsHidden ? 0 : 1; self.bottomBar.alpha = self.controlsHidden ? 0 : 1 }
      topBar.isUserInteractionEnabled = !controlsHidden; bottomBar.isUserInteractionEnabled = !controlsHidden
      UIView.animate(withDuration: UIAccessibility.isReduceMotionEnabled ? 0 : 0.2, delay: 0, options: [.curveLinear, .beginFromCurrentState], animations: changes)
    }
    func gestureRecognizerShouldBegin(_ gestureRecognizer: UIGestureRecognizer) -> Bool {
      guard let pan = gestureRecognizer as? UIPanGestureRecognizer, !retiring else { return false }
      guard video || abs(scroll.zoomScale - scroll.minimumZoomScale) <= 0.001 else { return false }
      let movement = pan.velocity(in: canvas)
      guard abs(movement.y) > abs(movement.x) else { return false }
      var hit = canvas.hitTest(pan.location(in: canvas), with: nil)
      while let current = hit, current !== canvas {
        if current is UIControl { return false }
        hit = current.superview
      }
      return true
    }
    @objc private func pan(_ recognizer: UIPanGestureRecognizer) {
      guard !retiring else { return }
      let distance = recognizer.translation(in: view).y
      switch recognizer.state {
      case .began: playerController?.player?.pause()
      case .changed:
        canvas.transform = CGAffineTransform(translationX: 0, y: distance)
        view.backgroundColor = UIColor.black.withAlphaComponent(max(0, 1 - abs(distance) / 80))
        let opacity = max(0, 1 - abs(distance) / 50)
        topBar.alpha = controlsHidden ? 0 : opacity; bottomBar.alpha = controlsHidden ? 0 : opacity
      case .ended:
        if abs(distance) > view.bounds.height / 4 || abs(recognizer.velocity(in: view).y) > 1000 {
          retiring = true
          let direction: CGFloat = distance < 0 ? -1 : 1
          UIView.animate(withDuration: UIAccessibility.isReduceMotionEnabled ? 0 : 0.2, delay: 0, options: [.curveLinear, .beginFromCurrentState], animations: {
            self.canvas.transform = CGAffineTransform(translationX: 0, y: direction * self.view.bounds.height)
            self.view.backgroundColor = .clear
            self.topBar.alpha = 0; self.bottomBar.alpha = 0
          }) { [weak self] _ in self?.closed?() }
        } else { restore() }
      case .cancelled, .failed: restore()
      default: break
      }
    }
    private func restore() {
      UIView.animate(withDuration: UIAccessibility.isReduceMotionEnabled ? 0 : 0.3, delay: 0, options: [.curveEaseOut, .beginFromCurrentState], animations: {
        self.canvas.transform = .identity; self.view.backgroundColor = .black
        self.topBar.alpha = self.controlsHidden ? 0 : 1; self.bottomBar.alpha = self.controlsHidden ? 0 : 1
      })
    }
    override func accessibilityPerformEscape() -> Bool {
      guard !retiring else { return false }
      close()
      return true
    }
    @objc private func close() { guard !retiring else { return }; retiring = true; stop(); closed?() }
    @objc private func share() {
      guard !retiring, presentedViewController == nil else { return }
      playerController?.player?.pause()
      let activity = UIActivityViewController(activityItems: [file], applicationActivities: nil)
      activity.popoverPresentationController?.sourceView = shareButton
      activity.popoverPresentationController?.sourceRect = shareButton.bounds
      present(activity, animated: !UIAccessibility.isReduceMotionEnabled)
    }
  }
}
