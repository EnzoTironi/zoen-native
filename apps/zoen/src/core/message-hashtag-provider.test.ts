import assert from 'node:assert/strict';
import { test } from 'node:test';
import { build } from 'esbuild';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ConversationSearch } from './conversation-search.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');

test('production-compiled hashtag owner preserves stable links and fences blur, old chats and unmount', async () => {
  const compiled = await build({
    stdin: {
      contents:
        "export {ConversationHashtagProvider} from './src/components/conversation-hashtag-provider'; export {ConversationHashtagSearch} from './src/components/conversation-hashtag-search';",
      resolveDir: process.cwd(),
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'production-hashtag-owner',
        setup(builder) {
          builder.onLoad({ filter: /conversation-hashtag-provider\.tsx$/ }, ({ path }) => ({
            contents: transformSync(readFileSync(path, 'utf8'), {
              filename: path,
              configFile: false,
              babelrc: false,
              presets: ['babel-preset-expo'],
              plugins: [['babel-plugin-react-compiler', { target: '19' }]],
              caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
            }).code,
            loader: 'js',
          }));
          builder.onResolve(
            { filter: /^(react|react\/jsx-runtime|react\/compiler-runtime|expo-router)$/ },
            ({ path }) => ({ path, external: true }),
          );
        },
      },
    ],
  });
  const module = { exports: {} as any };
  let focused = true;
  const Focus = React.createContext(true);
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    TextEncoder,
    require(name: string) {
      if (name === 'expo-router')
        return {
          useFocusEffect: (callback: () => void | (() => void)) => {
            const isFocused = React.use(Focus);
            React.useEffect(() => (isFocused ? callback() : undefined), [callback, isFocused]);
          },
        };
      return require(name);
    },
  });
  const root = createReactTestRoot();
  const make = () =>
    new ConversationSearch({
      current: () => true,
      search: async () => ({ hits: [], next_cursor: null }),
    });
  const a = make(),
    b = make();
  const captured: { open: ((tag: string) => void) | null } = { open: null };
  const currentOpen = () => {
    const value = captured.open;
    assert.ok(value, 'the mounted provider supplies its local action');
    return value;
  };
  let activated = 0;
  function Capture() {
    captured.open = React.use(module.exports.ConversationHashtagSearch);
    return null;
  }
  const render = (search: ConversationSearch, amount: number) =>
    root.render(
      React.createElement(
        Focus,
        { value: focused },
        React.createElement(
          module.exports.ConversationHashtagProvider,
          {
            search,
            onActivate: () => {
              activated += amount;
            },
          },
          React.createElement(Capture),
        ),
      ),
    );
  try {
    await render(a, 1);
    const original = currentOpen();
    await root.act(() => original('#release'));
    assert.equal(a.getSnapshot().query, '#release');
    assert.equal(activated, 1);
    await render(a, 10);
    assert.equal(
      currentOpen(),
      original,
      'canonical refresh must not retire an unrelated link launch',
    );
    await root.act(() => original('#next'));
    assert.equal(activated, 11);
    focused = false;
    await render(a, 10);
    await root.act(() => original('#offscreen'));
    assert.equal(a.getSnapshot().query, '#next');
    assert.equal(activated, 11);
    focused = true;
    await render(b, 100);
    await root.act(() => original('#old_chat'));
    assert.equal(a.getSnapshot().query, '#next');
    assert.equal(b.getSnapshot().active, false);
    assert.equal(activated, 11);
    const current = currentOpen();
    await root.act(() => current('#new_chat'));
    assert.equal(b.getSnapshot().query, '#new_chat');
    assert.equal(activated, 111);
    await root.close();
    current('#unmounted');
    assert.equal(activated, 111);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
