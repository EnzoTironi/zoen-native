import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import { transformSync } from 'esbuild';

const source = readFileSync(new URL('../notifications/system.ts', import.meta.url), 'utf8');
const { code } = transformSync(source, { loader: 'ts', format: 'cjs' });

for (const initialFirst of [false, true]) {
  test(`one local tap is handled once (initial response first: ${initialFirst})`, async () => {
    const response = {
      actionIdentifier: 'default',
      notification: { request: { identifier: 'message-1', trigger: null, content: { data: {} } } },
    };
    let receive: ((value: typeof response) => void) | undefined;
    let cleared = 0;
    const notifications = {
      DEFAULT_ACTION_IDENTIFIER: 'default',
      addNotificationReceivedListener: () => ({ remove() {} }),
      addNotificationResponseReceivedListener: (callback: typeof receive) => {
        receive = callback;
        return { remove() {} };
      },
      getLastNotificationResponseAsync: async () => response,
      clearLastNotificationResponseAsync: async () => { cleared += 1; },
    };
    const module = { exports: {} as typeof import('../notifications/system') };
    vm.runInNewContext(code, {
      module,
      require: (name: string) => name === 'expo-notifications' ? notifications : {},
    });
    let taps = 0;
    const stop = module.exports.listenNotificationEvents(() => {}, () => { taps += 1; });
    assert.ok(receive);
    if (initialFirst) await Promise.resolve();
    receive(response);
    await Promise.resolve();
    assert.equal(taps, 1);
    assert.equal(cleared, 1);
    stop();
  });
}
