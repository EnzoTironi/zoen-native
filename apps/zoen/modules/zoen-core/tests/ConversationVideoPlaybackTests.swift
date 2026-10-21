import AVFoundation
import Foundation

/// Runs the production playback-only transport with real host AVPlayer items.
/// No UIKit, network, exporter, FilePreview double or document store is used.
@main @MainActor struct ConversationVideoPlaybackTests {
  static func main() async throws {
    let input = URL(fileURLWithPath: CommandLine.arguments[1])
    func wait(_ condition: () -> Bool) async throws {
      for _ in 0..<500 {
        if condition() { return }
        try await Task.sleep(for: .milliseconds(10))
      }
      preconditionFailure("Timed out awaiting real AVPlayer state")
    }
    let item = AVPlayerItem(url: input), player = AVPlayer()
    player.replaceCurrentItem(with: item)
    let transport = ZoenConversationVideoPlayback(player: player)
    var emissions = 0
    transport.changed = { _ in emissions += 1 }
    transport.setActive(true)
    try await wait { transport.state.seekable }
    precondition(transport.state.duration > 5 && transport.state.duration < 7)
    transport.play(); precondition(transport.state.playing)
    try await wait { player.currentTime().seconds > 0.05 }
    transport.pause(); precondition(player.rate == 0 && !transport.state.playing)
    print("PASS real playback advances and pauses")

    transport.seek(to: 2)
    try await wait { abs(player.currentTime().seconds - 2) < 0.02 }
    transport.seek(by: 15); precondition(transport.state.position == transport.state.duration)
    transport.seek(by: -15); precondition(transport.state.position == 0)
    transport.seek(to: .nan); precondition(transport.state.position == 0)
    try await wait { player.currentTime().seconds < 0.02 }
    print("PASS real coalesced seek,15s bounds and nonfinite rejection")

    transport.seek(to: 2)
    try await wait { abs(player.currentTime().seconds - 2) < 0.02 }
    transport.beginScrubbing(); transport.updateScrubbing(fraction: 0.5)
    precondition(transport.state.scrubbing && abs(transport.state.position - 5) < 0.1)
    precondition(abs(player.currentTime().seconds - 2) < 0.02)
    transport.endScrubbing(apply: false)
    precondition(!transport.state.scrubbing && abs(transport.state.position - 2) < 0.02)
    transport.play(); transport.beginScrubbing(); transport.updateScrubbing(fraction: 0.25)
    transport.endScrubbing(apply: true)
    precondition(transport.state.playing && !transport.state.scrubbing)
    try await wait { player.currentTime().seconds > 3.4 }
    print("PASS relative scrub commits actual seek; cancel restores; prior playback resumes")

    transport.seek(to: 4)
    transport.setActive(false)
    let inactivePosition = transport.state.position
    transport.play(); transport.seek(to: 1); transport.beginScrubbing()
    try await Task.sleep(for: .milliseconds(80))
    precondition(!transport.state.playing && !transport.state.scrubbing && player.rate == 0)
    precondition(transport.state.position == inactivePosition)
    transport.setActive(true)
    precondition(!transport.state.playing && player.rate == 0)
    print("PASS inactive/late seek cannot revive playback; reactivation stays paused")

    transport.play(); transport.beginScrubbing(); transport.updateScrubbing(fraction: -1)
    transport.setActive(false); transport.endScrubbing(apply: true)
    precondition(!transport.state.scrubbing && !transport.state.playing && player.rate == 0)
    transport.retire()
    let count = emissions
    transport.setActive(true); transport.play(); transport.seek(to: 1)
    NotificationCenter.default.post(name: .AVPlayerItemDidPlayToEndTime, object: item)
    try await Task.sleep(for: .milliseconds(80))
    precondition(transport.retired && !transport.active && emissions == count && player.rate == 0)
    print("PASS retirement removes callbacks and rejects stale play/seek/end notification")

    // Unlike the editing adapter, playback has no exactly-one-track gate.
    let asset = AVURLAsset(url: input), tracks = try await asset.loadTracks(withMediaType: .video)
    let duration = try await asset.load(.duration)
    let composition = AVMutableComposition()
    for _ in 0..<2 {
      let track = composition.addMutableTrack(withMediaType: .video, preferredTrackID: kCMPersistentTrackID_Invalid)!
      try track.insertTimeRange(CMTimeRange(start: .zero, duration: duration), of: tracks[0], at: .zero)
    }
    let multi = AVPlayerItem(asset: composition), multiPlayer = AVPlayer(playerItem: multi)
    let playbackOnly = ZoenConversationVideoPlayback(player: multiPlayer)
    playbackOnly.setActive(true)
    try await wait { playbackOnly.state.seekable }
    precondition(!playbackOnly.state.failed && playbackOnly.state.duration > 5)
    playbackOnly.play(); precondition(playbackOnly.state.playing)
    playbackOnly.retire()
    print("PASS playback admits real two-video-track composition without editor gates")

    let empty = ZoenConversationVideoPlayback(player: AVPlayer())
    empty.setActive(true); empty.play(); empty.seek(to: 2)
    precondition(empty.state.failed && !empty.state.playing && !empty.state.seekable)
    empty.retire()
    print("PASS missing media fails closed without manufactured duration")
    print("7 production AVPlayer lifecycle scenarios PASS")
  }
}
