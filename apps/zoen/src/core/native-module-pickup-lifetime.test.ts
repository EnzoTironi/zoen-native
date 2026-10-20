import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

test(
  'native module destruction revokes pickup synchronously and rejects stale or duplicate Client bindings',
  { skip: process.platform !== 'darwin', timeout: 75_000 },
  async () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-module-pickup-'));
    try {
      const binary = join(directory, 'probe');
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10_000,
      }).trim();
      const files = [
        'ios/ZoenModulePickupLifetime.swift',
        'tests/ModulePickupLifetimeTests.swift',
      ].map((file) => fileURLToPath(new URL(`../../modules/zoen-core/${file}`, import.meta.url)));
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
        {
          env: { ...process.env, SDKROOT: sdk },
          detached: true,
          stdio: ['ignore', 'pipe', 'pipe'],
        },
      );
      let diagnostics = '';
      compiler.stdout.resume();
      compiler.stderr.setEncoding('utf8').on('data', (chunk: string) => {
        diagnostics += chunk;
      });
      let expired = false;
      const deadline = setTimeout(() => {
        expired = true;
        // swiftc owns frontend children. Kill the owned process group together;
        // killing only its driver leaves compilers using a deleted module cache.
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
        assert.equal(expired, false, 'Swift helper compilation exceeded its 45-second bound');
        assert.equal(status, 0, `Swift helper compilation failed (${signal}): ${diagnostics}`);
      } finally {
        clearTimeout(deadline);
      }
      const result = execFileSync(binary, [], { encoding: 'utf8', timeout: 10_000 });
      assert.match(result, /PASS module pickup lifetime/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
