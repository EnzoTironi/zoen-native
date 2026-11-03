import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { selectedMessageQuote } from './selected-message-quote.ts';
import { messageText } from './presentation.ts';
import { MessageTextSelection } from '../components/message-context-ports.ts';
import type { TimelineEntry } from './types';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
async function compile(path: string, ports: Record<string, unknown>) {
  const file = new URL(path, import.meta.url);
  const transformed = transformSync(readFileSync(file, 'utf8'), {
    filename: file.pathname,
    configFile: false,
    babelrc: false,
    presets: ['babel-preset-expo'],
    caller: {
      name: 'metro',
      platform: 'ios',
      engine: 'hermes',
      isDev: false,
      supportsReactCompiler: true,
    },
  }).code;
  const code = (
    await build({
      stdin: { contents: transformed, loader: 'tsx' },
      bundle: false,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
    })
  ).outputFiles[0]!.text;
  const module = { exports: {} as any };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'react/compiler-runtime' || name.startsWith('@babel/runtime/'))
        return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  return module.exports;
}
const entry = {
  id: 'source',
  text_revision: 'signed-content',
  deleted: false,
  kind: { Message: { text: '**same** same', card: null } },
} as unknown as TimelineEntry;
const ranges = '[{"start_utf16":9,"end_utf16":13}]';
const quote = selectedMessageQuote(entry, 'same', ranges)!;

test('compiled owner hook reviews before draft mutation; native completion accepts only same source/owner/token', async () => {
  const root = createReactTestRoot();
  let api: any, resolveRead!: (value: unknown) => void;
  const read = new Promise((resolve) => {
    resolveRead = resolve;
  });
  const calls: any[] = [],
    replies: any[] = [];
  const store = {
    getSnapshot: () => ({ snapshot: { account: { identity_id: 'A', device_id: 'device' } } }),
    subscribe: () => () => {},
    driver: {
      request: async (command: any) => {
        calls.push(command);
        return read;
      },
    },
  };
  const hook = await compile('../components/use-message-quote-selection.tsx', {
    react: React,
    'react/jsx-runtime': jsxRuntime,
    'expo-router': { useFocusEffect: (fn: () => () => void) => React.useEffect(fn, [fn]) },
    '../core/provider': { useClient: () => ({ store }) },
    '../core/presentation': { messageText },
    '../core/selected-message-quote': { selectedMessageQuote },
  });
  function Harness({ owner }: { owner: string }) {
    api = hook.useMessageQuoteSelection(
      () => entry,
      (entry: any, quote: any) => replies.push({ owner, entry, quote }),
      owner,
      'space',
      'thread-root',
    );
    return null;
  }
  try {
    await root.render(React.createElement(Harness, { owner: 'A:1' }));
    const old = api;
    const cancelOld = api.open(entry);
    const reviewed = api.review(entry, 'same', ranges);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].op, 'review_reply_selection');
    assert.equal(calls[0].thread_root_id, 'thread-root');
    assert.equal(calls[0].selection.source_id, entry.id);
    assert.equal(replies.length, 0);
    await root.render(React.createElement(Harness, { owner: 'B:2' }));
    const cancelNew = api.open(entry);
    cancelOld();
    resolveRead(quote);
    await assert.rejects(reviewed, /retired/);
    old.accept(entry, quote);
    old.open(entry);
    await assert.rejects(old.review(entry, 'same', ranges), /retired/);
    assert.equal(calls.length, 1);
    assert.equal(replies.length, 0);
    api.accept(entry, quote);
    assert.equal(replies.length, 1);
    assert.equal(replies[0].owner, 'B:2');
    assert.deepEqual(replies[0].quote, quote);
    cancelNew();
    api.accept(entry, quote);
    assert.equal(replies.length, 1);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});

for (const kind of ['plain', 'formatted', 'caption'] as const)
  test(`actual ${kind} leaf borrows existing rendered selection only inside source phase and restores ordinary content`, async () => {
    const root = createReactTestRoot();
    let native: any,
      mounts = 0,
      unmounts = 0,
      plain = 0;
    const selected: any[] = [];
    function Native(props: any) {
      React.useLayoutEffect(() => {
        native = props;
      }, [props]);
      React.useEffect(() => {
        mounts++;
        return () => {
          unmounts++;
        };
      }, []);
      return null;
    }
    function Plain() {
      React.useEffect(() => {
        plain++;
      }, []);
      return null;
    }
    const module = await compile('../components/message-markdown.tsx', {
      react: React,
      'react/jsx-runtime': jsxRuntime,
      'react-native': { Text: () => null },
      'react-native-enriched-markdown': { EnrichedMarkdownText: Native },
      '../i18n/index.ts': { t: (key: string) => key },
      '../core/message-markdown': {
        messageMarkdownLink: () => null,
        messageMarkdownMayLoadMedia: () => false,
      },
      '../theme': { useTheme: () => ({ colors: {}, scheme: 'light' }) },
      './use-link-launch': { useLinkLaunch: () => () => {} },
      './use-quote-collapse': { useQuoteCollapse: () => ({}) },
      './quote-layout-context': { useQuoteLayoutTransition: () => null },
      './use-message-code-copy': { useMessageCodeCopy: () => () => {} },
      './message-context-ports': { MessageTextSelection },
    });
    const props = {
      text: kind === 'formatted' ? '**same** same' : 'same same',
      color: '#111111',
      fontSize: 17,
      maxWidth: 280,
    };
    const element = React.createElement(
      kind === 'formatted' ? module.MessageMarkdown : module.MessageTextSelectionLeaf,
      props,
      React.createElement(Plain),
    );
    const render = async (active: boolean, error = false) =>
      root.render(
        React.createElement(
          MessageTextSelection,
          {
            value: {
              active,
              pending: false,
              error,
              quote: async (text: string, ranges?: string) => {
                selected.push({ text, ranges });
              },
            },
          },
          element,
        ),
      );
    try {
      await render(false);
      assert.equal(mounts, kind === 'formatted' ? 1 : 0);
      assert.equal(plain > 0, kind !== 'formatted');
      await render(true);
      assert.equal(mounts, 1);
      assert.equal(native.markdown, props.text);
      assert.equal(native.selectable, true);
      native.contextMenuItems[0].onPress({
        text: 'same',
        selection: { start: 5, end: 9 },
        sourceRangesJSON: ranges,
      });
      assert.deepEqual(selected, [{ text: 'same', ranges }]);
      await render(true, true);
      assert.equal(mounts, 1, 'feedback must not remount the selected native text');
      assert.equal(unmounts, 0);
      await render(false);
      assert.equal(unmounts, kind === 'formatted' ? 0 : 1);
      assert.deepEqual(root.errors, []);
    } finally {
      await root.close();
    }
  });
