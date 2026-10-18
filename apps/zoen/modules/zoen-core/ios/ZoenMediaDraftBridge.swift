import Foundation

/// The JSON bridge carries only provenance and bounded metadata. Rust alone stores draft bytes.
enum ZoenMediaDraftBridge {
  struct Scope: Decodable, Equatable {
    let expectedIdentityId: String
    let expectedDeviceId: String
    let spaceId: String
    let threadRootId: String?
    enum CodingKeys: String, CodingKey {
      case expectedIdentityId = "expected_identity_id", expectedDeviceId = "expected_device_id"
      case spaceId = "space_id", threadRootId = "thread_root_id"
    }
    var core: MediaDraftScope {
      MediaDraftScope(expectedIdentityId: expectedIdentityId, expectedDeviceId: expectedDeviceId,
        spaceId: spaceId, threadRootId: threadRootId)
    }
  }
  struct Asset: Decodable {
    let id: String
    let uri: String
    let name: String
    let mime: String
    let voiceSourceId: String?
    let voiceRole: String?
    enum CodingKeys: String, CodingKey {
      case id, uri, name, mime
      case voiceSourceId = "voice_source_id", voiceRole = "voice_role"
    }
  }
  struct Request: Decodable {
    let scope: Scope
    let expectedRevision: String?
    let contentJson: String?
    let assets: [Asset]?
    let assetId: String?
    enum CodingKeys: String, CodingKey {
      case scope, assets
      case expectedRevision = "expected_revision", contentJson = "content_json", assetId = "asset_id"
    }
  }
  static func decode(_ json: String) throws -> Request {
    guard json.utf8.count <= 1 << 20 else { throw ZoenMediaFiles.Failure.tooLarge }
    return try JSONDecoder().decode(Request.self, from: Data(json.utf8))
  }
  static func object(_ draft: MediaDraftDto) -> [String: Any] {
    ["revision": draft.revision, "content_json": draft.contentJson,
     "assets": draft.assets.map(object), "attempted": draft.attempted]
  }
  static func object(_ asset: MediaDraftAsset) -> [String: Any] {
    ["id": asset.id, "name": asset.name, "mime": asset.mime,
     "size": asset.size, "sha256": asset.sha256]
  }
  static func content(_ json: String) throws -> [String: Any]? {
    guard json.utf8.count <= 512 << 10 else { throw ZoenMediaFiles.Failure.tooLarge }
    let value = try JSONSerialization.jsonObject(with: Data(json.utf8), options: [.fragmentsAllowed])
    if value is NSNull { return nil }
    guard let object = value as? [String: Any] else { throw ZoenMediaFiles.Failure.invalidFile }
    return object
  }
  static func sourceMetadata(_ content: [String: Any], source: ZoenVoiceEditor.Source) throws {
    guard let editor = content["editor"] as? [String: Any],
          let draft = editor["draft"] as? [String: Any],
          let original = draft["original"] as? [String: Any],
          (original["sampleRate"] as? NSNumber)?.doubleValue == source.sampleRate,
          (original["frameCount"] as? NSNumber)?.doubleValue == Double(source.frameCount)
          else { throw ZoenMediaFiles.Failure.invalidRecording }
  }
  static func renderMetadata(_ value: [String: Any], render: ZoenVoiceEditor.Render) throws {
    guard value["mime"] as? String == render.mime,
          let duration = value["duration_ms"] as? NSNumber,
          duration.doubleValue.isFinite, abs(duration.doubleValue - render.durationMs) < 0.001,
          let received = levels(value["levels"]), let actual = levels(render.levels),
          received.count == actual.count,
          zip(received, actual).allSatisfy({ Float($0.0) == Float($0.1) })
          else { throw ZoenMediaFiles.Failure.invalidRecording }
  }
  private static func levels(_ value: Any?) -> [Double]? {
    guard let levels = value as? [Double], levels.count <= 160,
          levels.allSatisfy({ $0.isFinite && (0...1).contains($0) }) else { return nil }
    return levels
  }
  static func sameOriginal(_ left: Any?, _ right: Any?) -> Bool {
    guard let a = left as? [String: Any], let b = right as? [String: Any],
          let id = a["id"] as? String, id == b["id"] as? String,
          let rate = a["sampleRate"] as? NSNumber, rate.doubleValue.isFinite,
          rate.doubleValue > 0, rate.doubleValue == (b["sampleRate"] as? NSNumber)?.doubleValue,
          let frames = a["frameCount"] as? NSNumber, frames.doubleValue.isFinite,
          frames.doubleValue > 0, frames.doubleValue == (b["frameCount"] as? NSNumber)?.doubleValue,
          levels(a["levels"]) != nil, levels(b["levels"]) != nil else { return false }
    // The immutable asset authenticates the audio. Display peaks can be
    // canonically re-encoded by Rust without changing its identity or frames.
    return true
  }
  static func sameRender(_ left: [String: Any], _ right: [String: Any]) -> Bool {
    guard let id = left["asset_id"] as? String, id == right["asset_id"] as? String,
          let mime = left["mime"] as? String, mime == right["mime"] as? String,
          let transcript = left["transcript"] as? String,
          transcript == right["transcript"] as? String,
          let a = left["duration_ms"] as? NSNumber, let b = right["duration_ms"] as? NSNumber,
          a.doubleValue.isFinite, b.doubleValue.isFinite,
          abs(a.doubleValue - b.doubleValue) < 0.001,
          let aLevels = levels(left["levels"]), let bLevels = levels(right["levels"]),
          aLevels.count == bLevels.count else { return false }
    // VoiceRender stores published waveform peaks as f32. Compare that actual
    // representation rather than the spelling of a JS Double's JSON decimal.
    return zip(aLevels, bLevels).allSatisfy { Float($0.0) == Float($0.1) }
  }
}
