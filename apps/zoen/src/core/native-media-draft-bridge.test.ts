import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

// Only unrelated generated FFI records are stubbed. The real native admission
// helper, Foundation JSONEncoder/JSONSerialization and JS JSON codec all run.
const harness = String.raw`
import Foundation

struct MediaDraftScope { let expectedIdentityId: String; let expectedDeviceId: String; let spaceId: String; let threadRootId: String? }
struct MediaDraftAsset { let id: String; let name: String; let mime: String; let size: UInt64; let sha256: String }
struct MediaDraftDto { let revision: String; let contentJson: String; let assets: [MediaDraftAsset]; let attempted: Bool; var acceptedIntentJson: String? = nil }
enum ZoenMediaFiles { enum Failure: Error { case tooLarge, invalidFile, invalidRecording } }
enum ZoenVoiceEditor {
  struct Source { let sampleRate: Double; let frameCount: Int64 }
  struct Render: Codable {
    let uri: String; let mime: String; let frameCount: Int64
    let durationMs: Double; let levels: [Double]
  }
}

@main struct NativeMediaDraftBridgeTest {
  static func render() -> ZoenVoiceEditor.Render {
    // This first peak came from the synthetic native QA AAC render. The
    // waveform widens decoded Float32 samples to Double before JSON encoding.
    let peaks = (0..<160).map { index -> Double in
      if index % 3 == 0 { return 0.051471274346113205 }
      let bits: UInt32 = 0x3e000000 + UInt32(index) * 10600
      return Double(Float(bitPattern: bits))
    }
    return .init(uri: "file:///synthetic/render.m4a", mime: "audio/mp4", frameCount: 80900,
      durationMs: Double(80900) / 24000 * 1000, levels: peaks)
  }
  static func main() throws {
    if CommandLine.arguments.last == "receipt" || CommandLine.arguments.last == "legacy" {
      let value = MediaDraftDto(revision: "r2", contentJson: "null", assets: [], attempted: false,
        acceptedIntentJson: CommandLine.arguments.last == "receipt" ? "{\"input_revision\":\"r1\",\"messages\":[\"sent\"],\"reply\":{\"reply_to\":\"source\",\"reply_intent\":\"A\"}}" : nil)
      let json = try JSONSerialization.data(withJSONObject: ZoenMediaDraftBridge.object(value))
      print(String(decoding: json, as: UTF8.self)); return
    }
    let actual = render()
    if CommandLine.arguments.last == "emit" {
      print(String(decoding: try JSONEncoder().encode(actual), as: UTF8.self))
      return
    }
    let data = FileHandle.standardInput.readDataToEndOfFile()
    let json = String(decoding: data, as: UTF8.self)
    var accepted = false
    var mismatches = 0
    if let value = try ZoenMediaDraftBridge.content(json) {
      if let received = value["levels"] as? [Double] {
        mismatches = zip(received, actual.levels).filter { $0.0 != $0.1 }.count
      }
      do {
        try ZoenMediaDraftBridge.renderMetadata(value, render: actual)
        accepted = true
      } catch { }
    }
    let result = try JSONSerialization.data(withJSONObject: ["accepted": accepted, "doubleMismatches": mismatches])
    print(String(decoding: result, as: UTF8.self))
  }
}
`;

test(
  'native voice admission accepts JSON-rounded Float32 peaks and rejects changed audio metadata',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-native-media-draft-'));
    try {
      const source = join(directory, 'Harness.swift');
      const binary = join(directory, 'probe');
      writeFileSync(source, harness);
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10_000,
      }).trim();
      const bridge = fileURLToPath(
        new URL('../../modules/zoen-core/ios/ZoenMediaDraftBridge.swift', import.meta.url),
      );
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-module-cache-path',
          join(directory, 'module-cache'),
          bridge,
          source,
          '-o',
          binary,
        ],
        { env: { ...process.env, SDKROOT: sdk }, timeout: 45_000 },
      );
      const native = JSON.parse(execFileSync(binary, ['emit'], { encoding: 'utf8' }));
      const accepted = JSON.parse(execFileSync(binary, ['receipt'], { encoding: 'utf8' }));
      assert.deepEqual(JSON.parse(accepted.accepted_intent_json), {
        input_revision: 'r1',
        messages: ['sent'],
        reply: { reply_to: 'source', reply_intent: 'A' },
      });
      assert.equal(
        Object.hasOwn(
          JSON.parse(execFileSync(binary, ['legacy'], { encoding: 'utf8' })),
          'accepted_intent_json',
        ),
        false,
      );
      const metadata = {
        mime: native.mime as string,
        duration_ms: native.durationMs as number,
        levels: native.levels as number[],
      };
      const check = (value: unknown): { accepted: boolean; doubleMismatches: number } =>
        JSON.parse(
          execFileSync(binary, ['check'], { input: JSON.stringify(value), encoding: 'utf8' }),
        );
      const valid = check(metadata);
      assert.ok(valid.doubleMismatches > 0, 'the fixture must cross the measured decimal boundary');
      assert.equal(valid.accepted, true);

      for (const changed of [
        { ...metadata, mime: 'audio/wav' },
        { ...metadata, duration_ms: metadata.duration_ms + 1 },
        { ...metadata, levels: [0.9, ...metadata.levels.slice(1)] },
        { ...metadata, levels: metadata.levels.slice(1) },
        { ...metadata, levels: [...metadata.levels, 0.5] },
        { ...metadata, levels: [1.01, ...metadata.levels.slice(1)] },
        { ...metadata, levels: [-0.01, ...metadata.levels.slice(1)] },
        { ...metadata, levels: [null, ...metadata.levels.slice(1)] },
      ])
        assert.equal(check(changed).accepted, false, 'changed or invalid metadata must reject');
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
