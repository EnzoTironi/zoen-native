// SPDX-License-Identifier: GPL-3.0-or-later
import AVFoundation
import Foundation

/// Ephemeral transport for an already authorized FilePreview copy. It never
/// opens files, exports, edits, owns plaintext copies or changes document state.
@MainActor final class ZoenConversationVideoPlayback {
  struct State: Equatable {
    var position: Double = 0
    var duration: Double = 0
    var playing = false
    var scrubbing = false
    var failed = false
    var seekable: Bool { duration.isFinite && duration > 0 && !failed }
  }
  let player: AVPlayer
  private(set) var state = State()
  var changed: ((State) -> Void)?
  private(set) var active = false
  private(set) var retired = false
  private var itemObservation: NSKeyValueObservation?
  private var durationObservation: NSKeyValueObservation?
  private var timeObserver: Any?
  private var endObserver: NSObjectProtocol?
  private var generation = UUID()
  private var seekInFlight = false
  private var requestedSeek: CMTime?
  private var scrubOrigin: Double = 0
  private var wasPlaying = false

  init(player: AVPlayer) {
    self.player = player
    guard let item = player.currentItem else { state.failed = true; return }
    itemObservation = item.observe(\.status, options: [.initial, .new]) { [weak self, weak item] _, _ in
      Task { @MainActor [weak self, weak item] in
        guard let self, !self.retired, let item, self.player.currentItem === item else { return }
        self.updateItem(item)
      }
    }
    durationObservation = item.observe(\.duration, options: [.initial, .new]) { [weak self, weak item] _, _ in
      Task { @MainActor [weak self, weak item] in
        guard let self, !self.retired, let item, self.player.currentItem === item else { return }
        self.updateItem(item)
      }
    }
    endObserver = NotificationCenter.default.addObserver(forName: .AVPlayerItemDidPlayToEndTime,
      object: item, queue: .main) { [weak self, weak item] _ in
      MainActor.assumeIsolated {
        guard let self, self.active, !self.retired, let item, self.player.currentItem === item else { return }
        self.state.position = self.state.duration; self.pause()
      }
    }
  }

  deinit {
    if let timeObserver { player.removeTimeObserver(timeObserver) }
    if let endObserver { NotificationCenter.default.removeObserver(endObserver) }
  }

  private func updateItem(_ item: AVPlayerItem) {
    state.failed = item.status == .failed
    let duration = item.duration.seconds
    state.duration = duration.isFinite && duration > 0 ? duration : 0
    if state.failed { player.pause(); state.playing = false }
    publish()
  }

  func setActive(_ value: Bool) {
    guard !retired, active != value else { return }
    active = value
    if value {
      let expected = generation
      timeObserver = player.addPeriodicTimeObserver(forInterval: CMTime(value: 1, timescale: 30), queue: .main) { [weak self] time in
        MainActor.assumeIsolated {
          guard let self, self.active, !self.retired, self.generation == expected,
            !self.state.scrubbing, self.requestedSeek == nil, time.seconds.isFinite else { return }
          self.state.position = max(0, time.seconds); self.publish()
        }
      }
    } else {
      generation = UUID(); player.pause(); player.currentItem?.cancelPendingSeeks()
      requestedSeek = nil; seekInFlight = false
      if let timeObserver { player.removeTimeObserver(timeObserver); self.timeObserver = nil }
      state.playing = false; state.scrubbing = false; wasPlaying = false
      publish()
    }
  }

  func toggle() { state.playing ? pause() : play() }
  func play() {
    guard active, !retired, !state.failed, !state.scrubbing else { return }
    if state.seekable, state.position >= state.duration { seek(to: 0) }
    state.playing = true; player.play(); publish()
  }
  func pause() {
    guard !retired else { return }
    player.pause(); state.playing = false; publish()
  }
  func seek(by delta: Double) { seek(to: state.position + delta) }
  func seek(to seconds: Double) {
    guard active, !retired, state.seekable, seconds.isFinite else { return }
    state.position = max(0, min(state.duration, seconds))
    requestedSeek = CMTime(seconds: state.position, preferredTimescale: 1_000_000_000)
    publish(); chaseSeek()
  }
  private func chaseSeek() {
    guard active, !retired, !seekInFlight, let requested = requestedSeek else { return }
    seekInFlight = true
    let expected = generation
    player.seek(to: requested, toleranceBefore: .zero, toleranceAfter: .zero) { [weak self] _ in
      Task { @MainActor [weak self] in
        guard let self, self.active, !self.retired, self.generation == expected else { return }
        self.seekInFlight = false
        if self.requestedSeek == requested { self.requestedSeek = nil }
        self.chaseSeek()
      }
    }
  }

  func beginScrubbing() {
    guard active, !retired, state.seekable, !state.scrubbing else { return }
    scrubOrigin = state.position; wasPlaying = state.playing
    player.pause(); state.scrubbing = true; state.playing = false; publish()
  }
  /// MediaPlayerScrubbingNode:635–636 clamps origin + duration * fraction.
  func updateScrubbing(fraction: Double) {
    guard active, !retired, state.scrubbing, fraction.isFinite else { return }
    state.position = max(0, min(state.duration, scrubOrigin + state.duration * fraction)); publish()
  }
  func endScrubbing(apply: Bool) {
    guard active, !retired, state.scrubbing else { return }
    let destination = apply ? state.position : scrubOrigin, resume = wasPlaying
    state.scrubbing = false; wasPlaying = false
    seek(to: destination)
    if resume { play() }
  }
  func retire() {
    guard !retired else { return }
    setActive(false); retired = true; generation = UUID()
    player.pause(); itemObservation = nil; durationObservation = nil
    if let endObserver { NotificationCenter.default.removeObserver(endObserver); self.endObserver = nil }
    changed = nil
  }
  private func publish() { changed?(state) }
}
