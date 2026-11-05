import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { runInNewContext } from 'node:vm';
import { createRequire } from 'node:module';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
const require = createRequire(import.meta.url);

test('thread hashtag handoff opens the chat composer with only an opaque scoped ticket and retires old taps', async () => {
  const driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => structuredClone(snapshot);
  const store = new ClientStore(driver);
  await store.open('account');
  const code = await build({
    stdin: {
      contents:
        "export {ThreadHashtagProvider} from './src/components/thread-hashtag-provider'; export {ConversationHashtagSearch} from './src/components/conversation-hashtag-search'; export {searchLaunches} from './src/core/search-launch';",
      resolveDir: process.cwd(),
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'thread-navigation-ports',
        setup(builder) {
          builder.onResolve(
            {
              filter:
                /^(react|react\/jsx-runtime|expo-router|expo-crypto|\.\.\/core\/provider|\.\/provider)$/,
            },
            ({ path }) => ({ path, external: true }),
          );
        },
      },
    ],
  });
  const routes: unknown[] = [];
  const module = { exports: {} as any };
  runInNewContext(code.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    TextEncoder,
    require(name: string) {
      if (name === 'react') return React;
      if (name === 'expo-router')
        return {
          router: { push: (route: unknown) => routes.push(route) },
          useFocusEffect: (callback: () => void | (() => void)) =>
            React.useEffect(callback, [callback]),
        };
      if (name === 'expo-crypto') return { randomUUID: () => 'opaque-thread-ticket' };
      if (name === '../core/provider' || name === './provider')
        return {
          useClient: () => ({
            store,
            ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
          }),
        };
      return require(name);
    },
  });
  const root = createReactTestRoot();
  const captured: { open: ((tag: string) => void) | null } = { open: null };
  function Capture() {
    captured.open = React.use(module.exports.ConversationHashtagSearch);
    return null;
  }
  try {
    const space = snapshot.conversations[0]!.id;
    await root.render(
      React.createElement(
        module.exports.ThreadHashtagProvider,
        { space },
        React.createElement(Capture),
      ),
    );
    const open = captured.open;
    if (!open) throw Error('missing hashtag action');
    await root.act(() => open('#private_topic'));
    assert.deepEqual(JSON.parse(JSON.stringify(routes)), [
      { pathname: '/chat/[id]', params: { id: space, searchTicket: 'opaque-thread-ticket' } },
    ]);
    assert.equal(
      module.exports.searchLaunches.read('opaque-thread-ticket', store).query,
      '#private_topic',
    );
    assert.equal(JSON.stringify(routes).includes('private_topic'), false);
    await root.close();
    open('#retired');
    assert.equal(routes.length, 1);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
    await store.close();
  }
});
