import AVFoundation
import CryptoKit
import Foundation

// Standalone native harness. Only the unrelated Rust-backed media bridge is
// stubbed; the production editor, PCM cuts, encoder and decoder run unchanged.
enum ZoenMediaFiles { static let maximumBytes: UInt64 = 64 << 20 }

@main struct VoiceEditorAudioTests {
  static func expect(_ value: Bool) { precondition(value) }
  static func digest(_ uri: String) throws -> SHA256.Digest { SHA256.hash(data: try Data(contentsOf: URL(string: uri)!)) }
  static func samples(_ uri: String) throws -> [Float] {
    let file = try AVAudioFile(forReading: URL(string: uri)!, commonFormat: .pcmFormatFloat32, interleaved: false)
    let buffer = AVAudioPCMBuffer(pcmFormat: file.processingFormat, frameCapacity: AVAudioFrameCount(file.length))!
    try file.read(into: buffer)
    return Array(UnsafeBufferPointer(start: buffer.floatChannelData![0], count: Int(buffer.frameLength)))
  }
  static func tonePower(_ values: ArraySlice<Float>, frequency: Double, rate: Double) -> Double {
    var real = 0.0, imaginary = 0.0
    for (i, sample) in values.enumerated() {
      let angle = 2 * Double.pi * frequency * Double(i) / rate
      real += Double(sample) * cos(angle)
      imaginary += Double(sample) * sin(angle)
    }
    return (real * real + imaginary * imaginary) / pow(Double(values.count), 2)
  }
  static func refused(_ editor: ZoenVoiceEditor, sourceId: String, uri: String) async throws {
    do { _ = try await editor.admitSend(sourceId: sourceId, uri: uri); fatalError("stale/unowned render admitted") }
    catch ZoenVoiceEditor.Failure.closed { }
  }
  static func main() async throws {
    let root = URL.temporaryDirectory.appendingPathComponent("zoen-voice-audio-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let input = root.appendingPathComponent("synthetic.caf")
    let rate = 44_100.0
    let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 1)!
    let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 132_300)!
    buffer.frameLength = buffer.frameCapacity
    for frame in 0..<Int(buffer.frameLength) {
      let frequency = frame < 44_100 ? 440.0 : frame < 88_200 ? 1_200.0 : 880.0
      buffer.floatChannelData![0][frame] = Float(0.6 * sin(2 * Double.pi * frequency * Double(frame) / rate))
    }
    var file: AVAudioFile? = try AVAudioFile(forWriting: input, settings: format.settings)
    try file!.write(from: buffer); file = nil
    let originalHash = try digest(input.absoluteString)
    let owned = root.appendingPathComponent("leases", isDirectory: true)
    let editor = ZoenVoiceEditor(cacheRoot: owned)
    let source = try await editor.prepare(uri: input.absoluteString)
    precondition(source.sampleRate == rate && source.frameCount == 132_300 && source.levels.count == 160)
    // A finite Speech timestamp may still exceed Int64 after scaling. These
    // regressions execute the production conversion, not a JS reimplementation.
    let extreme = ZoenVoiceEditor.speechRange(startSeconds: -Double.greatestFiniteMagnitude,
      endSeconds: Double.greatestFiniteMagnitude, source: source, previousEnd: 0)
    precondition(extreme?.startFrame == 0 && extreme?.endFrame == source.frameCount)
    precondition(ZoenVoiceEditor.speechRange(startSeconds: .nan, endSeconds: 1, source: source, previousEnd: 0) == nil)
    precondition(ZoenVoiceEditor.speechRange(startSeconds: 0, endSeconds: .infinity, source: source, previousEnd: 0) == nil)
    precondition(ZoenVoiceEditor.speechRange(startSeconds: 2, endSeconds: 1, source: source, previousEnd: 0) == nil)
    precondition(ZoenVoiceEditor.speechRange(startSeconds: 1e100, endSeconds: 1e200, source: source, previousEnd: 0) == nil)
    let adjacent = ZoenVoiceEditor.speechRange(startSeconds: 0.99999999, endSeconds: 1.25, source: source, previousEnd: 44_100)
    precondition(adjacent?.startFrame == 44_100 && adjacent?.endFrame == 55_125)
    precondition(ZoenVoiceEditor.processingTimeout(durationSeconds: 3) == 60)
    precondition(ZoenVoiceEditor.processingTimeout(durationSeconds: 120) == 90)
    precondition(ZoenVoiceEditor.processingTimeout(durationSeconds: 3_600) == 900)
    expect(try digest(source.uri).elementsEqual(originalHash))
    let ranges = [ZoenVoiceEditor.Range(startFrame: 5_512, endFrame: 38_587), ZoenVoiceEditor.Range(startFrame: 93_712, endFrame: 126_787)]
    let output = try await editor.render(sourceId: source.id, ranges: ranges)
    let admitted = try await editor.admitSend(sourceId: source.id, uri: output.uri)
    precondition(admitted.frameCount == 66_150 && admitted.durationMs == 1_500 && admitted.mime == "audio/mp4")
    try await refused(editor, sourceId: source.id, uri: source.uri)
    try await refused(editor, sourceId: UUID().uuidString, uri: output.uri)
    let other = try await editor.prepare(uri: input.absoluteString)
    try await refused(editor, sourceId: other.id, uri: output.uri)
    let replacement = try await editor.render(sourceId: source.id, ranges: ranges)
    try await refused(editor, sourceId: source.id, uri: output.uri)
    _ = try await editor.admitSend(sourceId: source.id, uri: replacement.uri)
    let decoded = try samples(output.uri)
    precondition(output.frameCount == 66_150 && decoded.count == 66_150)
    precondition(abs(output.durationMs - 1_500) < 0.001)
    expect(try digest(source.uri).elementsEqual(originalHash))
    expect(try digest(input.absoluteString).elementsEqual(originalHash))
    let first = decoded[4_096..<12_288], second = decoded[40_000..<48_192]
    precondition(tonePower(first, frequency: 440, rate: rate) > 100 * tonePower(first, frequency: 1_200, rate: rate))
    precondition(tonePower(second, frequency: 880, rate: rate) > 100 * tonePower(second, frequency: 1_200, rate: rate))
    let edge = decoded.prefix(64).reduce(0.0) { $0 + Double(abs($1)) } / 64
    let body = first.reduce(0.0) { $0 + Double(abs($1)) } / Double(first.count)
    precondition(edge < body * 0.2, "15ms fade must remain inside kept frames")
    precondition(decoded.allSatisfy { $0.isFinite && abs($0) <= 1 })
    for bad in [[], [.init(startFrame: -1, endFrame: 20_000)], [.init(startFrame: 0, endFrame: 100)],
                [.init(startFrame: 0, endFrame: 50_000), .init(startFrame: 40_000, endFrame: 80_000)],
                [.init(startFrame: 50_000, endFrame: 70_000), .init(startFrame: 0, endFrame: 20_000)],
                [.init(startFrame: 0, endFrame: 132_301)]] as [[ZoenVoiceEditor.Range]] {
      do { _ = try await editor.render(sourceId: source.id, ranges: bad); fatalError("malformed edit admitted") }
      catch ZoenVoiceEditor.Failure.invalidRanges { }
    }
    try await refused(editor, sourceId: source.id, uri: replacement.uri)
    let beforeCancel = try await editor.render(sourceId: source.id, ranges: ranges)
    await editor.cancel(source.id)
    try await refused(editor, sourceId: source.id, uri: beforeCancel.uri)
    let beforeRelease = try await editor.render(sourceId: source.id, ranges: ranges)
    try await editor.releaseOutput(sourceId: source.id, uri: beforeRelease.uri)
    try await refused(editor, sourceId: source.id, uri: beforeRelease.uri)
    // A cleanup failure leaves readable files behind, but never a send capability.
    let blocked = try await editor.render(sourceId: other.id, ranges: ranges)
    let blockedRoot = URL(string: other.uri)!.deletingLastPathComponent()
    try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: blockedRoot.path)
    var cleanupFailed = false
    do { try await editor.release(other.id) } catch { cleanupFailed = true }
    precondition(cleanupFailed && FileManager.default.fileExists(atPath: URL(string: blocked.uri)!.path))
    try await refused(editor, sourceId: other.id, uri: blocked.uri)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: blockedRoot.path)
    try await editor.release(other.id)
    try await editor.releaseOutput(sourceId: source.id, uri: source.uri)
    precondition(FileManager.default.fileExists(atPath: URL(string: source.uri)!.path), "releaseRender must never erase original")
    try await editor.releaseOutput(sourceId: source.id, uri: input.absoluteString)
    precondition(FileManager.default.fileExists(atPath: input.path), "releaseRender must never erase an unowned file")
    try await editor.releaseOutput(sourceId: source.id, uri: output.uri)
    precondition(!FileManager.default.fileExists(atPath: URL(string: output.uri)!.path))
    let cancelled = Task { try await editor.render(sourceId: source.id, ranges: ranges) }
    cancelled.cancel()
    do { _ = try await cancelled.value; fatalError("cancelled export returned") } catch is CancellationError { }
    try await editor.release(source.id)
    try await editor.release(source.id)
    try await refused(editor, sourceId: source.id, uri: replacement.uri)
    precondition(!FileManager.default.fileExists(atPath: URL(string: source.uri)!.deletingLastPathComponent().path))
    expect(try digest(input.absoluteString).elementsEqual(originalHash))
    print("Native voice PCM→AAC PASS: 66150 decoded frames,1500ms; disjoint440/880Hz kept,1200Hz excluded;15ms fades; immutable source; invalid edits refused; cancellation and owned cleanup; extreme/invalid Speech times and duration-aware processing budgets; only current completed render admitted, stale/foreign/cancelled/released/failed-cleanup renders refused.")
  }
}
