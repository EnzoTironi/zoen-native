import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { telegramPresentation } from '../theme/telegram.ts';
import { messageText } from './presentation.ts';
import type { ClientState } from './store.ts';
import type { TimelineEntry } from './types.ts';
import type { ReplySelection } from './generated/ReplySelection.ts';
import type { AttachmentDraftScope } from '../media/attachment-draft.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const path = fileURLToPath(new URL('../media/attachment-reply-context.tsx', import.meta.url));
const compiled = transformSync(readFileSync(path, 'utf8'), {
  filename: path,
  configFile: false,
  babelrc: false,
  presets: [require.resolve('babel-preset-expo')],
  caller: {
    name: 'metro',
    platform: 'ios',
    engine: 'hermes',
    isDev: false,
    supportsReactCompiler: true,
  },
}).code;
const external = new Set([
  'react/compiler-runtime',
  'react/jsx-runtime',
  'react-native',
  'react-native-reanimated',
  '../components/primitives',
  './primitives',
  './use-semantic-message-preview',
  '../core/provider',
  '../core/query',
  '../core/presentation',
  '../theme',
  '../i18n/index.ts',
]);
const output = await build({
  stdin: {
    contents: compiled + "\nObject.assign(exports, require('../components/message-context'));",
    resolveDir: dirname(path),
    loader: 'js',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'attachment-reply-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) =>
          external.has(path) ? { path, external: true } : undefined,
        );
      },
    },
  ],
});

type Element = { type: unknown; key?: string; props: Record<string, unknown> };
const scope: AttachmentDraftScope = {
  mode: 'account',
  identityId: 'owner',
  deviceId: 'device',
  spaceId: 'chat',
  replyTo: 'original-reply',
  thread: false,
};
function client(): ClientState {
  return {
    phase: 'ready',
    erasePending: false,
    mode: 'account',
    snapshot: {
      account: {
        identity_id: scope.identityId,
        device_id: scope.deviceId,
        unlocked: true,
        setup_pending: false,
      },
      me: { id: scope.identityId },
      conversations: [{ id: 'chat', members: [{ id: scope.identityId }] }],
    } as ClientState['snapshot'],
    timelines: {},
    notice: null,
    activity: {} as ClientState['activity'],
  };
}
const original = {
  id: scope.replyTo,
  author: { id: 'author', name: 'Canonical author' },
  kind: { Message: { text: 'Captured canonical reply', card: null } },
  deleted: false,
  voice: null,
} as TimelineEntry;

function render({
  state = client(),
  captured = scope,
  data = original,
  error = null,
  onCancel,
  context,
}: {
  state?: ClientState;
  captured?: AttachmentDraftScope;
  data?: TimelineEntry | null;
  error?: string | null;
  onCancel?: () => void;
  context?: {
    entry: TimelineEntry;
    editing?: boolean;
    quote?: ReplySelection;
  };
} = {}) {
  const queries: unknown[] = [];
  const previews: { text: string; literal: boolean }[] = [];
  const keys: string[] = [];
  const jsx = (type: unknown, props: Element['props'], key?: string) => ({ type, props, key });
  const modules: Record<string, unknown> = {
    'react/compiler-runtime': {
      c: (size: number) => Array(size).fill(Symbol.for('react.memo_cache_sentinel')),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { View: 'View', Text: 'Text' },
    'react-native-reanimated': { View: 'AnimatedView' },
    '../core/provider': { useClient: () => state },
    './use-semantic-message-preview': {
      useSemanticMessagePreview: (input: { text: string; literal: boolean }) => {
        previews.push(input);
        return { text: input.literal ? input.text : 'Native semantic preview' };
      },
    },
    '../core/query': {
      useCoreQuery: (command: unknown) => {
        queries.push(command);
        return { data, error };
      },
    },
    '../core/presentation': { messageText },
    '../theme': {
      useTheme: () => ({
        colors: {},
        presentation: telegramPresentation('day'),
        reducedMotion: true,
      }),
    },
    '../i18n/index.ts': {
      t: (key: string, values?: unknown) => `${key}:${JSON.stringify(values)}`,
    },
    '../components/primitives': { IconButton: 'IconButton' },
    './primitives': { IconButton: 'IconButton', Icon: 'Icon' },
  };
  const module = {
    exports: {} as {
      AttachmentReplyContext(props: Record<string, unknown>): Element;
      MessageContext(props: Record<string, unknown>): Element;
    },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(Object.hasOwn(modules, name), `Unexpected native import: ${name}`);
      return modules[name];
    },
  });
  function expand(value: unknown): Element[] {
    if (Array.isArray(value)) return value.flatMap(expand);
    if (!value || typeof value !== 'object' || !('props' in value)) return [];
    const node = value as Element;
    if (node.key) keys.push(node.key);
    if (typeof node.type === 'function') return expand(node.type(node.props));
    return [node, ...expand(node.props.children)];
  }
  const nodes = expand(
    context
      ? module.exports.MessageContext({ ...context, onCancel })
      : module.exports.AttachmentReplyContext({ scope: captured, onCancel }),
  );
  return {
    nodes,
    queries,
    previews,
    keys,
    text: nodes.filter((node) => node.type === 'Text').map((node) => node.props.children),
    cancel: nodes.find((node) => node.type === 'IconButton'),
  };
}

test('compiled attachment reply reads the captured canonical message and preserves optional cancel', () => {
  let canceled = 0;
  const result = render({ onCancel: () => canceled++ });
  assert.equal(
    JSON.stringify(result.queries),
    JSON.stringify([{ op: 'message', space_id: 'chat', message_id: 'original-reply' }]),
  );
  assert.equal(result.previews[0]?.text, 'Captured canonical reply');
  assert.ok(result.text.includes('Native semantic preview'));
  assert.ok(result.text.some((text) => String(text).includes('Canonical author')));
  (result.cancel!.props.onPress as () => void)();
  assert.equal(canceled, 1);
  assert.equal(render().cancel, undefined, 'a frozen attachment target has no misleading cancel');
});

test('compiled shared media reply renders the reviewed selected snapshot without querying inaccessible source history', () => {
  const captured = {
    ...scope,
    replyTo: null,
    replyIntent: 'A',
    sharedReply: {
      space_id: 'private',
      message_id: 'source',
      revision: 'r1',
      author_id: 'author',
      author_name: 'Original author',
      text: '😀',
      selection: {
        source_id: 'source',
        revision: 'r1',
        text: '😀',
        spans: [{ start_utf16: 6, end_utf16: 8 }],
      },
    },
  };
  let canceled = 0;
  const result = render({ captured, onCancel: () => canceled++ });
  assert.equal(result.queries.length, 0);
  assert.ok(result.text.some((value) => String(value).includes('Original author')));
  assert.equal(result.previews[0]?.text, '😀');
  (result.cancel!.props.onPress as () => void)();
  assert.equal(canceled, 1);
  assert.equal(render({ state: { ...client(), phase: 'opening' }, captured }).queries.length, 0);
});

test('foreign owner/device/mode, lost membership and unavailable phases never query or show old text', () => {
  const base = client();
  for (const state of [
    { ...base, phase: 'opening' as const },
    { ...base, erasePending: true },
    { ...base, mode: 'demo' as const },
    {
      ...base,
      snapshot: {
        ...base.snapshot!,
        account: { ...base.snapshot!.account!, identity_id: 'foreign' },
      },
    },
    {
      ...base,
      snapshot: {
        ...base.snapshot!,
        account: { ...base.snapshot!.account!, device_id: 'foreign' },
      },
    },
    {
      ...base,
      snapshot: { ...base.snapshot!, account: { ...base.snapshot!.account!, unlocked: false } },
    },
    { ...base, snapshot: { ...base.snapshot!, conversations: [] } },
    {
      ...base,
      snapshot: {
        ...base.snapshot!,
        conversations: [{ ...base.snapshot!.conversations[0]!, members: [] }],
      },
    },
  ]) {
    const result = render({
      state,
      onCancel: () => assert.fail('foreign target cannot be canceled'),
    });
    assert.deepEqual(result.queries, []);
    assert.ok(!result.text.includes('Captured canonical reply'));
    assert.equal(result.cancel, undefined);
  }
});

test('loading, errors, deleted and mismatched canonical messages use truthful labels', () => {
  assert.ok(render({ data: null }).text.includes('media.loading:undefined'));
  for (const result of [
    render({ data: null, error: 'private engine detail' }),
    render({ data: { ...original, id: 'a-different-message' } }),
    render({ data: { ...original, deleted: true } }),
    render({ error: 'failed refresh', data: original }),
  ]) {
    assert.ok(result.text.includes('chat.messageUnavailable:undefined'));
    assert.ok(!result.text.includes('Captured canonical reply'));
    assert.ok(!result.text.includes('private engine detail'));
  }
});

test('a changed device remounts its reader and a batch without a reply makes no query', () => {
  const first = render();
  const state = client();
  const next = { ...scope, deviceId: 'new-device' };
  state.snapshot!.account!.device_id = next.deviceId;
  const second = render({ state, captured: next });
  assert.notDeepEqual(
    first.keys,
    second.keys,
    'the query hook cannot recycle the prior device reader',
  );
  const noReply = render({ captured: { ...scope, replyTo: null } });
  assert.deepEqual(noReply.queries, []);
  assert.deepEqual(noReply.nodes, []);
});

test('composer previews project canonical Markdown but preserve literal selected text and generated labels', () => {
  const canonical = {
    ...original,
    kind: { Message: { text: String.raw`\"café🦊\" **bold**`, card: null } },
  } as TimelineEntry;
  const before = structuredClone(canonical);
  const full = render({ context: { entry: canonical } });
  assert.equal(full.previews[0]?.literal, false);
  assert.equal(
    full.previews[0]?.text,
    canonical.kind && 'Message' in canonical.kind ? canonical.kind.Message.text : '',
  );
  assert.ok(full.text.includes('Native semantic preview'));
  const text = String.raw`*literal* \[a\](b) café🦊`;
  const quote = {
    source_id: canonical.id,
    revision: 'r1',
    spans: [{ start_utf16: 0, end_utf16: 1 }],
    text,
  };
  const selected = render({ context: { entry: canonical, quote } });
  assert.equal(selected.previews[0]?.literal, true);
  assert.ok(selected.text.includes(text));
  const editing = render({ context: { entry: canonical, editing: true, quote } });
  assert.equal(editing.previews[0]?.literal, false);
  assert.ok(!editing.text.includes(text));
  const generated = render({
    context: {
      entry: {
        ...canonical,
        kind: {
          Message: {
            text: '',
            card: { kind_id: 'file', item_id: 'file', title: 'photo_[1].jpg', version: 1 },
          },
        } as TimelineEntry['kind'],
      },
    },
  });
  assert.equal(generated.previews[0]?.literal, true);
  assert.deepEqual(
    canonical,
    before,
    'presentation never rewrites canonical text or draft context',
  );
});
