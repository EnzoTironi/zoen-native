import * as messagePresentation from './message-presentation.ts';
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
import { conversationTitle } from '../saved-messages/presentation.ts';
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
const [previewCode, mediaCode, routeCode, panelCode, destinationsCode] = await Promise.all([
  compile('../components/forward-message-preview.tsx'),
  compile('../media/MediaAttachment.tsx'),
  compile('../app/forward/[space]/[message].tsx'),
  compile('../components/forward-review-panel.tsx'),
  compile('../components/chat-destinations.tsx'),
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
function testIds(value: React.ReactNode): string[] {
  const ids: string[] = [];
  React.Children.forEach(value, (child) => {
    if (!React.isValidElement<{ testID?: string; children?: React.ReactNode }>(child)) return;
    if (child.props.testID) ids.push(child.props.testID);
    ids.push(...testIds(child.props.children));
  });
  return ids;
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

for (const mode of [
  'cancel',
  'success',
  'unknownAck',
  'coldAccepted',
  'demo',
  'largeType',
  'saved',
] as const)
  test(`compiled forwarding route: ${mode} preserves keyboard/media and canonical operation semantics`, async () => {
    const root = createReactTestRoot();
    const views = new Map<string, any>();
    const events: string[] = [];
    const message = entry(card('photo'));
    const owner = {
      mode: mode === 'demo' ? 'demo' : 'account',
      identityId: 'alice',
      deviceId: 'phone',
    };
    const snapshot = {
      conversations: [
        { id: 'source', title: 'Source', can_write: true },
        {
          id: 'target',
          title: 'Target',
          kind: mode === 'saved' ? 'saved' : 'Group',
          can_write: true,
        },
      ],
    };
    let canonical: any =
      mode === 'coldAccepted'
        ? {
            target_space: 'target',
            operation_id: 'retained-operation',
            source_revision: 'retained-digest',
            message,
            accepted_message_id: 'accepted',
            accepted: { ...message, id: 'accepted' },
          }
        : null;
    const store = {
      sessionEpoch: 1,
      open: async (mode: string) => {
        assert.equal(mode, 'account');
        events.push('account');
      },
      getSnapshot: () => ({ snapshot }),
      subscribe: () => () => {},
      driver: {
        request: async (command: any) => {
          if (command.op === 'forward_status') {
            events.push('status');
            return canonical;
          }
          if (command.op === 'forward_review') {
            events.push('review');
            canonical = {
              target_space: command.scope.target_space,
              operation_id: 'canonical-operation',
              source_revision: 'canonical-digest',
              message,
              accepted_message_id: null,
              accepted: null,
            };
            return canonical;
          }
          if (command.op === 'dismiss_forward') {
            assert.equal(command.operation_id, canonical.operation_id);
            assert.equal(command.accepted_message_id, canonical.accepted_message_id);
            events.push(canonical.accepted_message_id ? 'ack' : 'cancel');
            canonical = null;
            return;
          }
          assert.fail(`Unexpected forwarding command ${command.op}`);
        },
      },
      request: async (command: any) => {
        assert.equal(command.op, 'confirm_forward');
        assert.equal(command.operation_id, canonical.operation_id);
        assert.equal(command.source_revision, canonical.source_revision);
        assert.equal(command.scope.target_space, canonical.target_space);
        events.push('send');
        canonical = {
          ...canonical,
          accepted_message_id: 'accepted',
          accepted: { ...message, id: 'accepted' },
        };
        if (mode === 'unknownAck') throw Error('ACK lost after commit');
        return canonical;
      },
    };
    const host =
      (name: string, children = false) =>
      (props: any) => {
        views.set(props.testID ?? name, props);
        return children ? React.createElement(React.Fragment, null, props.children) : null;
      };
    const ports: Record<string, unknown> = {
      react: React,
      'react/jsx-runtime': jsxRuntime,
      'react-native': {
        View: host('View', true),
        Text: host('Text'),
        TextInput: host('Search'),
        ScrollView: host('Scroll', true),
        Pressable: host('Pressable'),
        Keyboard: { dismiss: () => events.push('dismiss') },
        StyleSheet: { hairlineWidth: 0.5 },
        useWindowDimensions: () => ({
          width: mode === 'largeType' ? 320 : 402,
          fontScale: mode === 'largeType' ? 1.8 : 1,
        }),
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
      '../../../saved-messages/conversation-avatar': { ConversationAvatar: () => null },
      '../../../saved-messages/presentation': { conversationTitle },
      '../../../components/primitives': {
        NativeButton: host('Button'),
        EmptyState: host('Empty', true),
      },
    };
    ports['../../../components/forward-review-panel'] = load(panelCode, {
      react: React,
      'react/jsx-runtime': jsxRuntime,
      'react-native': ports['react-native'],
      'react-native-keyboard-controller': { KeyboardAvoidingView: host('Avoiding', true) },
      '../theme': {
        useTheme: () => ({
          presentation: {
            list: {
              text: '#111',
              secondary: '#777',
              block: '#fff',
              accent: '#08f',
              separator: '#ddd',
            },
          },
        }),
      },
      '../i18n/index.ts': ports['../../../i18n/index.ts'],
      '../profile/profile-avatar': ports['../../../profile/profile-avatar'],
      './primitives': ports['../../../components/primitives'],
      './forward-message-preview': { ForwardMessagePreview: host('Preview') },
      '../core/message-presentation': messagePresentation,
    });
    ports['../../../components/chat-destinations'] = load(destinationsCode, {
      'react/jsx-runtime': jsxRuntime,
      'react-native': ports['react-native'],
      '@shopify/flash-list': ports['@shopify/flash-list'],
      '../saved-messages/conversation-avatar': ports['../../../saved-messages/conversation-avatar'],
      '../saved-messages/presentation': ports['../../../saved-messages/presentation'],
      '../theme': ports['../../../theme'],
      '../i18n': ports['../../../i18n/index.ts'],
      './primitives': ports['../../../components/primitives'],
    });
    const route = load(routeCode, ports);
    try {
      await root.render(React.createElement(route.default));
      if (mode === 'demo') {
        assert.deepEqual(events, []);
        await root.act(() => views.get('forward-message-use-account').onPress());
        assert.deepEqual(events, ['account']);
      } else if (mode === 'coldAccepted') {
        assert.equal(views.get('forward-message-accepted').children, 'chat.alreadyForwarded');
        assert.equal(views.get('forward-message-send').label, 'chat.forwardAgain');
        assert.deepEqual(events, ['status']);
        await root.act(() => views.get('forward-message-send').onPress());
        assert.deepEqual(events, ['status', 'ack']);
        assert.equal(canonical, null);
      } else {
        await root.act(() =>
          views.get('forward-search').onChangeText(mode === 'saved' ? 'Saved' : 'Target'),
        );
        const list = views.get('List');
        assert.equal(list.keyboardShouldPersistTaps, 'handled');
        if (mode === 'saved') {
          assert.equal(list.data.length, 1);
          assert.equal(list.data[0].id, 'target');
          assert.equal(
            list.renderItem({ item: list.data[0] }).props.accessibilityLabel,
            'Saved Messages',
          );
        }
        assert.equal(list.keyboardDismissMode, 'interactive');
        await root.act(() => list.renderItem({ item: snapshot.conversations[1] }).props.onPress());
        assert.deepEqual(events, ['status', 'dismiss', 'status', 'review']);
        const preview = views.get('Preview');
        assert.deepEqual(preview.entry, message);
        assert.equal(preview.entry.kind.Message.card.item_id, 'original-photo');
        assert.equal(preview.spaceId, 'source');
        assert.equal(
          preview.contentWidth,
          mode === 'largeType' ? 256 : 338,
          'the renderer is constrained to the inset preview, not the screen width',
        );
        assert.equal(preview.isCurrent(), true);
        const scrolling = views.get('forward-message-preview-scroll');
        assert.ok(scrolling, 'long content has its own scroll owner');
        const scrollingIds = testIds(scrolling.children);
        assert.equal(scrollingIds.includes('forward-message-send'), false);
        assert.equal(scrollingIds.includes('forward-message-cancel'), false);
        assert.ok(views.has('forward-message-send') && views.has('forward-message-cancel'));

        assert.equal(views.get('forward-message-review').style.paddingBottom, 34);
        if (mode === 'cancel') {
          await root.act(() => views.get('forward-message-cancel').onPress());
          assert.equal(preview.isCurrent(), false);
          assert.deepEqual(events, ['status', 'dismiss', 'status', 'review', 'cancel']);
        } else {
          await root.act(() => views.get('forward-message-send').onPress());
          assert.deepEqual(
            events,
            mode === 'success' || mode === 'largeType' || mode === 'saved'
              ? ['status', 'dismiss', 'status', 'review', 'send', 'ack', 'back']
              : ['status', 'dismiss', 'status', 'review', 'send', 'status', 'back'],
          );
          assert.equal(
            canonical?.accepted_message_id ?? null,
            mode === 'unknownAck' ? 'accepted' : null,
          );
        }
      }
      assert.deepEqual(root.errors, []);
    } finally {
      await root.close();
    }
  });
