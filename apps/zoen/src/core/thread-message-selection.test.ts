import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import { hookHost } from './message-selection-react.fixtures.ts';
import type { TimelineEntry } from './types';

type Element = { type: string; props: Record<string, unknown> };
const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../app/thread/[space]/[root].tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  jsx: 'automatic',
  bundle: true,
  external: ['*'],
  platform: 'node',
  format: 'cjs',
  write: false,
});
function find(value: unknown, type: string): Element | undefined {
  if (!value || typeof value !== 'object') return;
  if (Array.isArray(value)) {
    for (const child of value) {
      const found = find(child, type);
      if (found) return found;
    }
    return;
  }
  const element = value as Element;
  return element.type === type ? element : find(element.props?.children, type);
}
function fixture(active = true) {
  const host = hookHost();
  const calls: string[] = [];
  const entries = ['original', 'reply'].map(
    (id) => ({ id, kind: { Message: { text: id, card: null } } }) as TimelineEntry,
  );
  const toggle = (entry: TimelineEntry) => calls.push(`toggle:${entry.id}`);
  const selection = {
    active,
    ids: new Set(['original']),
    count: 1,
    limitReached: true,
    canCopy: true,
    canForward: true,
    toggle,
    cancel: () => calls.push('cancel'),
    copy: () => calls.push('copy'),
    forward: () => calls.push('forward'),
  };
  let hookArgs: unknown[] = [];
  let actionArgs: { disabled: boolean; onSelect: (entry: TimelineEntry) => void } | undefined;
  const actions = {
    busy: false,
    panel: null,
    reply() {},
    thread() {},
    profile() {},
    open() {},
    react() {},
  };
  const module = { exports: {} as { default(): unknown } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require(name: string) {
      if (name === 'react') return host.react;
      if (name === 'react/jsx-runtime')
        return {
          jsx: (type: string, props: Record<string, unknown>) => ({ type, props }),
          jsxs: (type: string, props: Record<string, unknown>) => ({ type, props }),
          Fragment: 'Fragment',
        };
      if (name === 'expo-router')
        return {
          router: { push() {} },
          Stack: { Screen: 'Screen' },
          useFocusEffect: host.useFocusEffect,
          useLocalSearchParams: () => ({ space: 'space', root: 'root-message' }),
        };
      if (name === 'expo-router/react-navigation') return { useHeaderHeight: () => 44 };
      if (name === 'react-native-safe-area-context')
        return { useSafeAreaInsets: () => ({ top: 20 }) };
      if (name === '@shopify/flash-list') return { FlashList: 'FlashList' };
      if (name === 'react-native') return { Text: 'Text', View: 'View' };
      if (name.endsWith('/i18n/index.ts')) return { t: (key: string) => key };
      if (name.endsWith('/core/query'))
        return { useCoreQuery: () => ({ data: entries, error: null, retry() {} }) };
      if (name.endsWith('/core/provider'))
        return {
          useClient: () => ({
            store: { sessionEpoch: 1, setTyping() {} },
            snapshot: { conversations: [{ id: 'space', kind: 'Group' }] },
            mode: 'account',
          }),
        };
      if (name.endsWith('/message-audio-provider')) return { useMessageAudioFocus() {} };
      if (name.endsWith('/mosaic-layout')) return { supportsMediaMosaic: true };
      if (name.endsWith('/core/presentation')) return { messageText: () => '' };
      if (name.endsWith('/message-presentation')) return { messageCard: () => null };
      if (name.endsWith('/conversation-timeline'))
        return {
          projectConversationTimeline: () => [
            { key: 'album', entry: entries[0], album: { entries } },
          ],
        };
      if (name.endsWith('/use-message-draft'))
        return {
          useMessageDraft: () => ({
            messageDraft: { text: 'draft', reply: null, editing: null, editText: '' },
            updateDraft() {},
            capture: () => null,
          }),
        };
      if (name.endsWith('/primitives'))
        return { EmptyState: 'Empty', Loading: 'Loading', NativeButton: 'Button' };
      if (name.endsWith('/theme')) return { useTheme: () => ({ colors: {} }) };
      if (name.endsWith('/home/provider')) return { useHome: () => ({ allItems: [] }) };
      if (name.endsWith('/message-composer')) return { MessageComposer: 'Composer' };
      if (name.endsWith('/use-message-selection'))
        return {
          useMessageSelection: (...args: unknown[]) => {
            hookArgs = args;
            return selection;
          },
        };
      if (name.endsWith('/message-selection-toolbar'))
        return {
          MessageSelectionToolbar: 'SelectionToolbar',
          MessageSelectionHeader: 'SelectionHeader',
          MessageSelectionCancel: 'SelectionCancel',
        };
      if (name.endsWith('/conversation-viewport'))
        return { ConversationContainer: 'Container', ConversationViewport: 'Viewport' };
      if (name.endsWith('/conversation-timeline-message'))
        return { ConversationTimelineMessage: 'Message' };
      if (name.endsWith('/use-message-actions'))
        return {
          useMessageActions: (args: typeof actionArgs) => {
            actionArgs = args;
            return actions;
          },
        };
      throw new Error(name);
    },
  });
  const tree = host.render(() => module.exports.default());
  const viewport = find(tree, 'Viewport')!;
  const list = (viewport.props.children as (spacer: unknown, scroll: unknown) => Element)(
    null,
    'Scroll',
  );
  return { calls, entries, selection, tree, viewport, list, hookArgs, actionArgs };
}

test('thread binds selection to its root and expands every original entry into toggle-only rows', () => {
  const f = fixture();
  assert.equal(f.hookArgs[0], 'space');
  assert.equal(f.hookArgs[1], f.entries);
  assert.equal(f.hookArgs[2], false);
  assert.equal(f.hookArgs[3], 'root-message');
  assert.equal(f.actionArgs!.disabled, true);
  assert.equal(f.actionArgs!.onSelect, f.selection.toggle);
  const rows = f.list.props.data as Array<{ entry: TimelineEntry; album?: unknown }>;
  assert.equal(rows.length, 2);
  assert.equal(rows[0]!.album, undefined);
  for (const [index, row] of rows.entries()) {
    const rendered = (f.list.props.renderItem as (value: unknown) => unknown)({ item: row, index });
    const message = find(rendered, 'Message')!;
    assert.equal(message.props.disabled, true);
    const control = message.props.selection as {
      selected: boolean;
      disabled: boolean;
      onToggle(entry: TimelineEntry): void;
    };
    assert.equal(control.selected, index === 0);
    assert.equal(control.disabled, index !== 0);
    control.onToggle(row.entry);
  }
  assert.deepEqual(f.calls, ['toggle:original', 'toggle:reply']);
});

test('thread selection replaces composition and exposes native count and Cancel without a back button', () => {
  const f = fixture();
  assert.equal(find(f.viewport.props.composer, 'Composer'), undefined);
  const toolbar = find(f.viewport.props.composer, 'SelectionToolbar')!;
  (toolbar.props.onCopy as () => void)();
  (toolbar.props.onForward as () => void)();
  const options = find(f.tree, 'Screen')!.props.options as {
    headerBackVisible: boolean;
    headerTitle(): Element;
    unstable_headerLeftItems(): unknown[];
    unstable_headerRightItems(): Array<{ onPress(): void }>;
  };
  assert.equal(options.headerBackVisible, false);
  assert.equal(options.headerTitle().props.count, 1);
  assert.equal(options.unstable_headerLeftItems().length, 0);
  options.unstable_headerRightItems()[0]!.onPress();
  assert.deepEqual(f.calls, ['copy', 'forward', 'cancel']);
});

test('ordinary thread keeps existing album projection, composer and message actions', () => {
  const f = fixture(false);
  assert.ok(find(f.viewport.props.composer, 'Composer'));
  assert.equal(find(f.viewport.props.composer, 'SelectionToolbar'), undefined);
  assert.equal(f.actionArgs!.disabled, false);
  const rows = f.list.props.data as Array<{ album?: unknown }>;
  assert.equal(rows.length, 1);
  assert.ok(rows[0]!.album);
  const options = find(f.tree, 'Screen')!.props.options as {
    title: string;
    headerBackVisible: boolean;
  };
  assert.equal(options.title, 'chat.replies');
  assert.equal(options.headerBackVisible, true);
});
