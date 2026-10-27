import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import type { Command, PageDto } from '../core/types.ts';
import type { PageBlockDto } from '../core/generated/PageBlockDto.ts';
import type { PageEditResult } from '../core/generated/PageEditResult.ts';
import type { ClientState } from '../core/store.ts';
import { documentScope } from './document-authority.ts';
import { homeSpacesFor } from '../home/access.ts';
import * as duplicate from './page-duplicate.ts';
import { hooks, nodes, type Element } from '../core/attachment-route-ui.fixtures.ts';
export { nodes, type Element } from '../core/attachment-route-ui.fixtures.ts';
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const path = new URL('../app/page/[id].tsx', import.meta.url);
const compiled = transformSync(readFileSync(path, 'utf8'), {
  filename: path.pathname,
  configFile: false,
  babelrc: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code;
export const tick = async () => {
  for (let i = 0; i < 40; i++) await Promise.resolve();
};
export function originalBlock(id = 'original'): PageBlockDto {
  return {
    id,
    kind: 'paragraph',
    text: 'Keep 👩‍💻 and all marks',
    spans: [{ start: 0, end: 4, key: 'future', value: 'opaque' }],
    level: 0,
    indent: 0,
    number: 1,
    checked: false,
    lang: '',
    url: '',
    alt: '',
  };
}
export function fixture() {
  let h = hooks();
  const wrapperHooks = hooks();
  let activeHooks = h;
  let mountedKey: string | undefined;
  const react = Object.fromEntries(
    Object.keys(h.react).map((name) => [
      name,
      (...args: unknown[]) =>
        (activeHooks.react as Record<string, (...values: unknown[]) => unknown>)[name]!(...args),
    ]),
  );
  const runtime = { c: (size: number) => activeHooks.runtime.c(size) };
  let state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: { identity_id: 'owner', device_id: 'device', unlocked: true, setup_pending: false },
      me: { id: 'owner' },
      conversations: [{ id: 'space', members: [{ id: 'owner' }] }],
    },
  } as unknown as ClientState;
  let page: PageDto = {
    item_id: 'page',
    space_id: 'space',
    title: 'Canonical Page',
    path: 'page.md',
    version: 3,
    ready: true,
    unsaved: false,
    input_pending: false,
    input_revision: '0',
    pending_sync: false,
    save_error: null,
    can_edit: true,
    content_hash: 'synthetic-hash',
    edit_context: 'observed-context',
    blocks: [originalBlock('before'), originalBlock(), originalBlock('after')],
  };
  const commands: Command[] = [],
    pushes: unknown[] = [],
    exports: string[] = [],
    keys: string[] = [];
  const receipts = new Map<string, PageEditResult>();
  let ids = 0,
    effects = 0,
    refreshes = 0,
    lostAck = false;
  let idGate: Promise<string> | null = null;
  let applyGate: Promise<void> | null = null;
  let confirmation: Promise<boolean> | null = null;
  let confirmations = 0;
  let appListener: ((state: string) => void) | undefined;
  let focusRead: (() => void | (() => void)) | undefined, focusCleanup: (() => void) | undefined;
  const driver = {
    pageInput: {},
    request: async (command: Command) => {
      if (command.op === 'new_mutation_id') {
        ids++;
        return idGate ?? `client_edit_${ids}`;
      }
      if (command.op === 'page_markdown') return 'actual read-port Markdown';
      throw Error(`Unexpected read port ${command.op}`);
    },
  };
  const store = {
    driver,
    sessionEpoch: 1,
    isSessionRetiring: false,
    getSnapshot: () => state,
    request: async (command: Command) => {
      commands.push(structuredClone(command));
      if (command.op !== 'page_apply') throw Error(`Unexpected mutation ${command.op}`);
      if (applyGate) await applyGate;
      const prior = receipts.get(command.mutation_id);
      if (prior) return { ...prior, replayed: true, page };
      // Explicit deterministic Core port. This checks frontend intent/recovery,
      // not Loro merge or native storage; those contracts stay in Rust tests.
      const current = new Map(page.blocks.map((block) => [block.id, block]));
      for (const block of command.changed) current.set(block.id, structuredClone(block));
      page = {
        ...page,
        unsaved: true,
        edit_context: 'merged-context',
        blocks: command.order.map((id) => current.get(id)!).filter(Boolean),
      };
      effects++;
      const receipt: PageEditResult = {
        mutation_hash: 'synthetic-request-hash',
        mutation_id: command.mutation_id,
        applied_content_hash: 'synthetic-applied-hash',
        replayed: false,
        applied_edit_context: 'accepted-own-context',
        page,
      };
      receipts.set(command.mutation_id, receipt);
      if (lostAck) {
        lostAck = false;
        throw Error('synthetic lost response after acceptance');
      }
      return receipt;
    },
  };
  const jsx = (type: any, props: Record<string, any>, key?: string) => ({ type, props, key });
  const module = { exports: {} as any };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      if (name === 'react') return react;
      if (name === 'react/compiler-runtime') return runtime;
      if (name === 'react/jsx-runtime') return { jsx, jsxs: jsx };
      if (name === 'react-native')
        return {
          Text: 'Text',
          View: 'View',
          ScrollView: 'ScrollView',
          AppState: {
            currentState: 'active',
            addEventListener: (_: string, read: typeof appListener) => {
              appListener = read;
              return {
                remove: () => {
                  appListener = undefined;
                },
              };
            },
          },
        };
      if (name === 'expo-router')
        return {
          Stack: { Screen: 'StackScreen' },
          router: { push: (route: unknown) => pushes.push(route) },
          useLocalSearchParams: () => ({ id: 'page' }),
          useFocusEffect: (read: () => void | (() => void)) => {
            h.react.useEffect(() => {
              focusRead = read;
              focusCleanup = read() ?? undefined;
              return focusCleanup;
            }, [read]);
          },
        };
      if (name === '../../core/provider') return { useClient: () => ({ ...state, store }) };
      if (name === '../../core/query')
        return { useCoreQuery: () => ({ data: page, error: null, retry: () => refreshes++ }) };
      if (name === '../../home/access') return { homeSpacesFor };
      if (name === '../../documents/document-authority') return { documentScope };
      if (name === '../../documents/page-duplicate') return duplicate;
      if (name === '../../documents/page-blocks') return { PageBlocks: 'PageBlocks' };
      if (name === '../../documents/editing')
        return {
          moveBlock: () => {
            throw Error('not used');
          },
        };
      if (name === '../../documents/markdown-files')
        return {
          exportMarkdown: async (_: string, text: string, current: () => boolean) => {
            if (current()) exports.push(text);
          },
        };
      if (name === '../../documents/use-dirty-editor') return { useDirtyEditor: () => {} };
      if (name === '../../documents/artifact-discussion')
        return { ArtifactDiscussion: 'ArtifactDiscussion' };
      if (name === '../../components/primitives')
        return {
          EmptyState: 'EmptyState',
          Loading: 'Loading',
          NativeButton: 'NativeButton',
          Surface: 'Surface',
        };
      if (name === '../../components/dialogs')
        return {
          useDialogs: () => ({
            confirm: async () => {
              confirmations++;
              return confirmation ?? true;
            },
          }),
        };
      if (name === '../../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../../i18n/index.ts') return { t: (key: string) => key };
      throw Error(name);
    },
  });
  function render() {
    // React Compiler may cache the keyed child element. Its own state still
    // renders independently; do not cache the child's rendered tree in JSX.
    activeHooks = wrapperHooks;
    wrapperHooks.begin();
    const child = module.exports.default();
    wrapperHooks.commit();
    keys.push(child.key);
    if (mountedKey !== undefined && mountedKey !== child.key) {
      h.retire();
      h = hooks();
    }
    mountedKey = child.key;
    activeHooks = h;
    h.begin();
    const tree = child.type(child.props) as Element;
    h.commit();
    return tree;
  }
  const blocks = (tree: Element) => nodes(tree).find((node) => node.type === 'PageBlocks')!;
  const button = (tree: Element, label: string) => {
    const node = nodes(tree).find((value) => value.props.label === label);
    assertButton(node, label);
    return node;
  };
  return {
    render,
    blocks,
    button,
    commands,
    pushes,
    exports,
    keys,
    read: () => page,
    confirmation: (gate: Promise<boolean>) => {
      confirmation = gate;
    },
    confirmationCount: () => confirmations,
    counts: () => ({ ids, effects, refreshes }),
    lostAck: () => {
      lostAck = true;
    },
    idGate: (gate: Promise<string> | null) => {
      idGate = gate;
    },
    applyGate: (gate: Promise<void> | null) => {
      applyGate = gate;
    },
    readonly: () => {
      page = { ...page, can_edit: false };
    },
    retainedInput: () => {
      page = { ...page, input_pending: true };
    },
    owner: () => {
      const before = state.snapshot;
      if (!before?.me || !before.account) throw Error('fixture owner missing');
      state = {
        ...state,
        snapshot: {
          ...before,
          account: { ...before.account, identity_id: 'replacement' },
          me: { ...before.me, id: 'replacement' },
        },
      };
    },
    revoke: () => {
      state = { ...state, snapshot: { ...state.snapshot!, conversations: [] } };
    },
    background: () => appListener?.('background'),
    foreground: () => appListener?.('active'),
    blur: () => focusCleanup?.(),
    focus: () => {
      focusCleanup = focusRead?.() ?? undefined;
    },
    retire: () => h.retire(),
  };
}
function assertButton(value: Element | undefined, label: string): asserts value is Element {
  if (!value) throw Error(`Missing actual Page button ${label}`);
}
