import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as content from '../media/attachment-content.ts';
import * as categories from '../media/attachment-categories.ts';
import { LocationSelection } from '../media/location-selection.ts';
import { MediaDraftChannel } from '../media/media-draft-channel.ts';
import type { MediaDraftDto, MediaDraftPort } from '../media/media-draft-types.ts';
import type { StaticLocation } from '../media/types.ts';
import * as actions from '../media/attachment-route-actions.ts';
import * as mediaReply from '../media/media-reply.ts';
import type { MediaReplyTarget } from '../media/media-reply.ts';
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
const routeCode = compile(
  process.env.ZOEN_ATTACHMENT_ROUTE_SOURCE ?? '../media/attachment-route.tsx',
);
const locationPickerCode = compile('../media/location-picker.tsx');
const pollFieldsCode = compile('../media/poll-attachment-fields.tsx');
export const editorCode = compile('../media/article-editor.tsx');
export type Element = { type: unknown; props: Record<string, any> };
export function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
const renderElement = (type: unknown, props: object): unknown =>
  typeof type === 'function' ? type(props) : { type, props };
export const jsx = {
  jsx: renderElement,
  jsxs: renderElement,
  Fragment: 'Fragment',
};
export function hooks() {
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
export function fixture(kind = 'article') {
  const h = hooks();
  let backs = 0,
    sends = 0,
    polls = 0,
    geoReads = 0,
    contactReads = 0,
    captured = 0,
    reveals = 0;
  let owner = { mode: 'account', identityId: 'synthetic-owner', deviceId: 'synthetic-device' };
  let phase = 'ready';
  let member = true;
  const foregroundListeners = new Set<(state: string) => void>();
  const focusCleanups = new Set<() => void>();
  let uuid = 0;
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
    replyIntent: () => `reply_${++uuid}`,
    load: async () => ({ ...stored }),
    save: async (_, expected, text, reply, reply_quote, shared_reply, edit, reply_intent) => {
      assert.equal(expected, stored.revision);
      stored = {
        revision: String(++revision),
        text,
        reply_to: reply,
        reply_quote,
        shared_reply,
        edit,
        reply_intent,
      };
      return { ...stored };
    },
    send: async (_, expected) => {
      assert.equal(expected, stored.revision);
      sends++;
      stored = { revision: String(++revision), text: '', reply_to: null };
      return {
        message_id: 'synthetic-message',
        text_revision: null,
        entry: { id: 'synthetic-message' } as TimelineEntry,
        draft: { ...stored },
      };
    },
  });
  const channel = { scope, controller };
  const state = () => ({ phase, owner });
  let mediaStored: MediaDraftDto = {
    revision: '0',
    content_json: 'null',
    assets: [],
    attempted: false,
  };
  const prepared: string[] = [];
  const searches: string[] = [];
  let gpsGate: Promise<StaticLocation | null> | null = null;
  let contactGate: Promise<content.SharedContact | null> | null = null;
  let searchGate: Promise<readonly StaticLocation[]> | null = null;
  const mediaPort: MediaDraftPort = {
    load: async () => mediaStored,
    save: async (_, expected, content, assets) => {
      assert.equal(expected, mediaStored.revision);
      mediaStored = {
        revision: String(Number(expected) + 1),
        content_json: content,
        assets: assets.map((a) => ({
          id: a.id,
          name: a.name,
          mime: a.mime,
          size: 120,
          sha256: 'a'.repeat(64),
        })),
        attempted: false,
      };
      return { accepted_revision: mediaStored.revision, draft: mediaStored };
    },
    materialize: async () => {
      throw Error('not used');
    },
    release: async () => {},
    send: async () => {
      sends++;
      throw Error('picker must not send');
    },
  };
  const mediaChannel = new MediaDraftChannel(
    {
      expected_identity_id: scope.owner.identityId,
      expected_device_id: scope.owner.deviceId,
      space_id: scope.spaceId,
      thread_root_id: null,
    },
    mediaPort,
  );
  const mediaDriver = {
    readLocation: async () => {
      throw Error('not a received file');
    },
    searchLocations: async (_binding: string, query: string) => {
      searches.push(query);
      return searchGate ?? [{ longitude: 4, latitude: 3, label: 'Chosen place' }];
    },
    cancelLocationSearch: () => {},
  };
  const store = {
    sessionEpoch: 7,
    driver: { media: mediaDriver },
    isSessionRetiring: false,
    getSnapshot: state,
    messageDrafts: {
      channel: () => channel,
      isCurrent: (value: unknown) =>
        value === channel && owner.identityId === scope.owner.identityId,
    },
    mediaDrafts: {
      channel: () => mediaChannel,
      isCurrent: (value: unknown) =>
        value === mediaChannel && owner.identityId === scope.owner.identityId,
    },
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
  const routeDependency = (name: string): any => {
    if (name.startsWith('./')) name = `../../media/${name.slice(2)}`;
    else if (name.startsWith('../')) name = `../${name}`;
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
          addEventListener: (_: string, listener: (state: string) => void) => {
            foregroundListeners.add(listener);
            return {
              remove() {
                foregroundListeners.delete(listener);
              },
            };
          },
        },
      };
    if (name === 'react-native-keyboard-controller')
      return { KeyboardAwareScrollView: 'KeyboardAwareScrollView' };
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
            const cleanup = read() ?? (() => {});
            focusCleanups.add(cleanup);
            return () => {
              focusCleanups.delete(cleanup);
              cleanup();
            };
          }, []),
        usePreventRemove: (enabled: boolean, action: (value: any) => void) => {
          guard = { enabled, action };
          return () => {
            guard!.enabled = false;
          };
        },
      };
    if (name === 'expo-crypto') return { randomUUID: () => `synthetic-id-${++uuid}` };
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
    if (name === '../../media/media-reply') return mediaReply;
    if (name === '../../media/location-map') return { LocationMap: 'LocationMap' };
    if (name === '../../media/location-selection') return { LocationSelection };
    if (name === '../../media/location-picker') {
      const picker = { exports: {} };
      runInNewContext(locationPickerCode, {
        module: picker,
        exports: picker.exports,
        require: routeDependency,
      });
      return picker.exports;
    }
    if (name === '../../media/article-editor') return { ArticleEditor: 'ArticleEditor' };
    if (name === '../../media/attachment-route-platform')
      return {
        chooseStaticLocation: async () => {
          geoReads++;
          return content.staticLocation(1, 2);
        },
        chooseStaticPoint: async () => {
          geoReads++;
          return gpsGate ?? { latitude: 1, longitude: 2 };
        },
        chooseSharedContact: async () => {
          contactReads++;
          return contactGate ?? content.sharedContact('Synthetic', ['+000']);
        },
        prepareLocationFile: async (text: string) => {
          prepared.push(text);
          return { uri: 'file:///location.geojson', release: async () => {} };
        },
        prepareContactFile: async (text: string) => {
          prepared.push(text);
          return { uri: 'file:///contact.vcf', release: async () => {} };
        },
      };
    if (name === '../../media/cache-work')
      return { sensitiveCacheWork: { run: (work: () => unknown) => work() } };
    if (name === '../../media/poll-attachment-fields') {
      const fields = { exports: {} };
      runInNewContext(pollFieldsCode, {
        module: fields,
        exports: fields.exports,
        require: routeDependency,
      });
      return fields.exports;
    }
    throw new Error(`Unexpected route dependency ${name}`);
  };
  runInNewContext(routeCode, {
    module,
    exports: module.exports,
    TextEncoder,
    require: routeDependency,
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
    if (tree.props.ref)
      tree.props.ref.current = {
        assureFocusedInputVisible: () => {
          reveals++;
        },
      };
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
    const result = [...candidates, ...headers].find(
      (node) => node.type === 'NativeButton' && node.props.label === label,
    );
    assert.ok(result, label);
    return result;
  };
  return {
    render: () => {
      if (kind === 'location') render();
      return render();
    },
    button,
    counts: () => ({ backs, sends, polls, geoReads, contactReads, captured, reveals }),
    read: () => stored,
    readMedia: () => mediaStored,
    prepared,
    searches,
    mediaChannel,
    chooseReply: (reply: MediaReplyTarget) => {
      if (reply.shared_reply) controller.setSharedReply(scope, reply.shared_reply);
      else controller.setReply(scope, reply.reply_to, reply.reply_quote ?? null);
    },
    contactGate: (value: Promise<content.SharedContact | null>) => {
      contactGate = value;
    },
    gpsGate: (gate: Promise<StaticLocation | null>) => {
      gpsGate = gate;
    },
    searchGate: (gate: Promise<readonly StaticLocation[]>) => {
      searchGate = gate;
    },
    native: (text: string) => {
      nativeText = text;
    },
    tick,
    guard: () => guard!,
    failPoll: () => {
      pollError = true;
    },
    blur: () => {
      for (const cleanup of focusCleanups) cleanup();
    },
    background: () => {
      for (const listener of foregroundListeners) listener('background');
    },
    foreground: () => {
      for (const listener of foregroundListeners) listener('active');
    },
    revoke: () => {
      member = false;
    },
    replaceMedia: () => {
      store.driver.media = { ...mediaDriver };
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
