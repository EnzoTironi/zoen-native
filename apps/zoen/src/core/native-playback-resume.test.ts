import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
const harness = String.raw`
import Foundation
import AVFoundation
enum ZoenFilePreview { enum Failure: Error { case changed, unavailable } }
actor Probe {
  var writes: [UInt32] = []
  func save(_ revision: String, _ position: ZoenMediaPlaybackBookmark.Position) -> ZoenMediaPlaybackBookmark.Value {
    precondition(revision == "r\(writes.count + 1)"); writes.append(position.position_ms)
    return .init(revision: "r\(writes.count + 1)", position: position)
  }
}
@main struct Harness {
  @MainActor static func main() async throws {
    let probe=Probe()
    let bookmark=ZoenMediaPlaybackBookmark(value:.init(revision:"r1",position:.init(position_ms:1250,duration_ms:2000,rate_milli:1000))) { revision,position in await probe.save(revision,position) }
    let item=AVPlayerItem(url:URL(fileURLWithPath:CommandLine.arguments[1]))
    let player=AVPlayer(playerItem:item)
    let playback=ZoenConversationVideoPlayback(player:player,bookmark:bookmark)
    playback.setActive(true)
    _ = try await item.asset.load(.duration)
    for _ in 0..<200 {
      if playback.state.seekable && abs(player.currentTime().seconds-1.25)<0.01 {break}
      try await Task.sleep(nanoseconds:25_000_000)
    }
    precondition(playback.state.seekable,"native asset duration never became available")
    precondition(abs(playback.state.position-1.25)<0.01,"restored transport point")
    precondition(abs(player.currentTime().seconds-1.25)<0.01,"actual AVPlayer seek")
    precondition(!playback.state.playing && player.rate==0,"restore must not autoplay")
    playback.seek(to:1.5)
    await bookmark.flush()
    let writes=await probe.writes
    precondition(writes.last==1500,"paused seek must be persisted")
    playback.retire()
    precondition(player.rate==0 && playback.retired)
    print("{\"restored\":true,\"autoplay\":false,\"saved\":1500}")
  }
}
`;
test(
  'real native AVPlayer restores a paused checkpoint, saves seek and retires without autoplay',
  { skip: process.platform !== 'darwin', timeout: 150000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-playback-resume-'));
    try {
      const source = join(directory, 'Harness.swift'),
        binary = join(directory, 'probe'),
        wav = join(directory, 'fixture.wav');
      writeFileSync(source, harness);
      const bytes = Buffer.alloc(44 + 32000);
      bytes.write('RIFF');
      bytes.writeUInt32LE(bytes.length - 8, 4);
      bytes.write('WAVEfmt ', 8);
      bytes.writeUInt32LE(16, 16);
      bytes.writeUInt16LE(1, 20);
      bytes.writeUInt16LE(1, 22);
      bytes.writeUInt32LE(8000, 24);
      bytes.writeUInt32LE(16000, 28);
      bytes.writeUInt16LE(2, 32);
      bytes.writeUInt16LE(16, 34);
      bytes.write('data', 36);
      bytes.writeUInt32LE(32000, 40);
      writeFileSync(wav, bytes);
      const native = (path: string) =>
        fileURLToPath(new URL(`../../modules/zoen-core/ios/${path}`, import.meta.url));
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
      }).trim();
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          native('ZoenConversationGalleryRequest.swift'),
          native('TelegramConversationVideo/ZoenConversationVideoPlayback.swift'),
          source,
          '-o',
          binary,
        ],
        { env: { ...process.env, SDKROOT: sdk }, timeout: 120000 },
      );
      assert.deepEqual(
        JSON.parse(execFileSync(binary, [wav], { encoding: 'utf8', timeout: 15000 })),
        { restored: true, autoplay: false, saved: 1500 },
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
