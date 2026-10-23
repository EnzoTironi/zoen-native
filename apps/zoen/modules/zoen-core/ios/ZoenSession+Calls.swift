import Foundation
import AVFoundation

extension ZoenSession {
  struct CallBorrow: Sendable {
    let engine: Client
    let generation: UInt64
    let identity: String
    let device: String
    let lease: ZoenModuleSessionLease
  }

  func bindCalls(lease: ZoenModuleSessionLease, changed: @escaping @Sendable (String) -> Void, audio: @escaping @Sendable (String) -> Void) async {
    guard lease.isLive, !closing, !erasing, mode == "account" else { return }
    if let borrow = callsBorrow, borrow.lease === lease, callBorrowIsCurrent(borrow) {
      callsChanged = changed; callsAudioChanged = audio
      return
    }
    if callsBorrow != nil { await stopCallsForClose() }
    callsChanged = changed; callsAudioChanged = audio; callsModuleLease = lease
    guard let core, let account = core.account(), account.unlocked, !account.setupPending else { return }
    callsBorrow = CallBorrow(engine: core, generation: generation, identity: account.identityId, device: account.deviceId, lease: lease)
    // Rebinding never resurrects media. The existing signed log retires this
    // device's abandoned resources before new invites are admitted.
    for space in core.spaces() {
      guard let borrow = callsBorrow, callBorrowIsCurrent(borrow) else { return }
      try? await retireAbandonedCalls(borrow, space: space.id)
    }
    await callCanonicalChanges(core.spaces().map(\.id))
  }

  private func retireAbandonedCalls(_ borrow: CallBorrow, space: String) async throws {
    guard callCoordinator == nil, callsRecovering.insert(space).inserted else { return }
    defer { callsRecovering.remove(space) }
    for call in try callRecords(borrow, space: space) where !call.terminal {
      if call.acceptedDevice != nil && (call.callerDevice == borrow.device || call.acceptedDevice == borrow.device) {
        try await callAction(borrow, space: space, call: call, action: "hang_up")
      } else if call.acceptedDevice == nil && call.callerDevice == borrow.device {
        try await callAction(borrow, space: space, call: call, action: "cancel")
      }
    }
  }

  private func callBorrowIsCurrent(_ borrow: CallBorrow) -> Bool {
    guard borrow.lease.isLive, !closing, !erasing, core === borrow.engine, generation == borrow.generation,
          mode == "account", let account = borrow.engine.account(), account.unlocked, !account.setupPending else { return false }
    return account.identityId == borrow.identity && account.deviceId == borrow.device
  }
  private func callRecords(_ borrow: CallBorrow, space: String) throws -> [ZoenCallRecord] {
    guard callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    return try zoenCallDecode([ZoenCallRecord].self, borrow.engine.callNativeState(identityId: borrow.identity, deviceId: borrow.device, spaceId: space))
  }
  private func currentCall(_ borrow: CallBorrow, scope: ZoenCallScope) throws -> ZoenCallRecord {
    guard let call = try callRecords(borrow, space: scope.space).first(where: { $0.callId == scope.call && $0.invitation == scope.invitation }) else { throw SessionError.closed }
    let peer = call.caller == borrow.identity ? call.callee : call.caller
    guard try !borrow.engine.blockedPeople().contains(peer) else { throw SessionError.closed }
    return call
  }

  func hasNotificationCallCandidate() -> Bool {
    guard notificationOnly, !closing, !erasing, let core, let account = core.account(),
          account.unlocked, let blocked = try? core.blockedPeople() else { return false }
    let now = Int64(Date().timeIntervalSince1970 * 1000)
    return core.spaces().contains { space in
      guard let calls = try? zoenCallDecode([ZoenCallRecord].self,
        core.callNativeState(identityId: account.identityId, deviceId: account.deviceId, spaceId: space.id)) else { return false }
      return calls.contains { $0.callee == account.identityId && $0.invitationConfirmed && $0.ringing(at: now) && !blocked.contains($0.caller) }
    }
  }
  private func callAction(_ borrow: CallBorrow, space: String, call: ZoenCallRecord, action: String) async throws {
    guard callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    if action == "accept" {
      let peer = call.caller == borrow.identity ? call.callee : call.caller
      guard try !borrow.engine.blockedPeople().contains(peer) else { throw SessionError.closed }
    }
    let command: [String: Any] = ["op": "respond_call", "expected_identity_id": borrow.identity, "expected_device_id": borrow.device,
      "space_id": space, "call_id": call.callId, "invitation": call.invitation, "action": action]
    let json = String(decoding: try JSONSerialization.data(withJSONObject: command), as: UTF8.self)
    requests += 1; defer { retireRequest() }
    _ = try await borrow.engine.clientRequest(request: json)
    guard callBorrowIsCurrent(borrow) else { throw SessionError.closed }
  }

  private func callPort(_ borrow: CallBorrow, space: String, call: ZoenCallRecord) -> ZoenCallPort {
    let scope = ZoenCallScope(identity: borrow.identity, device: borrow.device, space: space, call: call.callId, invitation: call.invitation)
    let audioFocus = callAudioFocus
    return ZoenCallPort(scope: scope, locale: sessionLocale, relayReady: { [weak self] in
      guard let self else { return false }; return await self.callRelayReady(borrow)
    }, current: { [weak self] in
      guard let self else { throw SessionError.closed }; return try await self.currentCall(borrow, scope: scope)
    }, transport: { [weak self] in
      guard let self else { throw SessionError.closed }; return try await self.readCallTransport(borrow, scope: scope)
    }, send: { [weak self] payload, generation in
      guard let self else { throw SessionError.closed }; try await self.sendCallTransport(borrow, scope: scope, payload: payload, generation: generation)
    }, prepareAudio: { [weak self] in
      guard let self else { throw SessionError.closed }; try await self.prepareCallAudio(borrow)
    }, releaseAudio: { await audioFocus.release() }, turn: { [weak self] in
      guard let self else { throw SessionError.closed }; return try await self.callTurn(borrow, scope: scope)
    }, action: { [weak self] action in
      guard let self else { throw SessionError.closed }; try await self.callAction(borrow, space: space, call: call, action: action)
    })
  }

  private func callRelayReady(_ borrow: CallBorrow) -> Bool {
    guard callBorrowIsCurrent(borrow) else { return false }
    let connection = borrow.engine.connection()
    return connection.state == "online" && connection.synced
  }

  private func prepareCallAudio(_ borrow: CallBorrow) async throws {
    guard callBorrowIsCurrent(borrow), let emit = callsAudioChanged else { throw SessionError.closed }
    if notificationOnly { try await callAudioFocus.acquireNative() }
    else { try await callAudioFocus.acquire(emit: emit) }
    guard callBorrowIsCurrent(borrow) else { await callAudioFocus.release(); throw SessionError.closed }
  }
  func callAudioReply(ticket: String, success: Bool) async { await callAudioFocus.answer(ticket: ticket, success: success) }

  private func readCallTransport(_ borrow: CallBorrow, scope: ZoenCallScope) throws -> [ZoenCallSignalRecord] {
    _ = try currentCall(borrow, scope: scope)
    return try zoenCallDecode([ZoenCallSignalRecord].self, borrow.engine.callTransportRead(identityId: borrow.identity, deviceId: borrow.device, spaceId: scope.space, callId: scope.call))
  }
  private func sendCallTransport(_ borrow: CallBorrow, scope: ZoenCallScope, payload: ZoenCallPayload, generation: UInt8) throws {
    let call = try currentCall(borrow, scope: scope)
    guard !call.terminal, let accepted = call.acceptedDevice else { throw SessionError.closed }
    let signal = ZoenCallSignalRecord.Signal(signalId: "tx_" + UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased(), callId: scope.call,
      invitation: scope.invitation, recipientDevice: borrow.device == call.callerDevice ? accepted : call.callerDevice, generation: generation, payload: payload)
    let encoder = JSONEncoder(); encoder.keyEncodingStrategy = .convertToSnakeCase
    let json = String(decoding: try encoder.encode(signal), as: UTF8.self)
    try borrow.engine.callTransportSend(identityId: borrow.identity, deviceId: borrow.device, spaceId: scope.space, signalJson: json)
  }
  private func callTurn(_ borrow: CallBorrow, scope: ZoenCallScope) async throws -> ZoenCallTurn {
    let call = try currentCall(borrow, scope: scope)
    requests += 1; defer { retireRequest() }
    let turn = try await borrow.engine.callTurn(expectedIdentityId: borrow.identity, expectedDeviceId: borrow.device, spaceId: scope.space,
      peerIdentityId: call.caller == borrow.identity ? call.callee : call.caller)
    _ = try currentCall(borrow, scope: scope)
    return ZoenCallTurn(urls: turn.urls, username: turn.username, credential: turn.credential, expiresAtMs: turn.expiresAtMs)
  }

  func startCall(space: String, video: Bool) async throws {
    if callsBorrow == nil, let lease = callsModuleLease, let changed = callsChanged, let audio = callsAudioChanged {
      await bindCalls(lease: lease, changed: changed, audio: audio)
    }
    guard !callStarting, callCoordinator == nil, let borrow = callsBorrow, callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    callStarting = true; defer { callStarting = false }
    try await retireAbandonedCalls(borrow, space: space)
    guard let chat = borrow.engine.spaces().first(where: { $0.id == space }),
          let peer = chat.members.first(where: { $0.id != borrow.identity }) else { throw SessionError.closed }
    guard try !borrow.engine.blockedPeople().contains(peer.id) else { throw SessionError.closed }
    requests += 1; defer { retireRequest() }
    // Refuse absent/invalid TURN configuration before ringing another person.
    _ = try await borrow.engine.callTurn(expectedIdentityId: borrow.identity, expectedDeviceId: borrow.device, spaceId: space, peerIdentityId: peer.id)
    guard callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    guard await AVAudioApplication.requestRecordPermission() else { throw ZoenCallFailure.denied }
    guard callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    if video {
      guard await AVCaptureDevice.requestAccess(for: .video) else { throw ZoenCallFailure.denied }
      guard callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    }
    guard callCoordinator == nil, !callPresenting else { throw ZoenCallFailure.unavailable }
    let command: [String: Any] = ["op": "invite_call", "expected_identity_id": borrow.identity, "expected_device_id": borrow.device, "space_id": space, "video": video]
    let json = String(decoding: try JSONSerialization.data(withJSONObject: command), as: UTF8.self)
    let result = try await borrow.engine.clientRequest(request: json)
    guard callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    let call = try zoenCallDecode(ZoenCallRecord.self, result)
    do { try await presentCall(borrow, space: space, call: call, outgoing: true) }
    catch { try? await callAction(borrow, space: space, call: call, action: "cancel"); throw error }
  }

  private func presentCall(_ borrow: CallBorrow, space: String, call: ZoenCallRecord, outgoing: Bool) async throws {
    guard callCoordinator == nil, !callPresenting, callBorrowIsCurrent(borrow) else { throw SessionError.closed }
    let peerID = call.caller == borrow.identity ? call.callee : call.caller
    guard try !borrow.engine.blockedPeople().contains(peerID) else { throw SessionError.closed }
    callPresenting = true; defer { callPresenting = false }
    let name = (try? borrow.engine.getProfile(identityId: peerID).name) ?? "Call"
    let port = callPort(borrow, space: space, call: call)
    let coordinator = await MainActor.run { ZoenCallCoordinator(port: port, record: call, name: name) { [weak self] state in
      Task { await self?.callPresentationChanged(scope: port.scope, state: state) }
    }}
    guard callBorrowIsCurrent(borrow) else { await coordinator.stop(writeEnd: false); throw SessionError.closed }
    callCoordinator = coordinator; callsScope = port.scope
    do { try await coordinator.begin(outgoing: outgoing) }
    catch { await coordinator.stop(writeEnd: true, reason: .failed); throw error }
  }

  private func callPresentationChanged(scope: ZoenCallScope, state: String) async {
    guard callsScope == scope, let coordinator = callCoordinator else { return }
    let snapshot = await coordinator.presentationSnapshot()
    guard callsScope == scope, callCoordinator === coordinator else { return }
    callsChanged?(snapshot.json())
    if snapshot.state == "ended" {
      callCoordinator = nil; callsScope = nil
      if notificationOnly, let lease = callsModuleLease {
        Task { await ZoenNativePushSessions.gate.releaseRetained(session: self, lease: lease) }
      }
    }
  }

  func currentCallPresentation() async -> String {
    guard let borrow = callsBorrow, callBorrowIsCurrent(borrow), let scope = callsScope,
          let coordinator = callCoordinator, (try? currentCall(borrow, scope: scope)) != nil else { return "null" }
    let value = await coordinator.presentationSnapshot()
    guard callsScope == scope, callCoordinator === coordinator, callBorrowIsCurrent(borrow) else { return "null" }
    return value.json()
  }

  func showCurrentCall(identity: String, device: String, call: String) async throws {
    guard let borrow = callsBorrow, callBorrowIsCurrent(borrow), borrow.identity == identity, borrow.device == device,
          let scope = callsScope, scope.call == call, let coordinator = callCoordinator else { throw SessionError.closed }
    do { _ = try currentCall(borrow, scope: scope); try await coordinator.show() }
    catch { await coordinator.stop(writeEnd: false, reason: .failed); throw error }
  }

  func callCanonicalChanges(_ spaces: [String]) async {
    if callsBorrow == nil, let lease = callsModuleLease, let changed = callsChanged, let audio = callsAudioChanged {
      await bindCalls(lease: lease, changed: changed, audio: audio)
      return
    }
    guard let borrow = callsBorrow, callBorrowIsCurrent(borrow) else { return }
    if let coordinator = callCoordinator {
      do { try await coordinator.refresh() } catch { await coordinator.stop(writeEnd: false, reason: .failed) }
      return
    }
    let now = Int64(Date().timeIntervalSince1970 * 1000)
    guard let blocked = try? borrow.engine.blockedPeople() else { return }
    for space in spaces {
      if !callStarting && !callPresenting { try? await retireAbandonedCalls(borrow, space: space) }
      guard callBorrowIsCurrent(borrow), let calls = try? callRecords(borrow, space: space) else { continue }
      if let incoming = calls.last(where: { $0.callee == borrow.identity && $0.invitationConfirmed && $0.ringing(at: now) && !blocked.contains($0.caller) }) {
        try? await presentCall(borrow, space: space, call: incoming, outgoing: false)
        return
      }
    }
  }

  func stopCallsForClose() async {
    let coordinator = callCoordinator
    callCoordinator = nil
    let scope = callsScope; callsScope = nil
    let borrow = callsBorrow; callsBorrow = nil
    await coordinator?.stop(writeEnd: false)
    if let borrow, let scope, core === borrow.engine,
       let records = try? zoenCallDecode([ZoenCallRecord].self, borrow.engine.callNativeState(identityId: borrow.identity, deviceId: borrow.device, spaceId: scope.space)),
       let call = records.first(where: { $0.callId == scope.call }), !call.terminal {
      let action = call.acceptedDevice == nil ? (call.caller == borrow.identity ? "cancel" : "decline") : "hang_up"
      let command: [String: String] = ["op": "respond_call", "expected_identity_id": borrow.identity, "expected_device_id": borrow.device,
        "space_id": scope.space, "call_id": call.callId, "invitation": call.invitation, "action": action]
      if let data = try? JSONSerialization.data(withJSONObject: command) { _ = try? await borrow.engine.clientRequest(request: String(decoding: data, as: UTF8.self)) }
    }
    // The coordinator releases media ownership after the system deactivation.
    callsChanged = nil; callsAudioChanged = nil; callsModuleLease = nil
  }
}
