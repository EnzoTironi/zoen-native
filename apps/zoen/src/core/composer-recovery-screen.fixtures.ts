import * as conversationSearch from './conversation-search.ts';
import assert from 'node:assert/strict';
import { build } from 'esbuild';
import { runInNewContext } from 'node:vm';
import type { useMessageDraft } from './use-message-draft';
import { fixture as thread, find } from './thread-message-selection.fixtures.ts';

const source = await build({
  entryPoints: [new URL('../components/chat-screen.tsx', import.meta.url).pathname],
  bundle: true,
  external: ['*'],
  jsx: 'automatic',
  platform: 'node',
  format: 'cjs',
  write: false,
});
const views: Record<string, string[]> = {
  'conversation-hashtag-provider': ['ConversationHashtagProvider'],
  'quote-layout-context': ['QuoteMotionView'],
  'quote-layout-transition': ['QuoteLayoutTransition'],
  'flash-list': ['FlashList'],
  'react-native': ['Pressable', 'Text', 'View'],
  'message-selection-toolbar': ['MessageSelectionToolbar'],
  'chat-header': ['ChatHeader'],
  'chat-date-navigation': ['ChatDateNavigation'],
  'conversation-rail': ['ConversationRail', 'ConversationRailSpacer'],
  'conversation-timeline-message': ['ConversationTimelineMessage'],
  'conversation-viewport': ['ConversationContainer', 'ConversationViewport'],
  'message-composer': ['MessageComposer'],
  primitives: ['EmptyState', 'Loading'],
  rail: ['MiniappRail'],
};
const module = { exports: {} as { renderChatScreen(context: unknown): unknown } };
runInNewContext(source.outputFiles[0]!.text, {
  module,
  exports: module.exports,
  process: { env: { EXPO_OS: 'ios' } },
  require(name: string) {
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: string, props: unknown) => ({ type, props }),
        jsxs: (type: string, props: unknown) => ({ type, props }),
        Fragment: 'Fragment',
      };
    if (name.endsWith('/conversation-search')) return conversationSearch;
    if (name.endsWith('/presentation')) return { conversationTitle: () => 'Chat' };
    if (name.endsWith('/index.ts')) return { t: (key: string) => key };
    if (name === 'expo-router') return { router: {} };
    // View ports remain inert: inspect the actual screen's composer contract,
    // without mounting native geometry, media, or list implementations.
    if (
      name.endsWith('/access') ||
      name.endsWith('/message-presentation') ||
      name.endsWith('/launch')
    )
      return {};
    const key = name.split('/').at(-1)!;
    assert.ok(views[key], `Unexpected screen dependency: ${name}`);
    return Object.fromEntries(views[key]!.map((component) => [component, component]));
  },
});
export function composerProps(
  surface: 'chat' | 'thread',
  draft: ReturnType<typeof useMessageDraft>,
  searching = false,
) {
  if (surface === 'thread') {
    const screen = thread(false, undefined, false, async () => {}, draft);
    const composer = find(screen.viewport.props.composer, 'Composer');
    assert.ok(composer);
    return composer.props as any;
  }
  const tree = module.exports.renderChatScreen({
    chat: { id: 'chat', can_write: true },
    id: 'chat',
    selection: { active: false },
    searchState: { active: searching, query: 'search' },
    initialEnd: {},
    searchReveal: {},
    actions: {},
    colors: {},
    cards: {},
    conversationMiniapps: [],
    home: {},
    rail: {},
    items: [],
    rows: [],
    normalDraft: draft,
    editing: draft.messageDraft.editing,
    reply: draft.messageDraft.reply,
    updateDraft: draft.updateDraft,
    composerText: draft.messageDraft.editing
      ? draft.messageDraft.editText
      : draft.messageDraft.text,
  });
  const viewport = find(tree, 'ConversationViewport');
  assert.ok(viewport);
  const composer = find(viewport.props.composer, 'MessageComposer');
  assert.ok(composer);
  return composer.props as any;
}
