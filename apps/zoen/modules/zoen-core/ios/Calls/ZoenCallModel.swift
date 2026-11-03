import Foundation

struct ZoenCallRecord: Decodable, Sendable {
  let callId: String
  let invitation: String
  let caller: String
  let callerDevice: String
  let callee: String
  let media: String
  let invitedAtMs: Int64
  let expiresAtMs: Int64
  let invitationConfirmed: Bool
  let acceptedDevice: String?
  let acceptedAtMs: Int64?
  let acceptanceConfirmed: Bool
  let endedAtMs: Int64?
  let endReason: String?
  let invalidated: Bool
  var terminal: Bool { invalidated || endReason != nil }
  var video: Bool { media == "video" }
  func ringing(at now: Int64) -> Bool { !terminal && acceptedDevice == nil && now >= invitedAtMs && now < expiresAtMs }
}

struct ZoenCallScope: Equatable, Sendable {
  let identity: String
  let device: String
  let space: String
  let call: String
  let invitation: String
}

struct ZoenCallTurn: Decodable, Sendable {
  let urls: [String]
  let username: String
  let credential: String
  let expiresAtMs: Int64
}

struct ZoenCallSignalRecord: Decodable, Sendable {
  struct Signal: Codable, Sendable {
    let signalId: String
    let callId: String
    let invitation: String
    let recipientDevice: String
    let generation: UInt8
    let payload: ZoenCallPayload
  }
  let senderDevice: String
  let signal: Signal
}

struct ZoenCallPayload: Codable, Sendable {
  let type: String
  var sdp: String?
  var candidate: String?
  var mid: String?
  var line: UInt16?
}

struct ZoenCallPort: Sendable {
  let scope: ZoenCallScope
  let locale: String
  let relayReady: @Sendable () async -> Bool
  let current: @Sendable () async throws -> ZoenCallRecord
  let transport: @Sendable () async throws -> [ZoenCallSignalRecord]
  let send: @Sendable (ZoenCallPayload, UInt8) async throws -> Void
  let prepareAudio: @Sendable () async throws -> Void
  let releaseAudio: @Sendable () async -> Void
  let turn: @Sendable () async throws -> ZoenCallTurn
  let action: @Sendable (String) async throws -> Void

  func mediaRecord() async throws -> ZoenCallRecord {
    let call = try await current()
    guard !call.terminal, call.callId == scope.call, call.invitation == scope.invitation,
          call.invitationConfirmed, call.acceptanceConfirmed,
          (call.callerDevice == scope.device || call.acceptedDevice == scope.device) else { throw ZoenCallFailure.retired }
    return call
  }

  /// The caller and callee renew through the same scoped authority. The native
  /// owner/generation fence must survive both the issuer and canonical reads.
  @MainActor
  func renewedTurn(isCurrent: () -> Bool) async throws -> ZoenCallTurn {
    guard isCurrent() else { throw ZoenCallFailure.retired }
    _ = try await mediaRecord()
    guard isCurrent() else { throw ZoenCallFailure.retired }
    let result = try await turn()
    _ = try await mediaRecord()
    guard isCurrent() else { throw ZoenCallFailure.retired }
    let now = Int64(Date().timeIntervalSince1970 * 1000)
    guard result.expiresAtMs > now, !result.username.isEmpty, !result.credential.isEmpty,
          !result.urls.isEmpty, result.urls.allSatisfy({ $0.hasPrefix("turns:") }) else { throw ZoenCallFailure.unavailable }
    return result
  }

  /// Re-read policy/canonical state after crossing to system presentation. The
  /// snapshot used to construct a coordinator cannot authorize a later report.
  func presentationRecord() async throws -> ZoenCallRecord {
    let record = try await current()
    let caller = record.caller == scope.identity && record.callerDevice == scope.device
    let callee = record.callee == scope.identity && record.invitationConfirmed
      && (record.acceptedDevice == nil || record.acceptedDevice == scope.device)
    guard !record.terminal, record.callId == scope.call, record.invitation == scope.invitation,
          caller || callee else { throw ZoenCallFailure.retired }
    return record
  }
}

enum ZoenCallFailure: Error { case retired, denied, unavailable, negotiation, exhausted }

func zoenCallDecode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
  let decoder = JSONDecoder()
  decoder.keyDecodingStrategy = .convertFromSnakeCase
  return try decoder.decode(type, from: Data(json.utf8))
}
