import ExpoModulesCore
import Foundation
import UIKit
import PDFKit
#if canImport(FoundationModels)
import FoundationModels
#endif

extension ZoenSession {
  private struct FileSend: Decodable {
    let spaceId: String
    let uri: String
    let name: String
    let mime: String
    let caption: String?
    let replyTo: String?
    let thread: Bool
    let mutationId: String
  }
  func sendFile(_ request: String) throws -> String {
    guard !erasing, !closing, let core else { throw SessionError.closed }
    guard request.utf8.count <= 131_072 else { throw ZoenMediaFiles.Failure.invalidFile }
    let save = try JSONDecoder().decode(FileSend.self, from: Data(request.utf8))
    guard !save.spaceId.isEmpty, save.spaceId.utf8.count <= 256,
          !save.spaceId.contains("\0"), save.uri.utf8.count <= 16_384,
          !save.mutationId.isEmpty, save.mutationId.utf8.count <= 256, !save.mutationId.contains("\0"),
          (save.caption?.utf8.count ?? 0) <= 16_384,
          save.caption.map({ !$0.contains("\0") }) ?? true,
          save.replyTo.map({ !$0.isEmpty && $0.utf8.count <= 256 && !$0.contains("\0") }) ?? true
          else { throw ZoenMediaFiles.Failure.invalidFile }
    let selection = try mediaFiles.readSelection(uri: save.uri, name: save.name, mime: save.mime)
    let thumbnail = try? ZoenMediaFiles.thumbnail(at: selection.url)
    // This actor method has no suspension: close/erase cannot cross the native
    // selection validation and synchronous canonical file-message mutation.
    let entry = try core.sendFileMessageWithCaption(spaceId: save.spaceId, name: selection.name,
      mime: selection.mime, bytes: selection.data, thumbnail: thumbnail,
      replyTo: save.replyTo, thread: save.thread, mutationId: save.mutationId,
      caption: save.caption ?? "")
    var result: [String: Any] = ["messageId": entry.id]
    if case let .message(_, card) = entry.kind, let card { result["itemId"] = card.itemId }
    return try ZoenMediaFiles.json(result)
  }

  private struct VoiceSend: Decodable {
    let spaceId: String
    let uri: String
    let sourceId: String?
    let replyTo: String?
    let thread: Bool
    let transcript: String
  }
  func sendVoice(_ request: String) async throws -> String {
    guard !erasing, !closing, let core else { throw SessionError.closed }
    guard request.utf8.count <= 131_072 else { throw ZoenMediaFiles.Failure.invalidRecording }
    let save = try JSONDecoder().decode(VoiceSend.self, from: Data(request.utf8))
    guard let sourceId = save.sourceId, !sourceId.isEmpty, voiceSources[sourceId] == generation,
          voiceRenders[sourceId] == save.uri,
          save.transcript.utf8.count <= 65_536 else { throw ZoenMediaFiles.Failure.invalidRecording }
    let expected = generation
    let render = try await voiceEditor.admitSend(sourceId: sourceId, uri: save.uri)
    // The awaited actor boundary may have allowed lock/close/open/erase. The
    // actual Rust mutation below stays synchronous after this final fence.
    guard !erasing, !closing, generation == expected, self.core === core,
          voiceSources[sourceId] == expected, voiceRenders[sourceId] == save.uri else { throw SessionError.closed }
    let selection = try mediaFiles.readSelection(uri: render.uri, name: "voice.m4a", mime: render.mime)
    let entry = try core.sendVoiceNote(spaceId: save.spaceId, bytes: selection.data, mime: render.mime,
      ms: UInt64(render.durationMs.rounded()), levels: render.levels.map(Float.init), transcript: save.transcript, replyTo: save.replyTo, thread: save.thread)
    // Consume only after acceptance. A throwing mutation can retry its preview;
    // another queued send cannot reuse this accepted render when it resumes.
    voiceRenders.removeValue(forKey: sourceId)
    voiceRenderAttempts.removeValue(forKey: sourceId)
    guard let voice = entry.voice else { throw ZoenMediaFiles.Failure.invalidRecording }
    return try ZoenMediaFiles.json(["messageId": entry.id, "itemId": voice.itemId])
  }

  private func voiceJSON<T: Encodable>(_ value: T) throws -> String {
    String(decoding: try JSONEncoder().encode(value), as: UTF8.self)
  }
  func prepareVoice(_ uri: String) async throws -> String {
    guard !erasing, !closing, core != nil else { throw SessionError.closed }
    let expected = generation
    let source = try await voiceEditor.prepare(uri: uri)
    guard !erasing, !closing, generation == expected, core != nil else {
      try await voiceEditor.release(source.id)
      throw SessionError.closed
    }
    voiceSources[source.id] = expected
    return try voiceJSON(source)
  }
  func renderVoice(_ sourceId: String, ranges: String) async throws -> String {
    guard !erasing, !closing, core != nil, voiceSources[sourceId] == generation, ranges.utf8.count <= 262_144 else { throw SessionError.closed }
    let expected = generation
    let attempt = UUID()
    voiceRenderAttempts[sourceId] = attempt
    voiceRenders.removeValue(forKey: sourceId)
    let ranges = try JSONDecoder().decode([ZoenVoiceEditor.Range].self, from: Data(ranges.utf8))
    let result = try await voiceEditor.render(sourceId: sourceId, ranges: ranges)
    guard !erasing, !closing, generation == expected, core != nil, voiceSources[sourceId] == expected,
          voiceRenderAttempts[sourceId] == attempt else {
      try await voiceEditor.releaseOutput(sourceId: sourceId, uri: result.uri)
      throw SessionError.closed
    }
    voiceRenders[sourceId] = result.uri
    return try voiceJSON(result)
  }
  func transcribeVoice(_ sourceId: String, locale: String) async throws -> String {
    guard !erasing, !closing, core != nil, voiceSources[sourceId] == generation, ["en", "pt-BR"].contains(locale) else { throw SessionError.closed }
    let expected = generation
    let result = try await voiceEditor.transcribe(sourceId: sourceId, locale: locale)
    guard !erasing, !closing, generation == expected, core != nil else { throw SessionError.closed }
    return try voiceJSON(result)
  }
  func releaseVoiceRender(_ sourceId: String, uri: String) async throws {
    if voiceRenders[sourceId] == uri { voiceRenders.removeValue(forKey: sourceId) }
    try await voiceEditor.releaseOutput(sourceId: sourceId, uri: uri)
  }
  func cancelVoice(_ sourceId: String) async {
    voiceRenders.removeValue(forKey: sourceId)
    voiceRenderAttempts.removeValue(forKey: sourceId)
    await voiceEditor.cancel(sourceId)
  }
  func releaseVoice(_ sourceId: String) async throws {
    voiceSources.removeValue(forKey: sourceId)
    voiceRenders.removeValue(forKey: sourceId)
    voiceRenderAttempts.removeValue(forKey: sourceId)
    try await voiceEditor.release(sourceId)
  }

}
