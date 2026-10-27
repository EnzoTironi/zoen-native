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
import type { TimelineEntry } from './types';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const file = new URL('../components/message-quote-selection.tsx', import.meta.url);
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
const entry = {
  id: 'message',
  text_revision: 'original',
  deleted: false,
  kind: { Message: { text: '**same** same', card: null } },
} as unknown as TimelineEntry;
for (const outcome of [
  'quote',
  'cancel',
  'retired',
  'unmapped',
  'refused',
  'cancel-review',
] as const) {
  test(`compiled native selection ${outcome} changes draft only after current explicit quotation dismissal`, async () => {
    const root = createReactTestRoot();
    let modal: any,
      markdown: any,
      button: any,
      current = true;
    let releaseReview: () => void = () => {};
    const quotes: unknown[] = [],
      closes: unknown[] = [];
    const container = (props: any) => props.children;
    const ports: Record<string, unknown> = {
      react: React,
      'react/jsx-runtime': jsxRuntime,
      'react-native': {
        Modal: (props: any) => {
          modal = props;
          return props.children;
        },
        View: container,
        ScrollView: container,
        Text: () => null,
        useWindowDimensions: () => ({ width: 320 }),
      },
      'react-native-safe-area-context': {
        useSafeAreaInsets: () => ({ top: 0, bottom: 24 }),
      },
      '../core/presentation': { messageText },
      '../core/selected-message-quote': { selectedMessageQuote },
      '../i18n/index.ts': { t: (key: string) => key },
      '../theme': { useTheme: () => ({ colors: {} }) },
      './message-markdown': {
        MessageMarkdown: (props: any) => {
          markdown = props;
          return null;
        },
      },
      './primitives': {
        NativeButton: (props: any) => {
          button = props;
          return null;
        },
      },
    };
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
    try {
      await root.render(
        React.createElement(module.exports.MessageQuoteSelection, {
          entry,
          current: () => current,
          onClose: () => closes.push(true),
          onQuote: (q: unknown) => quotes.push(q),
          review: async (q: unknown) => {
            if (outcome === 'refused') throw new Error('unsupported');
            if (outcome === 'cancel-review')
              return new Promise((resolve) => {
                releaseReview = () => resolve(q);
              });
            return q;
          },
        }),
      );
      assert.equal(markdown.text, '**same** same');
      assert.equal(markdown.selectable, true);
      if (outcome === 'cancel-review') {
        await root.act(() => {
          void markdown.onQuoteSelection(
            'same',
            JSON.stringify([{ start_utf16: 9, end_utf16: 13 }]),
          );
        });
        assert.equal(modal.visible, true);
        await root.act(() => button.onPress());
        await root.act(() => modal.onDismiss());
        await root.act(() => releaseReview());
        assert.equal(quotes.length, 0);
        assert.equal(modal.visible, false);
        return;
      }
      if (outcome === 'cancel') await root.act(() => button.onPress());
      else
        await root.act(() =>
          markdown.onQuoteSelection(
            'same',
            outcome === 'unmapped' ? '' : JSON.stringify([{ start_utf16: 9, end_utf16: 13 }]),
          ),
        );
      assert.equal(quotes.length, 0);
      if (outcome === 'unmapped' || outcome === 'refused') {
        assert.equal(modal.visible, true);
        return;
      }
      assert.equal(modal.visible, false);
      if (outcome === 'retired') current = false;
      await root.act(() => modal.onDismiss());
      assert.equal(quotes.length, outcome === 'quote' ? 1 : 0);
      if (outcome === 'quote') assert.equal((quotes[0] as any).text, 'same');
      assert.equal(closes.length, 1);
      assert.deepEqual(root.errors, []);
    } finally {
      await root.close();
    }
  });
}

test('actual compiled selector retires owner A permanently and old dismissal cannot clear owner B', async () => {
  const hookFile = new URL('../components/use-message-quote-selection.tsx', import.meta.url);
  const hookTransform = transformSync(readFileSync(hookFile, 'utf8'), {
    filename: hookFile.pathname,
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
  const hookCode = (
    await build({
      stdin: { contents: hookTransform, loader: 'tsx' },
      bundle: false,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
    })
  ).outputFiles[0]!.text;
  const root = createReactTestRoot();
  const listeners = new Set<() => void>();
  const commands: any[] = [];
  let releaseRead: () => void = () => {};
  const readBarrier = new Promise<void>((resolve) => {
    releaseRead = resolve;
  });
  const store = {
    getSnapshot: () => ({ snapshot: { account: { identity_id: 'A', device_id: 'device' } } }),
    driver: {
      request: async (command: any) => {
        commands.push(command);
        await readBarrier;
        return command.selection;
      },
    },
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
  const module = { exports: {} as any };
  let panel: any, api: any;
  const replies: string[] = [];
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': jsxRuntime,
    'expo-router': { useFocusEffect: (fn: () => () => void) => React.useEffect(fn, [fn]) },
    '../core/provider': { useClient: () => ({ store }) },
    '../core/presentation': { messageText },
    './message-quote-selection': {
      MessageQuoteSelection: (props: any) => {
        panel = props;
        return null;
      },
    },
  };
  runInNewContext(hookCode, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'react/compiler-runtime' || name.startsWith('@babel/runtime/'))
        return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  function Harness({ owner }: { owner: string }) {
    api = module.exports.useMessageQuoteSelection(
      () => entry,
      () => replies.push(owner),
      owner,
      'space',
      'thread-root',
    );
    return api.panel;
  }
  try {
    await root.render(React.createElement(Harness, { owner: 'A:epoch1' }));
    await root.act(() => api.open(entry));
    const old = panel;
    assert.equal(old.current(), true);
    const quote = {
      source_id: entry.id,
      revision: 'original',
      spans: [{ start_utf16: 9, end_utf16: 13 }],
      text: 'same',
    };
    const reviewed = old.review(quote);
    assert.deepEqual(JSON.parse(JSON.stringify(commands[0])), {
      op: 'review_reply_selection',
      expected_identity_id: 'A',
      expected_device_id: 'device',
      space_id: 'space',
      thread_root_id: 'thread-root',
      selection: quote,
    });
    assert.deepEqual(replies, []);
    // No store event or loading phase: a ready-to-ready ownership transition.
    await root.render(React.createElement(Harness, { owner: 'B:epoch2' }));
    assert.equal(old.current(), false);
    releaseRead();
    await assert.rejects(reviewed, /Selection retired/);
    await root.act(() => api.open(entry));
    const newer = panel;
    assert.equal(newer.current(), true);
    await root.act(() => old.onClose());
    assert.equal(newer.current(), true);
    assert.equal(api.panel.props.entry.id, entry.id);
    assert.equal(api.panel.props.entry.text_revision, entry.text_revision);
    await root.act(() => old.onQuote({ text: 'same' }));
    await root.render(React.createElement(Harness, { owner: 'A:epoch3' }));
    await root.act(() => old.onQuote({ text: 'same' }));
    assert.deepEqual(replies, []);
    assert.equal(old.current(), false);
    if ('Message' in entry.kind) {
      const previousText = entry.kind.Message.text;
      try {
        await root.act(() => api.open(entry));
        const captured = panel;
        entry.kind.Message.text = 'Changed during native selection';
        await root.render(React.createElement(Harness, { owner: 'A:epoch3' }));
        assert.equal(captured.current(), false);
      } finally {
        entry.kind.Message.text = previousText;
      }
    }
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});

test('compiled Markdown wrapper forwards genuine native provenance without inventing source offsets', async () => {
  const markdownFile = new URL('../components/message-markdown.tsx', import.meta.url);
  const markdownTransform = transformSync(readFileSync(markdownFile, 'utf8'), {
    filename: markdownFile.pathname,
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
  const markdownCode = (
    await build({
      stdin: { contents: markdownTransform, loader: 'tsx' },
      bundle: false,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
    })
  ).outputFiles[0]!.text;
  let native: any;
  const quotes: unknown[] = [];
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': jsxRuntime,
    'react-native': { Text: () => null },
    'react-native-enriched-markdown': {
      EnrichedMarkdownText: (props: any) => {
        native = props;
        return null;
      },
    },
    '../i18n/index.ts': { t: (key: string) => key },
    '../core/message-markdown': {
      messageMarkdownLink: () => null,
      messageMarkdownMayLoadMedia: () => false,
    },
    '../theme': { useTheme: () => ({ colors: {}, scheme: 'light' }) },
    './use-link-launch': { useLinkLaunch: () => () => {} },
    './use-message-code-copy': { useMessageCodeCopy: () => () => {} },
  };
  const module = { exports: {} as any };
  runInNewContext(markdownCode, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'react/compiler-runtime' || name.startsWith('@babel/runtime/'))
        return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const root = createReactTestRoot();
  try {
    await root.render(
      React.createElement(module.exports.MessageMarkdown, {
        text: '**same** same',
        color: '#111111',
        fontSize: 17,
        maxWidth: 280,
        selectable: true,
        onQuoteSelection: (text: string, ranges?: string) => quotes.push({ text, ranges }),
      }),
    );
    assert.equal(native.contextMenuItems.length, 1);
    assert.deepEqual(quotes, []);
    native.contextMenuItems[0].onPress({
      text: 'same',
      selection: { start: 5, end: 9 },
      sourceRangesJSON: '[{"start_utf16":9,"end_utf16":13}]',
    });
    assert.deepEqual(quotes, [{ text: 'same', ranges: '[{"start_utf16":9,"end_utf16":13}]' }]);
    native.contextMenuItems[0].onPress({ text: 'same', selection: { start: 5, end: 9 } });
    assert.deepEqual(quotes[1], { text: 'same', ranges: undefined });
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
