import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

const patch = readFileSync(
  new URL('../../patches/@expo+ui+58.0.23.patch', import.meta.url),
  'utf8',
);
function addedSwift(name: string) {
  const marker = `+++ b/node_modules/@expo/ui/ios/Toggle/${name}\n`;
  const part = patch.split(marker)[1];
  assert.ok(part, `actual patch source ${name} must exist`);
  return (
    part
      .split('diff --git ')[0]
      .split('\n')
      .filter((line) => line.startsWith('+'))
      .map((line) => line.slice(1))
      .join('\n') + '\n'
  );
}

test(
  'actual patched Swift capture state refuses stale, foreign, retired and duplicate callbacks',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-theme-touch-'));
    try {
      const state = join(directory, 'ZoenThemeTouchState.swift');
      writeFileSync(state, addedSwift('ZoenThemeTouchState.swift'));
      const adapter = join(directory, 'ZoenThemeTouchProbe.swift');
      writeFileSync(adapter, addedSwift('ZoenThemeTouchProbe.swift'));
      const logger = join(directory, 'ZoenThemeInputProbe.swift');
      writeFileSync(logger, addedSwift('ZoenThemeInputProbe.swift'));
      // Parse actual UIKit/SwiftUI adapter; this does not typecheck or link iOS.
      execFileSync('xcrun', ['swiftc', '-frontend', '-parse', adapter, logger], {
        timeout: 10_000,
      });
      const binary = join(directory, 'probe');
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
          state,
          fileURLToPath(
            new URL('../../modules/zoen-core/tests/ThemeTouchStateTests.swift', import.meta.url),
          ),
          '-o',
          binary,
        ],
        { timeout: 40_000 },
      );
      const result = execFileSync(binary, [], { encoding: 'utf8', timeout: 10_000 });
      assert.match(
        result,
        /PASS exact opt-in, one touch, foreign\/stale callbacks, terminal ordering, cancellation, retirement and cap8/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
