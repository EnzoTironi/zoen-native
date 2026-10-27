import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { allowLocalNotification } from '../notifications/local-admission.ts';
import { ClientStore } from './store.ts';
import { Driver, deferred } from './store.fixtures.ts';
import { mutedSnapshot } from './muted-chats.fixtures.ts';

async function fixture() {
  const snapshot = mutedSnapshot();
  const driver = new Driver();
  driver.opening = async () => structuredClone(snapshot);
  const store = new ClientStore(driver);
  await store.open('account');
  driver.operation = async () => structuredClone(snapshot);
  const device = snapshot.account!.device_id;
  const data = { zoen_local: 1, device_id: device, space_id: 'chat-0', event_seq: '1' };
  return {
    snapshot,
    store,
    driver,
    device,
    data,
    allow: () => allowLocalNotification(store, data, device),
  };
}

test('local presentation reads current Core expiry even when the cached conversation remains muted', async () => {
  const f = await fixture();
  try {
    assert.equal(f.store.getSnapshot().snapshot!.conversations[0]!.muted, true);
    Object.assign(f.snapshot.conversations[0]!, { muted: false, muted_until_ms: null });
    assert.equal(await f.allow(), true);
    assert.equal(
      f.store.getSnapshot().snapshot!.conversations[0]!.muted,
      true,
      'Admission reads canonical data without creating a second UI state authority',
    );
    f.snapshot.conversations[0]!.muted = true;
    assert.equal(await f.allow(), false);
    f.snapshot.conversations[0]!.muted = false;
    f.snapshot.conversations[0]!.archived = true;
    assert.equal(await f.allow(), false);
  } finally {
    await f.store.close();
  }
});

for (const transition of ['close', 'reopen', 'membership'] as const)
  test(`a late fresh admission read cannot present after ${transition}`, async () => {
    const f = await fixture();
    try {
      Object.assign(f.snapshot.conversations[0]!, { muted: false, muted_until_ms: null });
      const read = deferred<typeof f.snapshot>();
      f.driver.operation = () => read.promise;
      const pending = f.allow();
      if (transition === 'membership') {
        f.driver.operation = async () => {
          f.snapshot.conversations[0]!.members = [];
          return structuredClone(f.snapshot);
        };
        await f.store.refresh();
      } else {
        await f.store.close();
        if (transition === 'reopen') await f.store.open('account');
      }
      read.resolve(f.snapshot);
      assert.equal(await pending, false);
    } finally {
      await f.store.close();
    }
  });

test('fresh foreign owner or membership cannot authorize a cached local notification', async () => {
  const f = await fixture();
  try {
    f.snapshot.conversations[0]!.muted = false;
    f.snapshot.conversations[0]!.members = [];
    assert.equal(await f.allow(), false);
    f.snapshot.account!.device_id = 'c'.repeat(64);
    assert.equal(await f.allow(), false);
  } finally {
    await f.store.close();
  }
});

const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../notifications/system.ts', import.meta.url), 'utf8'),
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'os-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
function system() {
  let handler: any;
  const module = {
    exports: {} as {
      installNotificationHandler(callback: (data: unknown) => Promise<boolean>): () => void;
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    require(name: string) {
      if (name === 'expo-notifications')
        return {
          setNotificationHandler(value: unknown) {
            handler = value;
          },
        };
      if (name === 'expo-constants') return {};
      if (name === 'react-native') return { Platform: { OS: 'ios' } };
      if (name === '../../modules/zoen-core/src/ZoenCoreModule') return {};
      if (['./native-adapter', './configuration', './clear', '../i18n'].includes(name)) return {};
      throw Error(`Unexpected import ${name}`);
    },
  });
  return { install: module.exports.installNotificationHandler, handler: () => handler };
}
const notification = (type = 'date') => ({
  request: { trigger: { type }, content: { data: { synthetic: true } } },
});
const denied = {
  shouldPlaySound: false,
  shouldSetBadge: false,
  shouldShowBanner: false,
  shouldShowList: false,
};

test('OS handler awaits canonical admission and fences a result after cleanup', async () => {
  const f = system(),
    read = deferred<boolean>();
  const cleanup = f.install(() => read.promise);
  const handler = f.handler();
  const pending = handler.handleNotification(notification());
  cleanup();
  assert.equal(f.handler(), null);
  read.resolve(true);
  assert.deepEqual({ ...(await pending) }, denied);
  assert.deepEqual({ ...(await handler.handleNotification(notification())) }, denied);
});

test('OS handler fails closed on rejected reads and never asks local admission for pushes', async () => {
  const f = system();
  let calls = 0;
  f.install(async () => {
    ++calls;
    throw Error('Core unavailable');
  });
  assert.deepEqual({ ...(await f.handler().handleNotification(notification())) }, denied);
  assert.deepEqual({ ...(await f.handler().handleNotification(notification('push'))) }, denied);
  assert.equal(calls, 1);
});

test('OS handler presents local notifications only after a successful fresh admission', async () => {
  const f = system();
  f.install(async () => true);
  assert.deepEqual(
    { ...(await f.handler().handleNotification(notification())) },
    {
      ...denied,
      shouldPlaySound: true,
      shouldShowBanner: true,
      shouldShowList: true,
    },
  );
});
