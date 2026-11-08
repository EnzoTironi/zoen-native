import * as React from 'react';
import { createRequire } from 'node:module';
import { memberHookReactFixture } from './community-members-hook-react.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import * as lifetime from './conversation-scope.ts';
import * as flow from '../pins/session.ts';
import * as links from './message-link.ts';
import { messageText } from './presentation.ts';
import type { MessagePinReview } from './generated/MessagePinReview';
import type { TimelineEntry } from './types';
const require = createRequire(import.meta.url);
for (const file of [
  '../pins/use-message-pins.ts',
  '../pins/pin-review.tsx',
  '../pins/pinned-list.tsx',
  '../pins/pinned-message-bar.tsx',
])
  compileExpoSource(new URL(file, import.meta.url));
export async function pinsReactFixture(kind: 'review' | 'bar' | 'list' = 'review') {
  const f = await memberHookReactFixture();
  const space = f.snapshot.conversations[0]!.id,
    account = f.snapshot.account!;
  const entry: TimelineEntry = {
    id: 'pinned-message',
    seq: '1',
    author: f.snapshot.conversations[0]!.members[0]!,
    at_ms: '1000',
    text_revision: 'message-revision',
    kind: { Message: { text: 'Saturday meeting point', card: null } },
    delivery: 'Read',
    reply_to: null,
    in_thread: null,
    thread_replies: 0,
    edited: false,
    deleted: false,
    forwarded: false,
    reactions: [],
    voice: null,
    pinned: false,
    can_pin: true,
  };
  let review: MessagePinReview = {
    scope: {
      expected_identity_id: account.identity_id,
      expected_device_id: account.device_id,
      expected_relay_url: account.relay_url,
      space_id: space,
    },
    entry,
    pinned: false,
    can_pin: true,
    pin_revision: entry.id,
    pending: false,
  };
  const scope = lifetime.conversationScope(f.store.getSnapshot(), f.store.sessionEpoch, space)!;
  const buttons = new Map<string, any>(),
    texts = new Map<object, unknown>();
  const writes: any[] = [],
    routes: unknown[] = [],
    reveals: string[] = [];
  let id = 0;
  const original = f.driver.operation;
  let operation: (command: any) => Promise<unknown> = async (command) => {
    if (command.op === 'message_pin_review') return structuredClone(review);
    if (command.op === 'message_pins')
      return {
        entries: review.pinned ? [{ ...review.entry, pinned: true }] : [],
        total: review.pinned ? 1 : 0,
        next: null,
        revision: 'page-revision',
      };
    if (command.op === 'set_message_pin') {
      writes.push(structuredClone(command.input));
      review = {
        ...review,
        pinned: command.input.change.pinned,
        pin_revision: 'pin-revision',
        entry: { ...review.entry, pinned: command.input.change.pinned },
      };
      return { review: structuredClone(review), applied_revision: 'pin-revision' };
    }
    if (command.op === 'message')
      return structuredClone({ ...review.entry, pinned: review.pinned });
    return original(command);
  };
  const execute = operation;
  f.driver.operation = (command) => operation(command);
  function View(props: any) {
    return props.children;
  }
  function Text(props: any) {
    const key = React.useMemo(() => ({}), []);
    React.useLayoutEffect(() => {
      texts.set(key, props.children);
      return () => {
        texts.delete(key);
      };
    }, [key, props.children]);
    return null;
  }
  function Button(props: any) {
    const key = props.testID ?? props.label ?? props.accessibilityLabel;
    React.useLayoutEffect(() => {
      buttons.set(key, props);
      return () => {
        if (buttons.get(key) === props) buttons.delete(key);
      };
    }, [key, props]);
    return props.children;
  }
  function List(props: any) {
    return React.createElement(
      React.Fragment,
      null,
      props.data.map((item: TimelineEntry) =>
        React.createElement(React.Fragment, { key: item.id }, props.renderItem({ item })),
      ),
      props.ListFooterComponent,
    );
  }
  const ports = {
    ...f.ports,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      ...(f.ports['react-native'] as object),
      Text,
      View,
      ScrollView: View,
      Pressable: Button,
    },
    'expo-router': {
      ...(f.ports['expo-router'] as object),
      router: {
        push: (route: unknown) => routes.push(structuredClone(route)),
        replace: (route: unknown) => routes.push(structuredClone(route)),
      },
    },
    'expo-crypto': { randomUUID: () => `operation-${++id}` },
    '../core/conversation-scope': lifetime,
    './session': flow,
    '../components/primitives': {
      NativeButton: Button,
      IconButton: Button,
      Loading: () => null,
      EmptyState: View,
    },
    '../components/navigation-glass': { NavigationGlass: View },
    '../core/presentation': { messageText },
    './preview': {
      usePinnedText: (_space: string, entry?: TimelineEntry) => (entry ? messageText(entry) : ''),
    },
    '../components/forward-message-preview': {
      ForwardMessagePreview: (props: { entry: TimelineEntry }) =>
        React.createElement(Text, null, messageText(props.entry)),
    },
    '../theme': { useTheme: () => ({ colors: {} }) },
    '../i18n': {
      t: (key: string, params?: Record<string, unknown>) =>
        params ? `${key}:${Object.values(params).join(',')}` : key,
    },
    '@shopify/flash-list': { FlashList: List },
  };
  const hook = loadEditorAgent('../pins/use-message-pins.ts', ports);
  const navigation = loadEditorAgent('../pins/navigation.ts', {
    ...ports,
    '../core/message-link': links,
  });
  const navigationHook = loadEditorAgent('../pins/use-pinned-navigation.ts', {
    ...ports,
    './navigation': navigation,
  });
  const component = loadEditorAgent(
    `../pins/${kind === 'review' ? 'pin-review' : kind === 'bar' ? 'pinned-message-bar' : 'pinned-list'}.tsx`,
    { ...ports, './use-message-pins': hook, './use-pinned-navigation': navigationHook },
  );
  return {
    ...f,
    execute,
    entry,
    space,
    scope,
    buttons,
    writes,
    routes,
    reveals,
    text: () => [...texts.values()].flat().join(' '),
    operation: (next: typeof operation) => {
      operation = next;
    },
    nativeReview: () => review,
    updateReview: (patch: Partial<MessagePinReview>) => {
      review = { ...review, ...patch };
    },
    renderPins: () =>
      f.renderContent(
        React.createElement(
          component[
            kind === 'review' ? 'PinReview' : kind === 'bar' ? 'PinnedMessageBar' : 'PinnedList'
          ],
          {
            space,
            message: entry.id,
            owner: scope,
            onReveal: async (message: string) => {
              reveals.push(message);
            },
          },
        ),
      ),
    press: async (key: string) => {
      const button = buttons.get(key);
      if (!button || button.disabled) throw Error(`Unavailable ${key}`);
      await f.act(() => {
        button.onPress();
      });
    },
  };
}
