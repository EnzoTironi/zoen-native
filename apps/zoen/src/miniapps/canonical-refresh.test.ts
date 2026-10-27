import assert from 'node:assert/strict';
import { test } from 'node:test';
import { CanonicalSceneHost, hash, type CanonicalSceneDoc } from './canonical-scene.ts';
import { canonicalRefresh } from './canonical-refresh.ts';
const doc = (version: number): CanonicalSceneDoc => {
  const scene_json = JSON.stringify({
    elements: [{ id: 'title', text: `Version ${version}` }],
    appState: {},
    files: {},
  });
  return {
    item_id: 'item',
    version,
    current_version: version,
    scene_json,
    sha256: hash(new TextEncoder().encode(scene_json)),
  };
};
const held = () => {
  let resolve!: () => void;
  const promise = new Promise<void>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
function fixture() {
  let active = true,
    conflicts = 0,
    next = doc(2),
    readGate: ReturnType<typeof held> | null = null;
  let applied: Record<string, unknown> | null = null;
  let behavior: (args: Record<string, unknown>) => Promise<void> = async () => {};
  const writes: unknown[] = [];
  const check = () => {
    if (!active) throw Error('owner changed');
  };
  const scene = new CanonicalSceneHost({
    initialDoc: doc(1),
    check,
    id: () => 'id',
    save: async (input) => {
      writes.push(input);
      return doc(3);
    },
  });
  const refresh = canonicalRefresh({
    itemId: 'item',
    scene,
    check,
    read: async () => {
      if (readGate) await readGate.promise;
      check();
      return next;
    },
    conflict: () => {
      conflicts++;
    },
    call: async (name, args) => {
      if (name === 'resume_scene') return { content: [], structuredContent: {} };
      applied = args;
      await behavior(args);
      check();
      refresh.observe({
        zoen: {
          sceneSave: {
            ...next,
            itemId: 'item',
            phase: 'saved',
            ready: true,
            clean: true,
            liveRefresh: 1,
          },
        },
      });
      return { content: [], structuredContent: { ...args, itemId: 'item' } };
    },
  });
  const observe = (clean = true, version = 1) => {
    const head = doc(version);
    refresh.observe({
      zoen: {
        sceneSave: {
          itemId: 'item',
          version,
          sha256: head.sha256,
          phase: 'saved',
          ready: true,
          clean,
          liveRefresh: 1,
        },
      },
    });
  };
  return {
    scene,
    refresh,
    observe,
    writes,
    state: () => ({ conflicts, applied }),
    setBehavior: (fn: typeof behavior) => {
      behavior = fn;
    },
    stop: () => {
      active = false;
    },
    setNext: (value: CanonicalSceneDoc) => {
      next = value;
    },
    holdRead: () => {
      readGate = held();
      return readGate;
    },
  };
}

test('SDKv1 cannot expose an agent head; ready clean observation enables it reactively', async () => {
  const f = fixture();
  let notifications = 0;
  const stop = f.refresh.readiness.subscribe(() => {
    notifications++;
  });
  f.refresh.observe({ zoen: { sceneSave: { phase: 'saved', sequence: 0 } } });
  assert.equal(f.refresh.readiness.getSnapshot(), false);
  await assert.rejects(f.refresh.observedHead(), /not ready/);
  f.observe();
  assert.equal(f.refresh.readiness.getSnapshot(), true);
  assert.equal(notifications, 1);
  f.observe(false);
  assert.equal(f.refresh.readiness.getSnapshot(), false);
  assert.equal(notifications, 2);
  stop();
  f.refresh.dispose();
  f.scene.dispose();
});

test('candidate reads expose v2 but save base remains v1 until exact canvas ACK, then same host uses v2', async () => {
  const f = fixture();
  f.observe();
  const gate = held();
  f.setBehavior(async (args) => {
    const value = (await f.scene.call('app_scene_read', { id: 'item', refreshId: args.refreshId }))
      .structuredContent;
    assert.equal((value as { version: number }).version, 2);
    assert.equal(f.scene.head().version, 1);
    await assert.rejects(
      f.scene.call('app_scene_begin', {
        id: 'item',
        mutationId: 'race',
        bytes: 1,
        sha256: 'ab'.repeat(32),
      }),
      /applying/,
    );
    await gate.promise;
  });
  const work = f.refresh.refresh();
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(f.refresh.readiness.getSnapshot(), false);
  assert.equal(f.scene.head().version, 1);
  gate.resolve();
  await work;
  assert.equal(f.scene.head().version, 2);
  assert.equal(f.refresh.readiness.getSnapshot(), true);
  assert.deepEqual(await f.refresh.observedHead(), {
    itemId: 'item',
    version: 2,
    sha256: doc(2).sha256,
  });
  assert.equal(f.writes.length, 0, 'a refresh is not another save');
  f.refresh.dispose();
  f.scene.dispose();
});

for (const failure of [
  'dirty',
  'racing-dirty',
  'UI-v1',
  'wrong-ACK',
  'pin-revoked',
  'owner',
] as const)
  test(`refresh ${failure} preserves accepted v1 and admits no write`, async () => {
    const f = fixture();
    f.observe(failure !== 'dirty');
    if (failure === 'racing-dirty') {
      const gate = f.holdRead();
      const work = f.refresh.refresh();
      f.observe(false);
      gate.resolve();
      await work;
    } else if (failure === 'dirty') await f.refresh.refresh();
    else {
      f.setBehavior(async (args) => {
        if (failure === 'wrong-ACK') {
          args.refreshId = 'foreign-frame';
          return;
        }
        if (failure === 'owner') {
          f.stop();
          return;
        }
        throw Error(failure);
      });
      await assert.rejects(
        f.refresh.refresh(),
        new RegExp(
          failure === 'owner'
            ? 'owner changed'
            : failure === 'wrong-ACK'
              ? 'did not acknowledge'
              : failure,
        ),
      );
    }
    if (failure !== 'owner') assert.equal(f.scene.head().version, 1);
    assert.equal(f.writes.length, 0);
    f.refresh.dispose();
    f.scene.dispose();
  });

test('read-only reload cannot silently replace an accepted head with an older/different same version', async () => {
  const f = fixture();
  f.observe();
  f.setNext({ ...doc(2), version: 1, current_version: 1 });
  await assert.rejects(f.refresh.refresh(), /Invalid drawing refresh head/);
  assert.equal(f.scene.head().version, 1);
  assert.equal(f.writes.length, 0);
  f.refresh.dispose();
  f.scene.dispose();
});
