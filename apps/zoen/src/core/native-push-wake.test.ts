import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { localNotes, type LocalNote } from '../notifications/local.ts';
import type { Snapshot } from './types.ts';

const nativeFile = (path: string) =>
  fileURLToPath(new URL(`../../modules/zoen-core/${path}`, import.meta.url));
const fixture = nativeFile('tests/fixtures/push-snapshots.json');

test('JS and native notification models share exact IDs, UInt64 cursors and human content vectors', () => {
  const value = JSON.parse(readFileSync(fixture, 'utf8')) as {
    before: Snapshot;
    after: Snapshot;
    expected: LocalNote[];
  };
  assert.deepEqual(localNotes(value.before, value.after), value.expected);
  assert.deepEqual(localNotes(value.after, value.after), []);
});

test(
  'actual native push gate serializes owners, cancels expiry and retires late effects without another Client owner',
  { skip: process.platform !== 'darwin', timeout: 75_000 },
  async () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-native-push-'));
    try {
      const binary = join(directory, 'probe');
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10_000,
      }).trim();
      const files = [
        'ios/ZoenProcessSessionGate.swift',
        'ios/ZoenPushModel.swift',
        'tests/PushWakeTests.swift',
      ].map(nativeFile);
      const env: NodeJS.ProcessEnv = {
        NODE_ENV: 'test',
        ...Object.fromEntries(
          ['PATH', 'HOME', 'USER', 'LOGNAME', 'TMPDIR'].flatMap((key) =>
            process.env[key] === undefined ? [] : [[key, process.env[key]!]],
          ),
        ),
      };
      const compiler = spawn(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-module-cache-path',
          join(directory, 'module-cache'),
          ...files,
          '-o',
          binary,
        ],
        { env: { ...env, SDKROOT: sdk }, detached: true, stdio: ['ignore', 'pipe', 'pipe'] },
      );
      let diagnostics = '';
      compiler.stdout.resume();
      compiler.stderr.setEncoding('utf8').on('data', (chunk: string) => {
        diagnostics += chunk;
      });
      let expired = false;
      const timer = setTimeout(() => {
        expired = true;
        if (compiler.pid) {
          try {
            process.kill(-compiler.pid, 'SIGKILL');
          } catch (error) {
            if ((error as NodeJS.ErrnoException).code !== 'ESRCH') throw error;
          }
        }
      }, 45_000);
      try {
        const [status, signal] = await once(compiler, 'close');
        assert.equal(expired, false, 'native push model compilation exceeded its bound');
        assert.equal(status, 0, `native push model compilation failed (${signal}): ${diagnostics}`);
      } finally {
        clearTimeout(timer);
      }
      const result = execFileSync(binary, [fixture], { encoding: 'utf8', timeout: 10_000, env });
      assert.match(
        result,
        /PASS native push gate, scope, payload, projection, expiry\/preemption and cleanup models/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
