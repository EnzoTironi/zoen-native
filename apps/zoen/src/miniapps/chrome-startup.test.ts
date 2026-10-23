import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import type { ChildProcess } from 'node:child_process';
import { spawn } from 'node:child_process';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { chromeEndpoint, type ChromeStartup } from './chrome-startup.fixtures.ts';

test('a silent owned process can publish Chrome readiness after a partial endpoint write', async () => {
  const profile = await mkdtemp(join(tmpdir(), 'zoen-chrome-endpoint-'));
  const guid = '8a2c5695-bd14-440f-aea1-f03d3e3ad952';
  const child = spawn(
    process.execPath,
    [
      '-e',
      `const fs=require('node:fs'); const file=process.argv[1];
       fs.writeFileSync(file,'43210\\n');
       setTimeout(()=>fs.appendFileSync(file,'/devtools/browser/${guid}'),80);
       setInterval(()=>{},1000);`,
      join(profile, 'DevToolsActivePort'),
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] },
  );
  const closed = new Promise<void>((resolve) => child.once('close', () => resolve()));
  let stderr = '';
  child.stderr.on('data', (data: Buffer) => (stderr += data.toString()));
  const state: ChromeStartup = { portFile: 'unobserved', checksStarted: 0, checksCompleted: 0 };
  try {
    const endpoint = await chromeEndpoint(child, profile, state, (message) => new Error(message));
    assert.equal(endpoint, `ws://127.0.0.1:43210/devtools/browser/${guid}`);
    assert.equal(state.portFile, 'ready');
    assert.ok(state.checksCompleted > 0);
    assert.equal(state.checksStarted, state.checksCompleted);
    assert.equal(stderr, '');
    assert.equal(child.listenerCount('exit'), 0);
    assert.equal(child.listenerCount('error'), 0);
  } finally {
    child.kill();
    await closed;
    await rm(profile, { recursive: true, force: true });
  }
});

test('an exited process cannot supply an endpoint left in its owned profile', async () => {
  const profile = await mkdtemp(join(tmpdir(), 'zoen-chrome-ended-'));
  const child = spawn(process.execPath, ['-e', 'process.exit(3)'], { stdio: 'ignore' });
  await new Promise<void>((resolve) => child.once('close', () => resolve()));
  try {
    await assert.rejects(
      chromeEndpoint(
        child,
        profile,
        { portFile: 'unobserved', checksStarted: 0, checksCompleted: 0 },
        (message) => new Error(message),
      ),
      /Owned Chrome exited before connection/,
    );
    assert.equal(child.listenerCount('exit'), 0);
    assert.equal(child.listenerCount('error'), 0);
  } finally {
    await rm(profile, { recursive: true, force: true });
  }
});

test('a deadline before the first file operation completes never claims an observed missing endpoint', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const child = Object.assign(new EventEmitter(), {
    exitCode: null,
    signalCode: null,
  }) as ChildProcess;
  const state: ChromeStartup = { portFile: 'unobserved', checksStarted: 0, checksCompleted: 0 };
  const waiting = chromeEndpoint(
    child,
    '/owned-not-read-yet',
    state,
    (message) => new Error(message),
  );
  const rejected = assert.rejects(waiting, /Owned Chrome did not start/);
  assert.deepEqual(state, { portFile: 'unobserved', checksStarted: 1, checksCompleted: 0 });
  t.mock.timers.tick(10_000);
  await rejected;
  assert.deepEqual(state, { portFile: 'unobserved', checksStarted: 1, checksCompleted: 0 });
  assert.equal(child.listenerCount('exit'), 0);
  assert.equal(child.listenerCount('error'), 0);
});
