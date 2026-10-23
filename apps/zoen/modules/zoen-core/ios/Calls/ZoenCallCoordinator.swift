import AVFoundation
import CallKit
import UIKit
@preconcurrency import WebRTC

/// System UI/media are transient resources lent by one exact ZoenSession generation.
/// Core decisions survive process death; this object and its handles do not.
@MainActor
final class ZoenCallCoordinator: NSObject {
  let id = UUID()
  let port: ZoenCallPort
  private let provider: CXProvider
  private let controller = CXCallController()
  private var mediaIntent = ZoenCallMediaIntent()
  private var rtc: ZoenCallRTC?
  private var call: ZoenCallRecord
  private var screen: ZoenCallViewController?
  private var watchdog: Task<Void, Never>?
  private var deadline: Task<Void, Never>?
  private var retired = false
  private var installed = false
  private var audio: AVAudioSession?
  private let name: String
  private let status: @MainActor (String) -> Void

  init(port: ZoenCallPort, record: ZoenCallRecord, name: String, status: @escaping @MainActor (String) -> Void) {
    self.port = port; self.call = record; self.name = name; self.status = status
    provider = ZoenCallSystem.shared.provider
    super.init()
  }

  func begin(outgoing: Bool) async throws {
    guard !retired, !call.terminal, ZoenCallSystem.shared.install(self) else { throw ZoenCallFailure.retired }
    installed = true
    if outgoing {
      try await port.prepareAudio()
      let action = CXStartCallAction(call: id, handle: CXHandle(type: .generic, value: name))
      action.isVideo = call.video
      try await controller.request(CXTransaction(action: action))
    } else {
      guard call.ringing(at: Int64(Date().timeIntervalSince1970 * 1000)) else {
        try? await port.action("decline")
        await stop(writeEnd: false)
        return
      }
      let update = CXCallUpdate()
      update.remoteHandle = CXHandle(type: .generic, value: name)
      update.localizedCallerName = name
      update.hasVideo = call.video
      update.supportsDTMF = false; update.supportsGrouping = false; update.supportsUngrouping = false
      try await provider.reportNewIncomingCall(with: id, update: update)
    }
    status(outgoing ? "calling" : "incoming")
    let remaining = max(0, call.expiresAtMs - Int64(Date().timeIntervalSince1970 * 1000))
    deadline = Task { [weak self] in
      do { try await Task.sleep(for: .milliseconds(remaining)) } catch { return }
      guard let self, !self.retired, self.call.acceptedDevice == nil else { return }
      await self.stop(writeEnd: true, reason: .unanswered)
    }
    watchCanonicalLease()
    try await refresh()
  }

  private func watchCanonicalLease() {
    watchdog = Task { [weak self] in
      var offlineSince: ContinuousClock.Instant?
      while !Task.isCancelled {
        do { try await Task.sleep(for: .seconds(1)) } catch { return }
        guard let self, !self.retired else { return }
        do { try await self.refresh() }
        catch { await self.stop(writeEnd: false, reason: .failed); return }
        if await self.port.relayReady() { offlineSince = nil }
        else {
          let now = ContinuousClock.now
          if let offlineSince, offlineSince.duration(to: now) >= .seconds(30) {
            await self.stop(writeEnd: true, reason: .failed); return
          }
          if offlineSince == nil { offlineSince = now }
        }
      }
    }
  }

  func refresh() async throws {
    guard !retired else { return }
    let record = try await port.current()
    guard !retired else { return }
    call = record
    if record.terminal { await stop(writeEnd: false, reason: .remoteEnded); return }
    if let chosen = record.acceptedDevice, record.callee == port.scope.identity, chosen != port.scope.device {
      await stop(writeEnd: false, reason: .answeredElsewhere); return
    }
    if record.acceptanceConfirmed, record.invitationConfirmed, record.acceptedDevice != nil {
      deadline?.cancel(); deadline = nil
      if rtc == nil {
        let rtc = ZoenCallRTC(port: port, intent: mediaIntent, changed: { [weak self] phase in self?.phase(phase) },
          failed: { [weak self] in Task { await self?.stop(writeEnd: true, reason: .failed) } },
          remoteVideo: { [weak self] track in self?.screen?.showVideo(track) })
        self.rtc = rtc
        rtc.start()
        if let audio { rtc.audioActivated(audio) }
      }
      rtc?.refresh()
    }
  }

  func show() {
    guard !retired else { return }
    if let screen { screen.presentIfNeeded(); return }
    let view = ZoenCallViewController(name: name, video: call.video, locale: port.locale)
    view.end = { [weak self] in Task { await self?.requestEnd() } }
    view.camera = { [weak self] value in self?.mediaIntent.cameraEnabled = value; self?.rtc?.setCameraEnabled(value) }
    view.mute = { [weak self] value in self?.requestMute(value) }
    view.speaker = { [weak self] value in self?.mediaIntent.speaker = value; try? self?.rtc?.setSpeaker(value) }
    screen = view
    view.presentIfNeeded()
    view.setIntent(mediaIntent)
  }

  private func phase(_ phase: String) {
    guard !retired else { return }
    status(phase); screen?.setPhase(phase)
    if phase == "connected", call.callerDevice == port.scope.device {
      provider.reportOutgoingCall(with: id, connectedAt: Date())
    }
  }

  private func requestMute(_ value: Bool) {
    guard !retired else { return }
    // Apply privacy intent synchronously; CallKit transaction latency or failure
    // must not let a pending permission callback briefly enable the microphone.
    mediaIntent.muted = value; rtc?.setMuted(value)
    Task {
      try? await controller.request(CXTransaction(action: CXSetMutedCallAction(call: id, muted: value)))
    }
  }

  func requestEnd() async {
    guard !retired else { return }
    do { try await controller.request(CXTransaction(action: CXEndCallAction(call: id))) }
    catch { await stop(writeEnd: true, reason: .failed) }
  }

  func stop(writeEnd: Bool, reason: CXCallEndedReason = .remoteEnded) async {
    guard !retired else { return }
    guard installed else { retired = true; status("ended"); return }
    let retirement = ZoenCallSystem.shared.retire(self) { [port] in await port.releaseAudio() }
    retired = true; deadline?.cancel(); deadline = nil; watchdog?.cancel(); watchdog = nil
    await rtc?.stop(); rtc = nil
    retirement.mediaDidStop()
    // Stop all capture before an awaited write. Failure stays in the canonical
    // outbox/recovery path; no SDK callback may revive this resource lifetime.
    if writeEnd {
      let action = call.acceptedDevice == nil ? (call.caller == port.scope.identity ? "cancel" : "decline") : "hang_up"
      try? await port.action(action)
    }
    provider.reportCall(with: id, endedAt: Date(), reason: reason)
    ZoenCallSystem.shared.remove(self)
    screen?.dismiss(animated: true); screen = nil
    audio = nil; status("ended")
  }

  func startAction(_ action: CXStartCallAction) async {
    guard !retired, action.callUUID == id else { action.fail(); return }
    do {
      call = try await port.current()
      guard !retired, !call.terminal else { throw ZoenCallFailure.retired }
      provider.reportOutgoingCall(with: id, startedConnectingAt: Date())
      action.fulfill(); show()
    } catch { action.fail(); await stop(writeEnd: true, reason: .failed) }
  }

  func answerAction(_ action: CXAnswerCallAction) async {
    guard !retired, action.callUUID == id else { action.fail(); return }
    do {
      try await port.prepareAudio()
      try await port.action("accept")
      guard !retired else { action.fail(); return }
      action.fulfill(); show()
      // A queued local accept is not media permission: refresh waits for its ACK.
      try await refresh()
    } catch { action.fail(); await stop(writeEnd: true, reason: .failed) }
  }

  func endAction(_ action: CXEndCallAction) async {
    guard action.callUUID == id else { action.fail(); return }
    await stop(writeEnd: true); action.fulfill()
  }
  func reset() { Task { await stop(writeEnd: true, reason: .failed) } }
  func activate(_ session: AVAudioSession) {
    guard !retired else { return }
    do {
      try session.setCategory(.playAndRecord, mode: call.video ? .videoChat : .voiceChat, options: [.allowBluetoothHFP])
      audio = session; rtc?.audioActivated(session)
    } catch { Task { await stop(writeEnd: true, reason: .failed) } }
  }
  func deactivate(_ session: AVAudioSession) {
    audio = nil
    rtc?.audioDeactivated(session)
  }
  func muteAction(_ action: CXSetMutedCallAction) {
    guard !retired, action.callUUID == id else { action.fail(); return }
    mediaIntent.muted = action.isMuted
    rtc?.setMuted(action.isMuted); screen?.setMuted(action.isMuted); action.fulfill()
  }
}
