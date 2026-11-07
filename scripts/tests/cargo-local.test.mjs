import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';

test('Cargo identity, metadata and package invalidation retain Cargo semantics; builds use mbx', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'zoen-cargo-routing-'));
  try {
    await mkdir(join(directory, 'bin'));
    for (const binary of ['cargo', 'mbx'])
      await writeFile(join(directory, 'bin', binary), `#!/bin/sh\nprintf '%s\\n' '${binary}' "$@"\n`, { mode: 0o700 });
    for (const args of [['-Vv'], ['metadata', '--offline'], ['clean', '--package', 'client-core'], ['build', '-p', 'client-core'], ['check'], ['test'], ['clippy']]) {
      const value = spawnSync(resolve('scripts/cargo-local.sh'), args, {
        encoding: 'utf8', env: { ...process.env, CARGO_HOME: directory },
      });
      assert.equal(value.status, 0, value.stderr);
      const expected = ['-Vv', 'metadata', 'clean'].includes(args[0]) ? 'cargo' : 'mbx';
      assert.deepEqual(value.stdout.trim().split('\n'), [expected, ...args]);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});
