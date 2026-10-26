import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as React from 'react';
import * as jsxRuntime from 'react/jsx-runtime';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import {
  ForwardMessageSession,
  ForwardMessageUnavailable,
  ForwardMessageChanged,
} from './forward-message.ts';
import type { TimelineEntry } from './types';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
async function compile(path: string) {
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
  return (
    await build({
      stdin: { contents: transformed, loader: 'tsx' },
      bundle: false,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
    })
  ).outputFiles[0]!.text;
}
const [previewCode, mediaCode, routeCode] = await Promise.all([
  compile('../components/forward-message-preview.tsx'),
  compile('../media/MediaAttachment.tsx'),
  compile('../app/forward/[space]/[message].tsx'),
]);
function load(code: string, ports: Record<string, unknown>) {
  const module = { exports: {} as Record<string, any> };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (path: string) => {
      if (path === 'react/compiler-runtime' || path.startsWith('@babel/runtime/'))
        return require(path);
      assert.ok(path in ports, `Missing forwarding presentation port ${path}`);
      return ports[path];
    },
  });
  return module.exports;
}
const entry = (card: unknown = null, voice: unknown = null): TimelineEntry =>
  ({
    id: 'reviewed',
    deleted: false,
    author: { id: 'alice', name: 'Alice', is_me: true, kind: 'Person' },
    kind: { Message: { text: '**Reviewed caption**', card } },
    voice,
  }) as TimelineEntry;
const card = (label: string) => ({
  kind_id: 'file',
  item_id: `original-${label}`,
  version: 7,
  title: `${label}.original`,
  kind_label: label,
  summary: null,
});

for (const kind of ['text', 'photo', 'video', 'file', 'voice'] as const) {
  test(`forward ${kind} review uses the canonical renderer and exact source, without editing or forwarding actions`, async () => {
    const root = createReactTestRoot();
    const files: Record<string, any>[] = [],
      markdown: Record<string, any>[] = [],
      focus: string[][] = [];
    let current = true;
    const isCurrent = () => current;
    const message = entry(
      kind === 'text' || kind === 'voice' ? null : card(kind),
      kind === 'voice'
        ? {
            item_id: 'original-voice',
            sha256: 'a'.repeat(64),
            ms: '1200',
            transcript: 'Exact words',
            levels: [0.2],
          }
        : null,
    );
    const media = load(mediaCode, {
      'react/jsx-runtime': jsxRuntime,
      './file-attachment-content': {
        __esModule: true,
        default: (props: Record<string, any>) => {
          files.push(props);
          return null;
        },
      },
    });
    const component = load(previewCode, {
      'react/jsx-runtime': jsxRuntime,
      'react-native': {
        View: (props: any) => React.createElement(React.Fragment, null, props.children),
        Text: () => null,
        useWindowDimensions: () => ({ width: 402 }),
      },
      '../theme': {
        useTheme: () => ({ colors: { text: '#111', secondary: '#777', mine: '#08f' } }),
      },
      '../media/MediaAttachment': media,
      '../media/message-audio-provider': {
        useMessageAudioFocus: (...args: string[]) => focus.push(args),
      },
      './message-markdown': {
        MessageMarkdown: (props: Record<string, any>) => {
          markdown.push(props);
          return null;
        },
      },
    });
    try {
      await root.render(
        React.createElement(component.ForwardMessagePreview, {
          spaceId: 'source',
          entry: message,
          isCurrent,
        }),
      );
      assert.deepEqual(root.errors, []);
      assert.equal(markdown.at(-1)?.text, '**Reviewed caption**');
      assert.equal(markdown.at(-1)?.selectable, true);
      assert.equal(focus.at(-1)?.[0], 'source');
      if (kind === 'text') assert.equal(files.length, 0);
      else {
        const rendered = files.at(-1)!;
        assert.equal(rendered.itemId, kind === 'voice' ? 'original-voice' : `original-${kind}`);
        assert.equal(rendered.version, kind === 'voice' ? undefined : 7);
        assert.equal(rendered.expectedHash, kind === 'voice' ? 'a'.repeat(64) : undefined);
        assert.equal(rendered.messageSource.spaceId, 'source');
        assert.strictEqual(rendered.messageSource.entry, message);
        assert.strictEqual(rendered.isCurrent, isCurrent);
        assert.equal(rendered.presentation, 'message');
        assert.equal(
          rendered.tile,
          undefined,
          'review retains contain/aspect geometry, never mosaic crop',
        );
        assert.equal(rendered.onActions, undefined);
        assert.equal(rendered.autoPlay, undefined);
        assert.equal(rendered.allowsEditing, undefined);
        current = false;
        assert.equal(
          rendered.isCurrent(),
          false,
          'retained native work shares the review retirement fence',
        );
      }
    } finally {
      await root.close();
    }
  });
}

test('destination selection dismisses focused search on its first handled tap, renders reviewed media, and retires it on Cancel', async () => {
  const root = createReactTestRoot();
  const views = new Map<string, any>();
  const events: string[] = [];
  const message = entry(card('photo'));
  const owner = { mode: 'account', identityId: 'alice', deviceId: 'phone' };
  const snapshot = {
    conversations: [
      { id: 'source', title: 'Source', can_write: true },
      { id: 'target', title: 'Target', can_write: true },
    ],
  };
  const store = {
    sessionEpoch: 1,
    getSnapshot: () => ({ snapshot }),
    subscribe: () => () => {},
    driver: {
      request: async () => {
        events.push('read');
        return message;
      },
    },
    request: () => assert.fail('review may not send'),
  };
  const host =
    (name: string, children = false) =>
    (props: any) => {
      views.set(props.testID ?? name, props);
      return children ? React.createElement(React.Fragment, null, props.children) : null;
    };
  const route = load(routeCode, {
    react: React,
    'react/jsx-runtime': jsxRuntime,
    'react-native': {
      View: host('View', true),
      Text: host('Text'),
      TextInput: host('Search'),
      ScrollView: host('Scroll', true),
      Pressable: host('Pressable'),
      Keyboard: { dismiss: () => events.push('dismiss') },
    },
    'expo-router': {
      Stack: { Screen: () => null },
      router: { back: () => events.push('back') },
      useLocalSearchParams: () => ({ space: 'source', message: 'reviewed' }),
      useFocusEffect: (work: () => () => void) => React.useEffect(work, [work]),
    },
    'expo-router/react-navigation': { useHeaderHeight: () => 64 },
    'react-native-safe-area-context': { useSafeAreaInsets: () => ({ bottom: 34 }) },
    '@shopify/flash-list': { FlashList: host('List') },
    '../../../core/provider': { useClient: () => ({ store, snapshot }) },
    '../../../core/forward-message': {
      ForwardMessageSession,
      ForwardMessageUnavailable,
      ForwardMessageChanged,
    },
    '../../../home/access': {
      homeOwnerFor: () => owner,
      homeSpacesFor: () => new Set(['source', 'target']),
    },
    '../../../theme': { useTheme: () => ({ colors: { text: '#111', secondary: '#777' } }) },
    '../../../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en-US' },
    '../../../profile/profile-avatar': { ProfileAvatar: () => null },
    '../../../components/primitives': { NativeButton: host('Button'), EmptyState: () => null },
    '../../../components/forward-message-preview': { ForwardMessagePreview: host('Preview') },
  });
  try {
    await root.render(React.createElement(route.default));
    await root.act(() => views.get('Search').onChangeText('Target'));
    const list = views.get('List');
    assert.equal(list.keyboardShouldPersistTaps, 'handled');
    assert.equal(list.keyboardDismissMode, 'interactive');
    await root.act(() => list.renderItem({ item: snapshot.conversations[1] }).props.onPress());
    assert.deepEqual(events, ['dismiss', 'read']);
    const preview = views.get('Preview');
    assert.deepEqual(preview.entry, message);
    assert.equal(preview.entry.kind.Message.card.item_id, 'original-photo');
    assert.equal(preview.spaceId, 'source');
    assert.equal(preview.isCurrent(), true);
    assert.equal(views.get('forward-message-review').style.paddingBottom, 34);
    await root.act(() => views.get('forward-message-cancel').onPress());
    assert.equal(preview.isCurrent(), false);
    assert.deepEqual(
      events,
      ['dismiss', 'read'],
      'Cancel does not forward or navigate into a source editor',
    );
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
