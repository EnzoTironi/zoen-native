import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import * as content from '../media/attachment-content.ts';
import * as categories from '../media/attachment-categories.ts';
import * as actions from '../media/attachment-route-actions.ts';
import {
  NormalMessageDrafts,
  type NormalDraftScope,
  type NormalDraftSnapshot,
} from './normal-message-drafts.ts';
import type { TimelineEntry } from './types.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const compile = (path: string, suffix = '') =>
  transformSync(readFileSync(new URL(path, import.meta.url), 'utf8') + suffix, {
    filename: new URL(path, import.meta.url).pathname,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [['babel-plugin-react-compiler', { target: '19' }]],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
  }).code as string;
const routeCode = compile('../app/chat-attachment/[id].tsx');
const editorCode = compile('../media/article-editor.tsx');
type Element = { type: unknown; props: Record<string, any> };
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
const jsx = {
  jsx: (type: unknown, props: object) => ({ type, props }),
  jsxs: (type: unknown, props: object) => ({ type, props }),
  Fragment: 'Fragment',
};
function hooks() {
  type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  const caches: unknown[][] = [];
  let cursor = 0,
    cacheCursor = 0;
  let effects: (() => void)[] = [];
  const next = () => (slots[cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    Boolean(a && b && a.length === b.length && a.every((x, index) => Object.is(x, b[index])));
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const slot = next();
    if (!same(slot.deps, deps)) {
      slot.deps = deps;
      slot.value = read();
    }
    return slot.value;
  };
  const effect = (read: () => void | (() => void), deps?: readonly unknown[]) => {
    const slot = next();
    if (same(slot.deps, deps)) return;
    slot.deps = deps;
    effects.push(() => {
      slot.cleanup?.();
      slot.cleanup = read() ?? undefined;
    });
  };
  const react = {
    useState: (initial: any) => {
      const slot = next();
      if (!('value' in slot)) slot.value = typeof initial === 'function' ? initial() : initial;
      return [
        slot.value,
        (value: any) => {
          slot.value = typeof value === 'function' ? value(slot.value) : value;
        },
      ];
    },
    useRef: (initial: unknown) => {
      const slot = next();
      return (slot.value ??= { current: initial });
    },
    useMemo: memo,
    useCallback: (read: unknown, deps: readonly unknown[]) => memo(() => read, deps),
    useEffect: effect,
    useLayoutEffect: effect,
    useSyncExternalStore: (_: unknown, read: () => unknown) => read(),
    useImperativeHandle: (ref: any, read: () => unknown, deps: readonly unknown[]) =>
      effect(() => {
        ref.current = read();
        return () => {
          ref.current = null;
        };
      }, deps),
  };
  return {
    react,
    effect,
    runtime: {
      c: (length: number) =>
        (caches[cacheCursor++] ??= Array(length).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    begin: () => {
      cursor = 0;
      cacheCursor = 0;
      effects = [];
    },
    commit: () => {
      for (const work of effects) work();
    },
    retire: () => {
      for (const slot of slots) slot.cleanup?.();
    },
  };
}
const tick = async () => {
  for (let index = 0; index < 35; index++) await Promise.resolve();
};
function fixture(kind = 'article') {
  const h = hooks();
  let backs = 0,
    sends = 0,
    polls = 0,
    geoReads = 0,
    contactReads = 0,
    captured = 0;
  let owner = { mode: 'account', identityId: 'synthetic-owner', deviceId: 'synthetic-device' };
  let phase = 'ready';
  let member = true;
  let foregroundListener: ((state: string) => void) | undefined;
  let focusCleanup: (() => void) | undefined;
  let stored: NormalDraftSnapshot = {
    revision: '0',
    text: '**old draft**',
    reply_to: 'synthetic-reply',
  };
  let revision = 0;
  let nativeText = stored.text;
  let pollError = false;
  const scope: NormalDraftScope = {
    owner: { ...owner, mode: 'account', sessionEpoch: 7 },
    spaceId: 'synthetic-space',
    threadRootId: null,
  };
  const controller = new NormalMessageDrafts({
    load: async () => ({ ...stored }),
    save: async (_, expected, text, reply) => {
      assert.equal(expected, stored.revision);
      stored = { revision: String(++revision), text, reply_to: reply };
      return { ...stored };
    },
    send: async (_, expected) => {
      assert.equal(expected, stored.revision);
      sends++;
      stored = { revision: String(++revision), text: '', reply_to: null };
      return { entry: { id: 'synthetic-message' } as TimelineEntry, draft: { ...stored } };
    },
  });
  const channel = { scope, controller };
  const state = () => ({ phase, owner });
  const store = {
    sessionEpoch: 7,
    isSessionRetiring: false,
    getSnapshot: state,
    messageDrafts: {
      channel: () => channel,
      isCurrent: (value: unknown) =>
        value === channel && owner.identityId === scope.owner.identityId,
    },
    mediaDrafts: { channel: () => null },
    refresh: async () => {},
    request: async (request: any) => {
      assert.equal(request.op, 'install_app');
      assert.equal(request.space_id, scope.spaceId);
      assert.equal(request.app_id, 'poll');
      polls++;
      if (pollError) throw new Error('synthetic unconfirmed effect');
      return { id: 'synthetic-poll' };
    },
  };
  const module: { exports: any } = { exports: {} };
  let guard: { enabled: boolean; action: (value: any) => void } | undefined;
  runInNewContext(routeCode, {
    module,
    exports: module.exports,
    TextEncoder,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return h.react;
      if (name === 'react/compiler-runtime') return h.runtime;
      if (name === 'react/jsx-runtime') return jsx;
      if (name === 'react-native')
        return {
          Text: 'Text',
          View: 'View',
          ScrollView: 'ScrollView',
          Linking: { openURL: async () => {} },
          AppState: {
            currentState: 'active',
            addEventListener: (_: string, listener: typeof foregroundListener) => {
              foregroundListener = listener;
              return { remove() {} };
            },
          },
        };
      if (name === 'expo-router')
        return {
          Stack: { Screen: 'StackScreen' },
          router: {
            back: () => {
              backs++;
            },
          },
          useLocalSearchParams: () => ({}),
          useFocusEffect: (read: () => void | (() => void)) =>
            h.effect(() => {
              focusCleanup = read() ?? undefined;
              return focusCleanup;
            }, []),
          usePreventRemove: (enabled: boolean, action: (value: any) => void) => {
            guard = { enabled, action };
            return () => {
              guard!.enabled = false;
            };
          },
        };
      if (name === 'expo-crypto') return { randomUUID: () => 'synthetic-id' };
      if (name === '../../components/dialogs')
        return { useDialogs: () => ({ confirm: async () => true }) };
      if (name === '../../core/provider')
        return { useClient: () => ({ ...state(), snapshot: { synthetic: true }, store }) };
      if (name === '../../home/access')
        return {
          homeOwnerFor: (value: any) => value.owner,
          homeSpacesFor: () => new Set(member ? [scope.spaceId] : []),
        };
      if (name === '../../i18n') return { t: (key: string) => key };
      if (name === '../../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../../components/primitives')
        return { Field: 'Field', Loading: 'Loading', NativeButton: 'NativeButton' };
      if (name === '../../media/attachment-categories') return categories;
      if (name === '../../media/attachment-content') return content;
      if (name === '../../media/attachment-route-actions') return actions;
      if (name === '../../media/article-editor') return { ArticleEditor: 'ArticleEditor' };
      if (name === '../../media/attachment-route-platform')
        return {
          chooseStaticLocation: async () => {
            geoReads++;
            return content.staticLocation(1, 2);
          },
          chooseSharedContact: async () => {
            contactReads++;
            return content.sharedContact('Synthetic', ['+000']);
          },
          prepareContactFile: async () => {
            throw Error('not used');
          },
        };
      if (name === '../../media/cache-work')
        return { sensitiveCacheWork: { run: (work: () => unknown) => work() } };
      throw new Error(`Unexpected route dependency ${name}`);
    },
  });
  const target = {
    id: scope.spaceId,
    thread: null,
    identity: owner.identityId,
    device: owner.deviceId,
    epoch: '7',
  };
  function render() {
    h.begin();
    const tree = module.exports.AttachmentRoute({ kind, destination: target }) as Element;
    h.commit();
    const editor = nodes(tree).find((node) => node.type === 'ArticleEditor');
    if (editor)
      editor.props.ref.current = {
        capture: async () => {
          captured++;
          return nativeText;
        },
      };
    return tree;
  }
  const button = (tree: Element, label: string): Element => {
    const candidates = nodes(tree);
    const stack = candidates.find((node) => node.type === 'StackScreen');
    const headers = [
      stack?.props.options.headerLeft?.(),
      stack?.props.options.headerRight?.(),
    ].filter(Boolean);
    const result = [...candidates, ...headers].find((node) => node.props.label === label);
    assert.ok(result, label);
    return result;
  };
  return {
    render,
    button,
    counts: () => ({ backs, sends, polls, geoReads, contactReads, captured }),
    read: () => stored,
    native: (text: string) => {
      nativeText = text;
    },
    tick,
    guard: () => guard!,
    failPoll: () => {
      pollError = true;
    },
    blur: () => focusCleanup?.(),
    background: () => foregroundListener?.('background'),
    revoke: () => {
      member = false;
    },
    replaceOwner: () => {
      owner = { ...owner, identityId: 'different-owner' };
    },
    phase: (value: string) => {
      phase = value;
    },
    retire: () => h.retire(),
    channel,
  };
}

test('production-compiled Article route survives first unloaded render and sends only explicit final native capture', async () => {
  const f = fixture();
  f.render();
  await f.tick();
  let tree = f.render();
  assert.equal(f.counts().sends, 0);
  const editor = nodes(tree).find((node) => node.type === 'ArticleEditor')!;
  editor.props.onChange('**typed** 👩‍💻\n');
  await f.tick();
  tree = f.render();
  assert.equal(f.read().text, '**typed** 👩‍💻\n');
  assert.equal(f.counts().sends, 0);
  f.native('**final native**  \n');
  const send = f.button(tree, 'attachmentTabs.send');
  assert.equal(send.props.disabled, false);
  send.props.onPress();
  send.props.onPress();
  await f.tick();
  assert.equal(f.counts().captured, 1);
  assert.equal(f.counts().sends, 1);
  assert.equal(f.counts().backs, 1);
  editor.props.onChange('late native input after accepted Send');
  await f.tick();
  assert.equal(f.read().text, '');
});

test('compiled Article guard flushes exact final input on Back without sending, and unavailable route never traps dismissal', async () => {
  const f = fixture();
  f.render();
  await f.tick();
  f.render();
  f.native('unsent **final**');
  let repeated = 0;
  assert.equal(f.guard().enabled, true);
  f.guard().action({
    repeat: () => {
      repeated++;
    },
  });
  await f.tick();
  assert.equal(f.read().text, 'unsent **final**');
  assert.equal(f.counts().sends, 0);
  assert.equal(repeated, 1);
  const revoked = fixture();
  revoked.render();
  await revoked.tick();
  revoked.revoke();
  const tree = revoked.render();
  assert.equal(revoked.guard().enabled, false);
  revoked.button(tree, 'attachmentTabs.close').props.onPress();
  assert.equal(revoked.counts().backs, 1);
});

test('retained compiled handlers refuse blur, background, owner replacement and membership loss', async () => {
  for (const retire of ['blur', 'background', 'replaceOwner', 'revoke'] as const) {
    const f = fixture();
    f.render();
    await f.tick();
    const tree = f.render();
    const editor = nodes(tree).find((node) => node.type === 'ArticleEditor')!;
    const send = f.button(tree, 'attachmentTabs.send');
    f[retire]();
    editor.props.onChange('stale input');
    send.props.onPress();
    await f.tick();
    assert.equal(f.counts().sends, 0, retire);
    assert.equal(f.read().text, '**old draft**', retire);
  }
});

test('compiled mode toggle captures once, never sends, and fences a retained old editor callback', async () => {
  const f = fixture();
  f.render();
  await f.tick();
  let tree = f.render();
  const old = nodes(tree).find((node) => node.type === 'ArticleEditor')!;
  f.native('**exact markdown**');
  f.button(tree, 'attachmentTabs.source').props.onPress();
  await f.tick();
  tree = f.render();
  old.props.onChange('late old editor');
  await f.tick();
  assert.equal(f.read().text, '**exact markdown**');
  assert.equal(f.counts().sends, 0);
  assert.equal(nodes(tree).find((node) => node.type === 'ArticleEditor')!.props.source, true);
});

test('compiled Location requires explicit preview selection and adds one link to the same draft without sending', async () => {
  const f = fixture('location');
  f.render();
  await f.tick();
  let tree = f.render();
  assert.equal(f.counts().geoReads, 0);
  f.button(tree, 'attachmentTabs.chooseLocation').props.onPress();
  await f.tick();
  tree = f.render();
  assert.equal(f.counts().geoReads, 1);
  assert.equal(f.read().text, '**old draft**');
  const add = f.button(tree, 'attachmentTabs.addToMessage');
  add.props.onPress();
  add.props.onPress();
  await f.tick();
  assert.equal(f.read().text, '**old draft**\n\nhttps://maps.apple.com/?ll=1,2');
  assert.equal(f.counts().sends, 0);
});

test('compiled Poll validation never creates on render/typing; explicit unconfirmed result is held, not resent', async () => {
  const f = fixture('poll');
  f.render();
  await f.tick();
  let tree = f.render();
  const fields = nodes(tree).filter((node) => node.type === 'Field');
  fields[0]!.props.onChangeText('Question?');
  fields[1]!.props.onChangeText('A');
  tree = f.render();
  nodes(tree)
    .filter((node) => node.type === 'Field')[2]!
    .props.onChangeText('B');
  tree = f.render();
  assert.equal(f.counts().polls, 0);
  f.failPoll();
  const create = f.button(tree, 'attachmentTabs.createPoll');
  create.props.onPress();
  await f.tick();
  tree = f.render();
  assert.equal(f.counts().polls, 1);
  assert.equal(f.button(tree, 'attachmentTabs.createPoll').props.disabled, true);
  create.props.onPress();
  await f.tick();
  assert.equal(f.counts().polls, 1);
  assert.equal(f.counts().backs, 0);
});

test('actual native Article editor formatting handlers use installed native methods and do not capture/send by rendering', async () => {
  const h = hooks();
  const calls: string[] = [];
  const handle = { current: null as any };
  const module: { exports: any } = { exports: {} };
  let editable = true;
  runInNewContext(editorCode, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return h.react;
      if (name === 'react/compiler-runtime') return h.runtime;
      if (name === 'react/jsx-runtime') return jsx;
      if (name === 'react-native')
        return { View: 'View', ScrollView: 'ScrollView', TextInput: 'TextInput' };
      if (name === 'react-native-enriched-markdown')
        return { EnrichedMarkdownTextInput: 'NativeMarkdown' };
      if (name === '../components/primitives')
        return { IconButton: 'IconButton', NativeButton: 'NativeButton', Field: 'Field' };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === './attachment-content') return content;
      throw Error(name);
    },
  });
  h.begin();
  const tree = module.exports.ArticleEditor({
    initial: '**initial**',
    disabled: false,
    source: false,
    onChange: () => calls.push('change'),
    canEdit: () => editable,
    ref: handle,
  }) as Element;
  h.commit();
  const native = nodes(tree).find((node) => node.type === 'NativeMarkdown')!;
  native.props.ref.current = {
    toggleBold: () => calls.push('bold'),
    toggleItalic: () => calls.push('italic'),
    toggleStrikethrough: () => calls.push('strike'),
    toggleHeading: () => calls.push('heading'),
    toggleUnorderedList: () => calls.push('bullet'),
    toggleOrderedList: () => calls.push('numbered'),
    getMarkdown: async () => {
      calls.push('capture');
      return '**native**';
    },
  };
  assert.deepEqual(calls, []);
  const buttons = nodes(tree).filter((node) => node.type === 'IconButton');
  for (const button of buttons.slice(0, 6)) button.props.onPress();
  assert.deepEqual(calls, ['bold', 'italic', 'strike', 'heading', 'bullet', 'numbered']);
  assert.equal(await handle.current.capture(), '**native**');
  editable = false;
  buttons[0]!.props.onPress();
  assert.equal(calls.length, 7);
});
