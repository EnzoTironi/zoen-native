import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import type { ClientState } from '../core/store.ts';
import type { ItemDiscussionDto } from '../core/generated/ItemDiscussionDto.ts';
import type { TimelineEntry } from '../core/types.ts';
import { hooks, nodes, type Element } from '../core/attachment-route-ui.fixtures.ts';
import { homeSpacesFor } from '../home/access.ts';
import * as discussion from './discussion.ts';
export { nodes } from '../core/attachment-route-ui.fixtures.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const path = new URL('./artifact-discussion.tsx', import.meta.url);
const compiled = transformSync(readFileSync(path, 'utf8'), {
  filename: path.pathname,
  configFile: false,
  babelrc: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code;

/** Actual component with explicit read/navigation ports; no native/Core runtime claim. */
export function fixture() {
  const wrapper = hooks();
  let child = hooks(),
    active = wrapper,
    key: string | undefined;
  const react = Object.fromEntries(
    Object.keys(wrapper.react).map((name) => [
      name,
      (...args: unknown[]) =>
        (active.react as Record<string, (...values: unknown[]) => unknown>)[name]!(...args),
    ]),
  );
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'owner', device_id: 'phone', unlocked: true, setup_pending: false },
      me: { id: 'owner' },
      conversations: [{ id: 'space', members: [{ id: 'owner' }] }],
    },
  } as unknown as ClientState;
  const store = { sessionEpoch: 1, isSessionRetiring: false, getSnapshot: () => state };
  let data: ItemDiscussionDto | null = {
    item_id: 'page',
    space_id: 'space',
    root: { id: 'original-root', in_thread: null, thread_replies: 2 } as TimelineEntry,
  };
  let error: string | null = null,
    retries = 0;
  const reads: unknown[] = [],
    pushes: unknown[] = [];
  let focusRead: (() => void | (() => void)) | undefined, focusCleanup: (() => void) | undefined;
  let appListener: ((state: string) => void) | undefined;
  const app = {
    currentState: 'active',
    addEventListener: (_: string, listener: typeof appListener) => {
      appListener = listener;
      return {
        remove: () => {
          appListener = undefined;
        },
      };
    },
  };
  const module = { exports: {} as any };
  const jsx = (type: unknown, props: unknown, key?: string) => ({ type, props, key });
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return react;
      if (name === 'react/compiler-runtime') return { c: (size: number) => active.runtime.c(size) };
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native') return { Text: 'Text', View: 'View', AppState: app };
      if (name === 'expo-router')
        return {
          router: { push: (target: unknown) => pushes.push(structuredClone(target)) },
          useFocusEffect: (read: () => void | (() => void)) =>
            active.react.useEffect(() => {
              focusRead = read;
              focusCleanup = read() ?? undefined;
              return focusCleanup;
            }, [read]),
        };
      if (name === '../components/primitives') return { NativeButton: 'NativeButton' };
      if (name === '../core/provider') return { useClient: () => ({ store, ...state }) };
      if (name === '../core/query')
        return {
          useCoreQuery: (command: unknown) => {
            reads.push(structuredClone(command));
            return { data, error, retry: () => retries++ };
          },
        };
      if (name === '../home/access') return { homeSpacesFor };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../i18n/index')
        return {
          t: (label: string, params?: { count: number }) =>
            params ? `${label}:${params.count}` : label,
        };
      if (name === './discussion') return discussion;
      throw Error(name);
    },
  });
  function renderOnce() {
    active = wrapper;
    wrapper.begin();
    const element = module.exports.ArtifactDiscussion({ itemId: 'page', spaceId: 'space' });
    wrapper.commit();
    if (!element) return null;
    if (key !== undefined && key !== element.key) {
      child.retire();
      child = hooks();
    }
    key = element.key;
    active = child;
    child.begin();
    const tree = element.type(element.props) as Element;
    child.commit();
    return tree;
  }
  return {
    render: () => {
      renderOnce();
      return renderOnce();
    },
    state,
    store,
    reads,
    pushes,
    button: (tree: unknown) => nodes(tree).find((node) => node.type === 'NativeButton')!,
    data: (next: ItemDiscussionDto | null) => {
      data = next;
    },
    error: (next: string | null) => {
      error = next;
    },
    retries: () => retries,
    blur: () => focusCleanup?.(),
    focus: () => {
      focusCleanup = focusRead?.() ?? undefined;
    },
    background: () => {
      app.currentState = 'background';
      appListener?.('background');
    },
    foreground: () => {
      app.currentState = 'active';
      appListener?.('active');
    },
    retire: () => {
      child.retire();
      wrapper.retire();
    },
  };
}
