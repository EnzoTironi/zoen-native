import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = new URL('../../modules/zoen-core/', import.meta.url);

test('caller and callee install renewed TURN credentials before restarting negotiation', () => {
  // Wiring guard; the executable below tests the actual scoped renewal contract.
  // Neither test executes WebRTC media or claims physical call acceptance.
  const source = readFileSync(new URL('ios/Calls/ZoenCallRTC.swift', root), 'utf8');
  const offer = source.split('case "offer":')[1]!.split('case "answer":')[0]!;
  assert.match(
    offer,
    /if generation > 0 \{ try await renewTurn\(peer, generation: generation\) \}/,
  );
  assert.ok(offer.indexOf('renewTurn(') < offer.indexOf('try await remote('));
  const failed = source.split('case .failed:')[1]!.split('case .closed:')[0]!;
  assert.match(failed, /try await rtc\.renewTurn\(peer, generation: rtc\.generation\)/);
  assert.ok(failed.indexOf('renewTurn(') < failed.indexOf('peer.restartIce()'));
  assert.match(source, /live && peer === connection && generation == expected/);
  assert.match(source, /configuration\.iceTransportPolicy = \.relay/);
  assert.equal(source.includes('port.turn()'), false);
});

test(
  'actual native TURN renewal refuses late owner/generation and invalid credentials',
  { skip: process.platform !== 'darwin', timeout: 150000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-call-turn-renewal-'));
    try {
      const binary = join(directory, 'renewal');
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-j',
          '1',
          '-module-cache-path',
          join(directory, 'modules'),
          ...['ios/Calls/ZoenCallModel.swift', 'tests/CallTurnRenewalTests.swift'].map((path) =>
            fileURLToPath(new URL(path, root)),
          ),
          '-o',
          binary,
        ],
        { encoding: 'utf8', timeout: 120000 },
      );
      assert.match(
        execFileSync(binary, [], { encoding: 'utf8', timeout: 10000 }),
        /PASS scoped native TURN renewal/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
