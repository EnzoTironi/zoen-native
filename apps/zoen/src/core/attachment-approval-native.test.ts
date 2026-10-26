import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import type { AttachmentApprovalProps } from '../media/attachment-approval.types';
import type { AttachmentComposerField } from '../media/attachment-composer';
import type { AttachmentDraftItem, AttachmentDraftScope } from '../media/attachment-draft';

const code = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../media/attachment-approval.ios.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'native-gallery-wrapper-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;
type Tree = { type: unknown; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
type Call = { kind: string; id?: string; text?: string; ids?: string[] };
const scope: AttachmentDraftScope = {
  mode: 'account',
  identityId: 'identity',
  deviceId: 'device',
  spaceId: 'chat',
  replyTo: null,
  thread: false,
};
const item = (id: string): AttachmentDraftItem => ({
  uri: `file:///cache/${id}.jpg`,
  name: `${id}.jpg`,
  mime: 'image/jpeg',
  size: 12,
  mutationId: id,
  caption: `Caption ${id}`,
  attempted: false,
  accepted: null,
});
function fixture() {
  const calls: Call[] = [],
    slots: Slot[] = [];
  let cursor = 0,
    serial = 0,
    current = true;
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[]) =>
    !!a && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const effect = (read: () => void | (() => void), deps: readonly unknown[]) => {
    const s = slot();
    if (same(s.deps, deps)) return;
    s.deps = deps;
    effects.push(() => {
      s.cleanup?.();
      s.cleanup = read() ?? undefined;
    });
  };
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const s = slot();
    if (!same(s.deps, deps)) {
      s.value = read();
      s.deps = deps;
    }
    return s.value;
  };
  const jsx = (type: unknown, props: Record<string, any>): Tree => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useLayoutEffect: effect,
      useEffect: effect,
      useMemo: memo,
      useCallback: (read: () => unknown, deps: readonly unknown[]) => memo(() => read, deps),
      useRef: (initial: unknown) => {
        const s = slot();
        return (s.value ??= { current: initial });
      },
      useState: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value'))
          s.value = typeof initial === 'function' ? initial() : initial;
        return [
          s.value,
          (next: unknown) => {
            s.value = typeof next === 'function' ? next(s.value) : next;
          },
        ];
      },
    },
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: Symbol('Fragment') },
    expo: {
      requireNativeView: (module: string, name: string) => {
        assert.equal(module, 'ZoenCore');
        assert.equal(name, 'ZoenAttachmentGalleryHost');
        return name;
      },
    },
    'expo-crypto': { randomUUID: () => `native-gallery-${++serial}` },
    '../i18n': { t: (key: string) => key },
    '../theme': {
      useTheme: () => ({
        presentation: {
          actionSheet: { background: '#17212b' },
          text: '#ffffff',
          secondary: '#aaaaaa',
          accent: '#0088ff',
          separator: '#334455',
        },
      }),
    },
  };
  const module = {
    exports: {} as { AttachmentApproval(props: AttachmentApprovalProps): Tree | null },
  };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let props: AttachmentApprovalProps = {
    state: {
      scope,
      phase: 'ready',
      items: [item('a'), item('b'), item('c')],
      targetFrozen: false,
      error: null,
    },
    field: {
      mutationId: 'a',
      scope,
      targetFrozen: false,
      value: 'Caption a',
      position: 1,
      total: 3,
      editable: true,
      sending: false,
      remaining: 3,
      retry: false,
      onChange: (text) => calls.push({ kind: 'caption', text }),
      onSend: (text) => calls.push({ kind: 'send', text }),
    },
    libraryPicker: null,
    selectedId: 'a',
    recipient: 'Recipient',
    busy: false,
    editing: false,
    suspended: false,
    canAdd: true,
    canEdit: true,
    error: null,
    isCurrent: () => current,
    onClose: () => calls.push({ kind: 'close' }),
    onSelect: (id) => calls.push({ kind: 'preview', id }),
    onReorder: (ids) => calls.push({ kind: 'reorder', ids: Array.from(ids) }),
    onEdit: (text) => calls.push({ kind: 'edit', text }),
    onAdd: (kind) => calls.push({ kind: 'add', id: kind }),
    onRemove: (id) => calls.push({ kind: 'remove', id }),
    onRetry: () => calls.push({ kind: 'retry' }),
  };
  const find = (tree: Tree | null): Tree | null => {
    if (!tree) return null;
    if (tree.type === 'ZoenAttachmentGalleryHost') return tree;
    const children = Array.isArray(tree.props.children)
      ? tree.props.children
      : [tree.props.children];
    for (const child of children)
      if (child && typeof child === 'object') {
        const found = find(child);
        if (found) return found;
      }
    return null;
  };
  const render = (update: Partial<AttachmentApprovalProps> = {}) => {
    props = { ...props, ...update };
    cursor = 0;
    effects = [];
    const tree = module.exports.AttachmentApproval(props);
    effects.forEach((run) => run());
    return find(tree);
  };
  const required = (update: Partial<AttachmentApprovalProps> = {}) => {
    const tree = render(update);
    assert.ok(tree);
    return tree;
  };
  const act = (tree: Tree, action: string, id?: string, payload: Record<string, unknown> = {}) => {
    const binding = JSON.parse(tree.props.binding),
      model = JSON.parse(tree.props.model);
    tree.props.onAction({
      nativeEvent: {
        token: binding.token,
        context: binding.context,
        revision: model.revision,
        action,
        id,
        ...payload,
      },
    });
  };
  const close = (tree: Tree, payload: Record<string, unknown> = {}) => {
    const b = JSON.parse(tree.props.binding);
    tree.props.onClosed({
      nativeEvent: { token: b.token, context: b.context, reason: 'closed', ...payload },
    });
  };
  return {
    calls,
    render,
    required,
    act,
    close,
    props: () => props,
    field: (update: Partial<AttachmentComposerField>) => ({ ...props.field, ...update }),
    current: (value: boolean) => {
      current = value;
    },
    unmount: () => {
      for (const s of slots) {
        s.cleanup?.();
        s.cleanup = undefined;
      }
    },
  };
}

test('actual approval projects canonical mutation IDs into the shared native gallery', () => {
  const f = fixture(),
    tree = f.required(),
    binding = JSON.parse(tree.props.binding),
    model = JSON.parse(tree.props.model);
  assert.equal(tree.type, 'ZoenAttachmentGalleryHost');
  assert.equal(binding.context, JSON.stringify(scope));
  assert.equal(binding.active, true);
  assert.equal(binding.suspended, false);
  assert.equal(binding.appearance.accent, '#0088ff');
  assert.equal(model.source, 'draft');
  assert.equal(model.screen, 'preview');
  assert.equal(model.previewId, 'a');
  assert.deepEqual(model.selected, ['a', 'b', 'c']);
  assert.deepEqual(
    model.items.map((v: { id: string }) => v.id),
    ['a', 'b', 'c'],
  );
  assert.equal(model.caption.value, 'Caption a');
  assert.equal(model.recipient, 'Recipient');
  assert.equal(f.calls.length, 0);
  f.unmount();
});

test('foreign and superseded native approval callbacks cannot mutate the current draft', () => {
  const f = fixture(),
    first = f.required();
  for (const p of [{ token: 'foreign' }, { context: 'foreign' }, { revision: 'foreign' }]) {
    f.act(first, 'caption', 'a', { text: 'Foreign', ...p });
    f.act(first, 'send', 'a', { text: 'Foreign', ...p });
    f.act(first, 'remove', 'a', p);
  }
  const next = f.required({ selectedId: 'b', field: f.field({ value: 'Caption b', position: 2 }) });
  assert.equal(JSON.parse(next.props.binding).token, JSON.parse(first.props.binding).token);
  assert.notEqual(JSON.parse(next.props.model).revision, JSON.parse(first.props.model).revision);
  f.act(first, 'send', 'a', { text: 'Old selection' });
  f.act(first, 'preview', 'c');
  f.close(first);
  assert.equal(f.calls.length, 0);
  f.current(false);
  f.act(next, 'send', 'b', { text: 'Retired destination' });
  assert.equal(f.calls.length, 0);
  f.unmount();
});

test('caption and Send address only the current item and Send forwards final native text exactly', () => {
  const f = fixture(),
    tree = f.required();
  for (const id of [undefined, 'b', 'foreign']) {
    f.act(tree, 'caption', id, { text: 'Wrong item' });
    f.act(tree, 'send', id, { text: 'Wrong item' });
  }
  assert.equal(f.calls.length, 0);
  f.act(tree, 'caption', 'a', { text: '  Exact caption 🖼️\n' });
  f.act(tree, 'send', 'a', { text: 'Final native keystroke!' });
  assert.deepEqual(f.calls, [
    { kind: 'caption', text: '  Exact caption 🖼️\n' },
    { kind: 'send', text: 'Final native keystroke!' },
  ]);
  assert.equal(f.props().field.value, 'Caption a');
  f.unmount();
});

test('approval rejects malformed text and admits the exact caption boundary', () => {
  const f = fixture(),
    tree = f.required();
  for (const text of ['x'.repeat(4097), 'no\0nul', 42, null]) {
    f.act(tree, 'caption', 'a', { text });
    f.act(tree, 'send', 'a', { text });
  }
  assert.equal(f.calls.length, 0);
  f.act(tree, 'send', 'a', { text: 'x'.repeat(4096) });
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0]!.text?.length, 4096);
  f.unmount();
});

test('readonly captions and busy, editing or suspended approval fence their respective actions', () => {
  const f = fixture(),
    readonly = f.required({ field: f.field({ editable: false }), canEdit: false, canAdd: false });
  f.act(readonly, 'caption', 'a', { text: 'Readonly' });
  f.act(readonly, 'edit', 'a');
  f.act(readonly, 'add', undefined, { kind: 'photo' });
  assert.equal(f.calls.length, 0);
  for (const p of [
    { busy: true, editing: false, suspended: false },
    { busy: false, editing: true, suspended: false },
    { busy: false, editing: false, suspended: true },
  ]) {
    const tree = f.required({
      ...p,
      field: f.field({ editable: true }),
      canEdit: true,
      canAdd: true,
    });
    for (const action of [
      'caption',
      'send',
      'edit',
      'remove',
      'preview',
      'retry',
      'close',
      'selected',
    ])
      f.act(tree, action, 'a', { text: 'Fenced' });
    f.act(tree, 'add', undefined, { kind: 'photo' });
    f.act(tree, 'reorder', undefined, { ids: ['c', 'b', 'a'] });
    assert.equal(f.calls.length, 0);
    assert.equal(JSON.parse(tree.props.binding).suspended, p.editing || p.suspended);
  }
  const sending = f.required({
    busy: false,
    editing: false,
    suspended: false,
    field: f.field({ sending: true }),
  });
  f.act(sending, 'send', 'a', { text: 'Already sending' });
  assert.equal(f.calls.length, 0);
  const empty = f.required({ field: f.field({ sending: false, remaining: 0 }) });
  f.act(empty, 'send', 'a', { text: 'No remaining' });
  assert.equal(f.calls.length, 0);
  f.unmount();
});

test('nonterminal suspension retains its native token and resumes only fresh current callbacks', () => {
  const f = fixture(),
    first = f.required(),
    suspended = f.required({ suspended: true });
  assert.equal(JSON.parse(suspended.props.binding).token, JSON.parse(first.props.binding).token);
  assert.equal(JSON.parse(suspended.props.binding).active, true);
  assert.equal(JSON.parse(suspended.props.binding).suspended, true);
  f.act(suspended, 'send', 'a', { text: 'Child open' });
  const resumed = f.required({ suspended: false });
  assert.equal(JSON.parse(resumed.props.binding).token, JSON.parse(first.props.binding).token);
  f.act(first, 'send', 'a', { text: 'Old' });
  f.act(suspended, 'send', 'a', { text: 'Suspended' });
  assert.equal(f.calls.length, 0);
  f.act(resumed, 'send', 'a', { text: 'Resumed final caption' });
  assert.deepEqual(f.calls, [{ kind: 'send', text: 'Resumed final caption' }]);
  f.unmount();
});

test('selected mode stays native and reorder requires an exact unique permutation of current IDs', () => {
  const f = fixture(),
    first = f.required();
  f.act(first, 'selected');
  const selected = f.required();
  assert.equal(JSON.parse(selected.props.model).screen, 'selected');
  assert.equal(f.calls.length, 0);
  for (const ids of [
    [],
    ['a'],
    ['a', 'a', 'c'],
    ['a', 'b', 'foreign'],
    ['a', 'b', 'c', 'd'],
    [1, 2, 3],
    'abc',
  ])
    f.act(selected, 'reorder', undefined, { ids });
  assert.equal(f.calls.length, 0);
  f.act(selected, 'reorder', undefined, { ids: ['c', 'a', 'b'] });
  assert.deepEqual(f.calls, [{ kind: 'reorder', ids: ['c', 'a', 'b'] }]);
  f.act(selected, 'back');
  const preview = f.required();
  assert.equal(JSON.parse(preview.props.model).screen, 'preview');
  assert.equal(f.calls.filter((v) => v.kind === 'close').length, 0);
  f.act(preview, 'back');
  assert.equal(f.calls.filter((v) => v.kind === 'close').length, 1);
  f.unmount();
});

test('known preview, attempted-item removal and approved add kinds retain canonical gates', () => {
  const f = fixture(),
    tree = f.required();
  f.act(tree, 'preview', 'foreign');
  f.act(tree, 'remove', 'b');
  f.act(tree, 'add', undefined, { kind: 'unknown' });
  assert.equal(f.calls.length, 0);
  f.act(tree, 'preview', 'c');
  f.act(tree, 'edit', 'a', { text: 'Final native edit caption' });
  f.act(tree, 'remove', 'a');
  for (const kind of ['photo', 'camera', 'document']) f.act(tree, 'add', undefined, { kind });
  assert.deepEqual(f.calls, [
    { kind: 'preview', id: 'c' },
    { kind: 'edit', text: 'Final native edit caption' },
    { kind: 'remove', id: 'a' },
    { kind: 'add', id: 'photo' },
    { kind: 'add', id: 'camera' },
    { kind: 'add', id: 'document' },
  ]);
  f.calls.length = 0;
  const attempted = f.required({
    state: {
      ...f.props().state,
      items: f
        .props()
        .state.items.map((v) => (v.mutationId === 'a' ? { ...v, attempted: true } : v)),
    },
  });
  f.act(attempted, 'remove', 'a');
  assert.equal(f.calls.length, 0);
  f.unmount();
});

test('native close addresses its committed owner and retired contexts or unmounted callbacks cannot close replacements', () => {
  const f = fixture(),
    first = f.required();
  f.close(first, { token: 'foreign' });
  f.close(first, { context: 'foreign' });
  const replacement = f.required({
    field: f.field({ scope: { ...scope, spaceId: 'another-chat' } }),
  });
  assert.notEqual(
    JSON.parse(replacement.props.binding).token,
    JSON.parse(first.props.binding).token,
  );
  f.close(first);
  assert.equal(f.calls.length, 0);
  f.current(false);
  f.close(replacement, { reason: 'retired' });
  assert.deepEqual(f.calls, [{ kind: 'close' }]);
  f.calls.length = 0;
  f.unmount();
  f.close(replacement);
  f.act(replacement, 'send', 'a', { text: 'Unmounted' });
  assert.equal(f.calls.length, 0);
});

test('the real native Gallery model binds a mixed GeoJSON page to its retained URI/hash without changing order, reply/thread or captions', () => {
  const f = fixture();
  const threaded = { ...scope, replyTo: 'original-reply', thread: true };
  const items = [
    item('a'),
    {
      ...item('point'),
      name: 'location.geojson',
      mime: 'application/geo+json',
      sha256: 'a'.repeat(64),
      caption: 'Exact point caption',
    },
    { ...item('c'), mime: 'video/mp4' },
  ];
  let tree = f.required({
    state: { ...f.props().state, scope: threaded, items },
    field: { ...f.field({ value: 'Exact point caption' }), scope: threaded },
    selectedId: 'point',
    canEdit: false,
  });
  const model = JSON.parse(tree.props.model);
  assert.deepEqual(
    Array.from(model.items, (value: { id: string }) => value.id),
    ['a', 'point', 'c'],
  );
  assert.equal(model.items[1].uri, items[1]!.uri);
  assert.equal(
    model.items[1].sha256,
    'a'.repeat(64),
    'the map must verify the existing journal bytes, not accept coordinate props',
  );
  assert.equal(model.caption.value, 'Exact point caption');
  assert.deepEqual(JSON.parse(JSON.parse(tree.props.binding).context), threaded);
  assert.equal(model.canEdit, false);
  const old = tree;
  f.act(tree, 'caption', 'point', { text: 'Updated caption' });
  f.act(tree, 'preview', 'a');
  tree = f.required({ selectedId: 'a', field: f.field({ value: 'Photo caption' }) });
  f.act(old, 'send', 'point', { text: 'stale' });
  f.close(tree);
  assert.deepEqual(
    f.calls.map((call) => call.kind),
    ['caption', 'preview', 'close'],
  );
});
