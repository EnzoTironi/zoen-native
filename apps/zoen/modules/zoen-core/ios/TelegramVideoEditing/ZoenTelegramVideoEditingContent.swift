import AVFoundation
import UIKit

struct ZoenTelegramVideoState: Equatable, Sendable {
  var trimStart: Double
  var trimEnd: Double
  var position: Double
  var sendAsGif: Bool
  var range: CMTimeRange {
    CMTimeRange(start: CMTime(seconds: trimStart, preferredTimescale: 1_000_000_000),
                duration: CMTime(seconds: trimEnd - trimStart, preferredTimescale: 1_000_000_000))
  }
}

/// The actual pinned scrubber/trim/thumbnail/header views drive this reusable
/// player. State is a display projection; selection, media bytes, draft revision
/// and delivery remain owned by the host's canonical capture/CAS path.
@MainActor final class ZoenTelegramVideoEditingContent: NSObject,
  ZoenTGMediaPickerGalleryVideoScrubberDataSource, ZoenTGMediaPickerGalleryVideoScrubberDelegate, UIGestureRecognizerDelegate {
  let view = ZoenTelegramVideoSurface()
  let headerView = ZoenTGMediaPickerScrubberHeaderView(frame: .zero)
  let muteButton = UIButton(type: .custom)
  let asset: AVAsset
  private(set) var state: ZoenTelegramVideoState
  let initialState: ZoenTelegramVideoState
  var changed: ((ZoenTelegramVideoState) -> Void)?
  /// The host coordinates its existing audio focus policy. This component never
  /// overwrites the app's AVAudioSession while recording or calling.
  var willPlay: (() throws -> Void)?
  var didPause: (() -> Void)?
  var failed: ((Error) -> Void)?
  private let player: AVPlayer
  private let scrubber = ZoenTGMediaPickerGalleryVideoScrubber(frame: .zero, cover: false)!
  private let playButton = UIButton(type: .custom)
  private let info = UILabel()
  private let dimensions: CGSize
  private let duration: Double
  private let glyphs: ZoenTelegramVideoGlyphs
  private let fileInfo: String?
  private var timeObserver: Any?
  private var endObserver: NSObjectProtocol?
  private var inactiveObserver: NSObjectProtocol?
  private var isVisible = false
  private var retired = false
  private var isPlaying = false
  private var wasPlayingBeforeScrubbing = false
  private var seekInFlight = false
  private var requestedSeek: CMTime?
  private var summaryTask: Task<Void, Never>?
  private var zoomTask: Task<Void, Never>?
  private var summaryGenerator: AVAssetImageGenerator?
  private var zoomGenerator: AVAssetImageGenerator?
  private var summaryGeneration = UUID()
  private var zoomGeneration = UUID()
  private var headerWidth: CGFloat = 0

  static func prepare(asset: AVAsset, initialState: ZoenTelegramVideoState? = nil,
                      allowsTrimming: Bool, glyphs: ZoenTelegramVideoGlyphs,
                      fileInfo: String? = nil) async throws -> ZoenTelegramVideoEditingContent {
    guard try await asset.load(.isPlayable), try await asset.load(.isExportable) else {
      throw ZoenTelegramVideoFailure.unavailable
    }
    let duration = try await asset.load(.duration)
    let tracks = try await asset.loadTracks(withMediaType: .video)
    guard duration.seconds.isFinite, duration.seconds > 0, tracks.count == 1, let track = tracks.first else {
      throw ZoenTelegramVideoFailure.unavailable
    }
    let size = try await track.load(.naturalSize)
    let transformed = size.applying(try await track.load(.preferredTransform))
    let dimensions = CGSize(width: abs(transformed.width), height: abs(transformed.height))
    guard dimensions.width.isFinite, dimensions.height.isFinite, dimensions.width > 0, dimensions.height > 0 else {
      throw ZoenTelegramVideoFailure.unavailable
    }
    var state = initialState ?? ZoenTelegramVideoState(trimStart: 0, trimEnd: duration.seconds, position: 0, sendAsGif: false)
    try ZoenTelegramVideoRenderer.validate(range: state.range, duration: duration)
    guard state.position.isFinite else { throw ZoenTelegramVideoFailure.invalidRange }
    state.position = max(state.trimStart, min(state.trimEnd, state.position))
    try Task.checkCancellation()
    return ZoenTelegramVideoEditingContent(asset: asset, duration: duration.seconds, dimensions: dimensions,
      state: state, allowsTrimming: allowsTrimming, glyphs: glyphs, fileInfo: fileInfo)
  }

  private init(asset: AVAsset, duration: Double, dimensions: CGSize, state: ZoenTelegramVideoState,
               allowsTrimming: Bool, glyphs: ZoenTelegramVideoGlyphs, fileInfo: String?) {
    self.asset = asset; self.duration = duration; self.dimensions = dimensions
    self.state = state; self.initialState = state; self.glyphs = glyphs
    self.fileInfo = fileInfo
    player = AVPlayer(playerItem: AVPlayerItem(asset: asset))
    super.init()
    view.layerPlayer.player = player
    view.layerPlayer.videoGravity = .resizeAspect
    view.backgroundColor = .black
    view.layout = { [weak self] in self?.layoutMedia() }
    let tap = UITapGestureRecognizer(target: self, action: #selector(togglePlay))
    tap.delegate = self; view.addGestureRecognizer(tap)
    playButton.setImage(glyphs.play, for: .normal)
    playButton.tintColor = .white
    playButton.backgroundColor = UIColor.black.withAlphaComponent(0.3)
    playButton.layer.cornerRadius = 30
    playButton.accessibilityLabel = NSLocalizedString("Play video", comment: "Gallery video playback")
    playButton.addTarget(self, action: #selector(togglePlay), for: .touchUpInside)
    view.addSubview(playButton)
    muteButton.backgroundColor = UIColor.black.withAlphaComponent(0.5)
    muteButton.layer.cornerRadius = 20
    muteButton.addTarget(self, action: #selector(toggleMute), for: .touchUpInside)
    muteButton.accessibilityLabel = NSLocalizedString("Send without sound", comment: "Gallery send video as animation")
    view.addSubview(muteButton)
    let panel = UIView(frame: CGRect(x: 0, y: 0, width: 320, height: 64))
    panel.autoresizingMask = [.flexibleWidth]
    headerView.panelView = panel; headerView.addSubview(panel)
    // VideoItemView224–247: 64-point panel, scrubber at headerHeight-44,
    // 68-point control. The original header expands its hit region by16.
    scrubber.frame = CGRect(x: 0, y: 20, width: 320, height: 68)
    scrubber.autoresizingMask = [.flexibleWidth]
    scrubber.delegate = self; scrubber.dataSource = self
    scrubber.allowsTrimming = allowsTrimming && duration >= 1.5
    scrubber.trimStartValue = state.trimStart; scrubber.trimEndValue = state.trimEnd
    scrubber.value = state.position
    scrubber.setTrimApplied(state.trimStart > 0 || state.trimEnd < duration)
    headerView.scrubberView = scrubber; panel.addSubview(scrubber)
    info.frame = CGRect(x: 0, y: 10, width: 320, height: 21)
    info.autoresizingMask = [.flexibleWidth]
    info.textColor = .white; info.textAlignment = .center; info.font = .systemFont(ofSize: 13)
    panel.addSubview(info)
    updateInfo(); updateMute(); updatePlayerRange()
    timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(value: 1, timescale: 30), queue: .main) { [weak self] time in
      MainActor.assumeIsolated {
        guard let self, self.isVisible, !self.retired, time.seconds.isFinite else { return }
        self.state.position = time.seconds
        self.scrubber.value = time.seconds
      }
    }
    endObserver = NotificationCenter.default.addObserver(forName: .AVPlayerItemDidPlayToEndTime,
      object: player.currentItem, queue: .main) { [weak self] _ in
      MainActor.assumeIsolated { self?.playbackEnded() }
    }
    inactiveObserver = NotificationCenter.default.addObserver(forName: UIApplication.willResignActiveNotification,
      object: nil, queue: .main) { [weak self] _ in
      MainActor.assumeIsolated { self?.setVisible(false) }
    }
    seek(state.position)
  }

  deinit {
    summaryTask?.cancel(); zoomTask?.cancel()
    summaryGenerator?.cancelAllCGImageGeneration(); zoomGenerator?.cancelAllCGImageGeneration()
    if let timeObserver { player.removeTimeObserver(timeObserver) }
    if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
    if let inactiveObserver { NotificationCenter.default.removeObserver(inactiveObserver) }
  }

  var hasChanges: Bool {
    state.trimStart != initialState.trimStart || state.trimEnd != initialState.trimEnd || state.sendAsGif != initialState.sendAsGif
  }

  func updateLayout(safeArea: UIEdgeInsets) {
    headerView.safeAreaInset = safeArea
    headerView.layoutIfNeeded()
    if isVisible, headerView.bounds.width > 0, headerView.bounds.width != headerWidth {
      headerWidth = headerView.bounds.width
      scrubber.reloadDataAndReset(false)
    }
    layoutMedia()
  }

  private func layoutMedia() {
    playButton.frame = CGRect(x: floor((view.bounds.width - 60) / 2), y: floor((view.bounds.height - 60) / 2), width: 60, height: 60)
    muteButton.frame = CGRect(x: 12, y: max(0, view.bounds.height - 192 - view.safeAreaInsets.bottom), width: 40, height: 40)
  }

  func setVisible(_ visible: Bool) {
    guard !retired, visible != isVisible else { return }
    isVisible = visible
    if visible {
      scrubber.resetThumbnails(); scrubber.reloadData()
      if state.sendAsGif { play() }
    }
    else {
      pause(); scrubber.zoenSuspend(); cancelThumbnails(summary: true); cancelThumbnails(summary: false)
      requestedSeek = nil
    }
  }

  func retire(sensitive: Bool = false) {
    guard !retired else { return }
    setVisible(false); retired = true
    player.pause(); player.replaceCurrentItem(with: nil)
    view.layerPlayer.player = nil
    view.isUserInteractionEnabled = false; headerView.isUserInteractionEnabled = false
    if sensitive { view.isHidden = true; headerView.isHidden = true }
    changed = nil; failed = nil; willPlay = nil; didPause = nil
  }

  @objc private func togglePlay() { if isPlaying { pause() } else { play() } }
  func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
    var candidate = touch.view
    while let current = candidate, current !== view {
      if current is UIControl { return false }; candidate = current.superview
    }
    return true
  }
  func play() {
    guard isVisible, !retired, !isPlaying else { return }
    do { try willPlay?() } catch { failed?(error); return }
    if state.position >= state.trimEnd || state.position < state.trimStart { seek(state.trimStart) }
    player.play(); isPlaying = true; scrubber.isPlaying = true; showPlay(false, animated: true)
  }
  func pause() {
    let wasPlaying = isPlaying
    player.pause(); isPlaying = false; scrubber.isPlaying = false
    showPlay(true, animated: true)
    if wasPlaying { didPause?() }
  }
  private func showPlay(_ visible: Bool, animated: Bool) {
    let changes = { self.playButton.alpha = visible ? 1 : 0 }
    playButton.isUserInteractionEnabled = visible
    if animated, !UIAccessibility.isReduceMotionEnabled { UIView.animate(withDuration: 0.15, animations: changes) }
    else { changes() }
  }
  private func playbackEnded() {
    guard isVisible, !retired else { return }
    pause(); seek(state.trimStart)
    scrubber.setValue(state.trimStart, resetPosition: true)
    if state.sendAsGif { play() }
  }

  @objc func toggleMute() {
    guard isVisible, !retired else { return }
    state.sendAsGif.toggle(); updateMute(); changed?(state)
    if state.sendAsGif && !isPlaying { play() }
  }
  private func updateMute() {
    player.isMuted = state.sendAsGif
    muteButton.setImage(state.sendAsGif ? glyphs.gif : glyphs.mute, for: .normal)
    muteButton.tintColor = state.sendAsGif ? UIColor(red: 1, green: 211.0 / 255, blue: 0, alpha: 1) : .white
    muteButton.accessibilityValue = state.sendAsGif ? NSLocalizedString("On", comment: "Video without sound enabled") : NSLocalizedString("Off", comment: "Video without sound disabled")
    muteButton.accessibilityTraits = state.sendAsGif ? [.button, .selected] : .button
  }
  private func updatePlayerRange() { player.currentItem?.forwardPlaybackEndTime = state.range.end }
  private func updateInfo() {
    // Upstream file info is actual source extension/size/dimensions, not an
    // invented estimate. The authorized host supplies it when available.
    info.text = fileInfo
  }

  private func seek(_ seconds: Double) {
    guard !retired, seconds.isFinite else { return }
    let value = max(0, min(duration, seconds))
    state.position = value
    requestedSeek = CMTime(seconds: value, preferredTimescale: 1_000_000_000)
    chaseSeek()
  }
  private func chaseSeek() {
    guard !seekInFlight, let requested = requestedSeek, !retired else { return }
    seekInFlight = true
    player.seek(to: requested, toleranceBefore: .zero, toleranceAfter: .zero) { [weak self] _ in
      Task { @MainActor in
        guard let self, !self.retired else { return }
        self.seekInFlight = false
        if self.requestedSeek == requested { self.requestedSeek = nil }
        self.chaseSeek()
      }
    }
  }

  func videoScrubberDuration(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) -> TimeInterval { duration }
  func videoScrubber(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!, evenlySpacedTimestamps count: Int,
                     startingAt startTimestamp: TimeInterval, endingAt endTimestamp: TimeInterval) -> [Any]! {
    guard count > 0, count <= 256, startTimestamp.isFinite, endTimestamp.isFinite else { return [] }
    let end = min(duration, endTimestamp), start = max(0, startTimestamp)
    guard end >= start else { return [] }
    // VideoItemView1672–1692 uses count evenly spaced samples, excluding end.
    let step = (end - start) / Double(count)
    return (0..<count).map { NSNumber(value: start + Double($0) * step) }
  }
  func videoScrubberThumbnailAspectRatio(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) -> CGFloat { dimensions.width / dimensions.height }
  func videoScrubberOriginalSize(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!, cropRect: UnsafeMutablePointer<CGRect>!,
                                cropOrientation: UnsafeMutablePointer<UIImage.Orientation>!, cropMirrored: UnsafeMutablePointer<Bool>!) -> CGSize {
    cropRect?.pointee = CGRect(origin: .zero, size: dimensions)
    cropOrientation?.pointee = .up; cropMirrored?.pointee = false
    return dimensions
  }
  func videoScrubber(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!, requestThumbnailImagesForTimestamps timestamps: [Any]!,
                     size: CGSize, isSummaryThumbnails summary: Bool) {
    cancelThumbnails(summary: summary)
    guard isVisible, !retired, let timestamps, timestamps.count <= 256,
          size.width.isFinite, size.height.isFinite, size.width > 0, size.height > 0 else { return }
    let values = timestamps.compactMap { ($0 as? NSNumber)?.doubleValue }
    guard values.count == timestamps.count, values.allSatisfy({ $0.isFinite && $0 >= 0 && $0 <= duration }) else { return }
    let generation = UUID(), generator = AVAssetImageGenerator(asset: asset)
    generator.appliesPreferredTrackTransform = true
    generator.maximumSize = CGSize(width: min(1024, size.width), height: min(1024, size.height))
    if summary { summaryGeneration = generation; summaryGenerator = generator }
    else { zoomGeneration = generation; zoomGenerator = generator }
    let task = Task { @MainActor [weak self] in
      for (index, value) in values.enumerated() {
        guard let self, self.isVisible, !self.retired, !Task.isCancelled,
              (summary ? self.summaryGeneration : self.zoomGeneration) == generation else { return }
        do {
          let (image, _) = try await generator.image(at: CMTime(seconds: value, preferredTimescale: 1_000_000_000))
          guard self.isVisible, !self.retired, !Task.isCancelled,
                (summary ? self.summaryGeneration : self.zoomGeneration) == generation else { return }
          videoScrubber.setThumbnailImage(UIImage(cgImage: image), forTimestamp: value, index: index,
            isSummaryThubmnail: summary, last: index == values.count - 1)
        } catch {
          guard !Task.isCancelled, self.isVisible, !self.retired else { return }
          self.failed?(error)
          // A failed last thumbnail must not leave the original zoom state pending.
          self.cancelThumbnails(summary: summary); self.scrubber.zoenSuspend()
          return
        }
      }
    }
    if summary { summaryTask = task } else { zoomTask = task }
  }
  private func cancelThumbnails(summary: Bool) {
    if summary { summaryGeneration = UUID(); summaryTask?.cancel(); summaryTask = nil; summaryGenerator?.cancelAllCGImageGeneration(); summaryGenerator = nil }
    else { zoomGeneration = UUID(); zoomTask?.cancel(); zoomTask = nil; zoomGenerator?.cancelAllCGImageGeneration(); zoomGenerator = nil }
  }
  func videoScrubberDidBeginScrubbing(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) {
    guard isVisible, !retired else { return }
    wasPlayingBeforeScrubbing = isPlaying; pause(); showPlay(false, animated: false)
  }
  func videoScrubberDidEndScrubbing(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) {
    guard isVisible, !retired else { return }
    if wasPlayingBeforeScrubbing { play() } else { showPlay(true, animated: true) }
  }
  func videoScrubber(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!, valueDidChange position: TimeInterval) {
    guard isVisible, !retired else { return }; seek(position)
  }
  func videoScrubberDidBeginEditing(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) {
    guard isVisible, !retired else { return }; pause(); showPlay(false, animated: false)
  }
  func videoScrubberDidEndEditing(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) {
    guard isVisible, !retired else { return }
    state.trimStart = scrubber.trimStartValue; state.trimEnd = scrubber.trimEndValue
    state.position = max(state.trimStart, min(state.trimEnd, state.position))
    seek(state.position)
    updatePlayerRange(); updateInfo(); changed?(state); showPlay(true, animated: true)
  }
  func videoScrubber(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!, editingStartValueDidChange start: TimeInterval) {
    guard isVisible, !retired else { return }; seek(start)
  }
  func videoScrubber(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!, editingEndValueDidChange end: TimeInterval) {
    guard isVisible, !retired else { return }; seek(end)
  }
  func videoScrubberDidFinishRequestingThumbnails(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) {}
  func videoScrubberDidCancelRequestingThumbnails(_ videoScrubber: ZoenTGMediaPickerGalleryVideoScrubber!) { cancelThumbnails(summary: false) }
}

@MainActor final class ZoenTelegramVideoSurface: UIView {
  override class var layerClass: AnyClass { AVPlayerLayer.self }
  var layerPlayer: AVPlayerLayer { layer as! AVPlayerLayer }
  var layout: (() -> Void)?
  override func layoutSubviews() { super.layoutSubviews(); layout?() }
}
