import AVFoundation
import Foundation
@preconcurrency import WebRTC

/// One transient peer for the two devices already chosen by the signed Core log.
/// This class never opens a Client, holds a vault, or decides who accepted a call.
@MainActor
final class ZoenCallRTC: NSObject {
  let port: ZoenCallPort
  let changed: @MainActor (String) -> Void
  let failed: @MainActor () -> Void
  let remoteVideo: @MainActor (RTCVideoTrack?) -> Void
  private let factory = RTCPeerConnectionFactory(encoderFactory: RTCDefaultVideoEncoderFactory(), decoderFactory: RTCDefaultVideoDecoderFactory())
  private var peer: RTCPeerConnection?
  private var microphone: RTCAudioTrack?
  private var camera: RTCCameraVideoCapturer?
  private var cameraTrack: RTCVideoTrack?
  private var tail: Task<Void, Never>?
  private var recoveryDeadline: Task<Void, Never>?
  private var received: Set<String> = []
  private var pendingICE: [RTCIceCandidate] = []
  private var generation: UInt8 = 0
  private var remoteGeneration: UInt8?
  private var live = true
  private var didStart = false
  private var audioActive = false
  private var microphoneAllowed = false
  private var cameraAllowed = false
  private var intent: ZoenCallMediaIntent
  private var cameraStarted = false

  init(port: ZoenCallPort, intent: ZoenCallMediaIntent, changed: @escaping @MainActor (String) -> Void,
       failed: @escaping @MainActor () -> Void, remoteVideo: @escaping @MainActor (RTCVideoTrack?) -> Void) {
    self.intent = intent; self.port = port; self.changed = changed; self.failed = failed; self.remoteVideo = remoteVideo
    super.init()
  }

  private func current() async throws -> ZoenCallRecord {
    guard live else { throw ZoenCallFailure.retired }
    let call = try await port.mediaRecord()
    guard live else { throw ZoenCallFailure.retired }
    return call
  }

  func start() { enqueue { try await $0.prepare() } }
  func refresh() { enqueue { try await $0.receive() } }
  private func enqueue(_ work: @escaping @MainActor (ZoenCallRTC) async throws -> Void) {
    let previous = tail
    tail = Task { [weak self] in
      await previous?.value
      guard let self, self.live else { return }
      do { try await work(self) }
      catch { if self.live { self.failed() } }
    }
  }

  private func prepare() async throws {
    guard !didStart else { return }
    let call = try await current()
    let turn = try await port.renewedTurn { live && !didStart }
    let configuration = RTCConfiguration()
    configuration.sdpSemantics = .unifiedPlan
    // Relay-only avoids disclosing either person's local/public IP to their peer.
    configuration.iceTransportPolicy = .relay
    configuration.iceServers = [RTCIceServer(urlStrings: turn.urls, username: turn.username, credential: turn.credential)]
    configuration.continualGatheringPolicy = .gatherContinually
    let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
    guard let peer = factory.peerConnection(with: configuration, constraints: constraints, delegate: self) else { throw ZoenCallFailure.unavailable }
    self.peer = peer
    RTCAudioSession.sharedInstance().useManualAudio = true
    RTCAudioSession.sharedInstance().isAudioEnabled = false
    let microphone = factory.audioTrack(with: factory.audioSource(with: constraints), trackId: "microphone")
    microphone.isEnabled = false
    self.microphone = microphone
    _ = peer.add(microphone, streamIds: ["call"])
    if call.video {
      let source = factory.videoSource()
      let capture = RTCCameraVideoCapturer(delegate: source)
      let track = factory.videoTrack(with: source, trackId: "camera")
      track.isEnabled = false
      camera = capture; cameraTrack = track
      _ = peer.add(track, streamIds: ["call"])
    }
    didStart = true
    changed("connecting"); boundRecovery()
    // Creating tracks alone never acquires microphone/camera. CallKit activation
    // plus explicit OS permission and the current Core lease must all succeed.
    microphoneAllowed = await AVAudioApplication.requestRecordPermission()
    _ = try await current()
    guard microphoneAllowed else { throw ZoenCallFailure.denied }
    if call.video && intent.cameraEnabled { cameraAllowed = await AVCaptureDevice.requestAccess(for: .video); _ = try await current() }
    updateAudio()
    if cameraAllowed && intent.cameraEnabled { try await startCamera() }
    if call.callerDevice == port.scope.device { try await offer() }
    try await receive()
  }

  private func renewTurn(_ connection: RTCPeerConnection, generation expected: UInt8) async throws {
    let turn = try await port.renewedTurn { live && peer === connection && generation == expected }
    let configuration = connection.configuration
    configuration.iceServers = [RTCIceServer(urlStrings: turn.urls, username: turn.username, credential: turn.credential)]
    guard connection.setConfiguration(configuration) else { throw ZoenCallFailure.negotiation }
  }

  private func offer() async throws {
    guard let peer else { throw ZoenCallFailure.retired }
    _ = try await current()
    let description: RTCSessionDescription = try await withCheckedThrowingContinuation { continuation in
      peer.offer(for: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)) { sdp, error in
        if let error { continuation.resume(throwing: error) }
        else if let sdp { continuation.resume(returning: sdp) }
        else { continuation.resume(throwing: ZoenCallFailure.negotiation) }
      }
    }
    try await local(description)
    _ = try await current()
    try await port.send(ZoenCallPayload(type: "offer", sdp: description.sdp), generation)
  }

  private func local(_ description: RTCSessionDescription) async throws {
    guard let peer else { throw ZoenCallFailure.retired }
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      peer.setLocalDescription(description) { error in
        if let error { continuation.resume(throwing: error) } else { continuation.resume() }
      }
    }
  }

  private func remote(_ description: RTCSessionDescription) async throws {
    guard let peer else { throw ZoenCallFailure.retired }
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      peer.setRemoteDescription(description) { error in
        if let error { continuation.resume(throwing: error) } else { continuation.resume() }
      }
    }
    let call = try await current()
    if !call.video {
      for transceiver in peer.transceivers where transceiver.mediaType == .video {
        transceiver.receiver.track?.isEnabled = false
        var failure: NSError?
        transceiver.setDirection(.inactive, error: &failure)
        if let failure { throw failure }
      }
    }
    remoteGeneration = generation
    let waiting = pendingICE; pendingICE.removeAll()
    for candidate in waiting { try await add(candidate) }
  }

  private func add(_ candidate: RTCIceCandidate) async throws {
    guard let peer else { throw ZoenCallFailure.retired }
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      peer.add(candidate) { error in
        if let error { continuation.resume(throwing: error) } else { continuation.resume() }
      }
    }
  }

  private func receive() async throws {
    _ = try await current()
    guard let peer else { return }
    let signals = try await port.transport()
    _ = try await current()
    for record in signals where !received.contains(record.signal.signalId) {
      let signal = record.signal
      guard signal.generation >= generation else { received.insert(signal.signalId); continue }
      guard signal.generation <= 7 else { throw ZoenCallFailure.exhausted }
      if signal.generation > generation { pendingICE.removeAll() }
      generation = signal.generation
      let payload = signal.payload
      switch payload.type {
      case "offer":
        guard let sdp = payload.sdp else { throw ZoenCallFailure.negotiation }
        // A network change may need a new allocation after REST credentials
        // expire, even though the previous allocation could still be alive.
        if generation > 0 { try await renewTurn(peer, generation: generation) }
        try await remote(RTCSessionDescription(type: .offer, sdp: sdp))
        let answer: RTCSessionDescription = try await withCheckedThrowingContinuation { continuation in
          peer.answer(for: RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)) { sdp, error in
            if let error { continuation.resume(throwing: error) }
            else if let sdp { continuation.resume(returning: sdp) }
            else { continuation.resume(throwing: ZoenCallFailure.negotiation) }
          }
        }
        try await local(answer)
        _ = try await current()
        try await port.send(ZoenCallPayload(type: "answer", sdp: answer.sdp), generation)
      case "answer":
        guard let sdp = payload.sdp else { throw ZoenCallFailure.negotiation }
        try await remote(RTCSessionDescription(type: .answer, sdp: sdp))
      case "ice":
        guard let candidate = payload.candidate, let line = payload.line else { throw ZoenCallFailure.negotiation }
        let ice = RTCIceCandidate(sdp: candidate, sdpMLineIndex: Int32(line), sdpMid: payload.mid)
        if remoteGeneration != generation { pendingICE.append(ice) } else { try await add(ice) }
      default: throw ZoenCallFailure.negotiation
      }
      received.insert(signal.signalId)
    }
  }

  func audioActivated(_ session: AVAudioSession) {
    guard live else { return }
    RTCAudioSession.sharedInstance().audioSessionDidActivate(session)
    audioActive = true; updateAudio()
  }
  func audioDeactivated(_ session: AVAudioSession) {
    guard live else { return }
    audioActive = false; updateAudio()
    RTCAudioSession.sharedInstance().audioSessionDidDeactivate(session)
  }
  private func updateAudio() {
    let enabled = live && audioActive && microphoneAllowed
    microphone?.isEnabled = intent.microphoneLive(allowed: microphoneAllowed, activated: audioActive, live: live)
    RTCAudioSession.sharedInstance().isAudioEnabled = enabled
    if enabled { try? AVAudioSession.sharedInstance().overrideOutputAudioPort(intent.speaker ? .speaker : .none) }
  }
  func setCameraEnabled(_ value: Bool) {
    intent.cameraEnabled = value
    cameraTrack?.isEnabled = intent.cameraLive(allowed: cameraAllowed, started: cameraStarted, live: live)
    guard didStart else { return }
    if !value { Task { await stopCamera() }; return }
    enqueue { rtc in try await rtc.startCamera() }
  }
  func setMuted(_ value: Bool) { intent.muted = value; updateAudio() }
  func setSpeaker(_ value: Bool) throws {
    intent.speaker = value
    if audioActive { try AVAudioSession.sharedInstance().overrideOutputAudioPort(value ? .speaker : .none) }
  }

  private func stopCamera() async {
    cameraTrack?.isEnabled = false
    guard cameraStarted, let camera else { return }
    cameraStarted = false
    await withCheckedContinuation { continuation in camera.stopCapture { continuation.resume() } }
  }

  private func boundRecovery() {
    guard recoveryDeadline == nil else { return }
    // A hung remote process or TURN outage must not hold the canonical call open
    // forever. This monotonic resource timer never resets on repeated callbacks.
    recoveryDeadline = Task { [weak self] in
      do { try await Task.sleep(for: .seconds(30)) } catch { return }
      guard let self, self.live else { return }
      self.failed()
    }
  }

  private func startCamera() async throws {
    guard live, intent.cameraEnabled, !cameraStarted else { return }
    if !cameraAllowed {
      cameraAllowed = await AVCaptureDevice.requestAccess(for: .video)
      _ = try await current()
    }
    guard cameraAllowed, intent.cameraEnabled else { return }
    guard let camera, let device = RTCCameraVideoCapturer.captureDevices().first(where: { $0.position == .front }) ?? RTCCameraVideoCapturer.captureDevices().first,
          let format = RTCCameraVideoCapturer.supportedFormats(for: device).filter({ CMVideoFormatDescriptionGetDimensions($0.formatDescription).width <= 1280 }).max(by: { CMVideoFormatDescriptionGetDimensions($0.formatDescription).width < CMVideoFormatDescriptionGetDimensions($1.formatDescription).width }) else { return }
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      camera.startCapture(with: device, format: format, fps: 30) { error in
        if let error { continuation.resume(throwing: error) } else { continuation.resume() }
      }
    }
    _ = try await current()
    cameraStarted = true
    cameraTrack?.isEnabled = intent.cameraLive(allowed: cameraAllowed, started: cameraStarted, live: live)
    if !intent.cameraEnabled { await stopCamera() }
  }

  func stop() async {
    guard live else { return }
    live = false; tail?.cancel(); recoveryDeadline?.cancel(); recoveryDeadline = nil
    microphone?.isEnabled = false; cameraTrack?.isEnabled = false
    RTCAudioSession.sharedInstance().isAudioEnabled = false
    peer?.delegate = nil; peer?.close(); peer = nil
    if let camera { await withCheckedContinuation { continuation in camera.stopCapture { continuation.resume() } } }
    self.camera = nil; cameraTrack = nil; microphone = nil; cameraStarted = false
    pendingICE.removeAll(); received.removeAll(); remoteVideo(nil)
    // A retired delegate cannot disable a newer call when deactivation arrives late.
  }
}

extension ZoenCallRTC {
  func generated(_ candidate: RTCIceCandidate, from connection: RTCPeerConnection) {
    guard live, peer === connection, candidate.sdpMLineIndex >= 0, candidate.sdpMLineIndex <= Int32(UInt16.max) else { return }
    let candidateGeneration = generation
    enqueue { rtc in
      guard candidateGeneration == rtc.generation else { return }
      _ = try await rtc.current()
      try await rtc.port.send(ZoenCallPayload(type: "ice", candidate: candidate.sdp, mid: candidate.sdpMid, line: UInt16(candidate.sdpMLineIndex)), rtc.generation)
    }
  }

  func connection(_ connection: RTCPeerConnection, changed state: RTCPeerConnectionState) {
    guard live, peer === connection else { return }
    enqueue { rtc in
      let call = try await rtc.current()
      switch state {
      case .connected:
        rtc.recoveryDeadline?.cancel(); rtc.recoveryDeadline = nil
        rtc.changed("connected")
      case .connecting, .disconnected: rtc.boundRecovery(); rtc.changed("reconnecting")
      case .failed:
        rtc.boundRecovery()
        guard call.callerDevice == rtc.port.scope.device else { rtc.changed("reconnecting"); return }
        guard rtc.generation < 7 else { throw ZoenCallFailure.exhausted }
        guard let peer = rtc.peer else { throw ZoenCallFailure.retired }
        try await rtc.renewTurn(peer, generation: rtc.generation)
        rtc.generation += 1
        peer.restartIce()
        try await rtc.offer()
      case .closed: throw ZoenCallFailure.retired
      default: break
      }
    }
  }

  func receivedVideo(_ track: RTCVideoTrack, from connection: RTCPeerConnection) {
    guard live, peer === connection else { return }
    enqueue { rtc in
      let call = try await rtc.current()
      guard call.video else { track.isEnabled = false; rtc.remoteVideo(nil); return }
      rtc.remoteVideo(track)
    }
  }
}
