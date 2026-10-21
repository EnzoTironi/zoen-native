import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
const code = (
  await build({
    stdin: {
      contents: readFileSync(
        new URL('../components/use-message-actions.tsx', import.meta.url),
        'utf8',
      ),
      loader: 'tsx',
    },
    bundle: true,
    format: 'cjs',
    platform: 'node',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'context-hook-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;
const deferred = <T>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
const tick = async () => {
  for (let i = 0; i < 8; i++) await Promise.resolve();
};
function fixture() {
  const keyboard = deferred<void>(),
    admission = deferred<boolean>();
  const calls: any[] = [],
    refs: { current: any }[] = [],
    states: any[] = [],
    effects: (() => void)[] = [];
  let refCursor = 0,
    stateCursor = 0,
    initialized = false,
    focused = false;
  let focusCleanup: () => void = () => {},
    mountedCleanup: () => void = () => {};
  let row = true,
    visible = true;
  let entry = { id: 'm1', author: { is_me: false }, reactions: [] as any[], deleted: false };
  const state = {
    phase: 'ready',
    mode: 'native',
    snapshot: { account: { identity_id: 'me', device_id: 'device' }, me: { id: 'me' } },
  };
  const store = {
    getSnapshot: () => state,
    subscribe: () => () => {},
    request: async (value: unknown) => {
      calls.push(value);
    },
    driver: { request: async () => entry },
  };
  const request = {
    binding: 'row',
    gestureId: 'gesture',
    kind: 'hold',
    current: () => row,
    native: { consumeGesture: () => admission.promise },
  };
  const ports: Record<string, any> = {
    react: {
      useRef: (value: unknown) => (refs[refCursor++] ??= { current: value }),
      useState: (initial: unknown) => {
        const i = stateCursor++;
        if (!(i in states)) states[i] = initial;
        return [
          states[i],
          (value: any) => {
            states[i] = typeof value === 'function' ? value(states[i]) : value;
          },
        ];
      },
      useCallback: (callback: unknown) => callback,
      useLayoutEffect: (effect: () => void) => effects.push(effect),
      useEffect: (effect: () => void | (() => void)) => {
        if (!initialized)
          effects.push(() => {
            const cleanup = effect();
            if (cleanup) mountedCleanup = cleanup;
          });
      },
    },
    'react/jsx-runtime': { jsx: (type: unknown, props: unknown) => ({ type, props }) },
    'expo-router': {
      router: { push: (value: unknown) => calls.push(value) },
      useFocusEffect: (effect: () => () => void) => {
        if (!focused) {
          focused = true;
          focusCleanup = effect();
        }
      },
    },
    'expo-clipboard': {},
    'react-native-keyboard-controller': {
      KeyboardController: {
        dismiss: () => {
          calls.push('keyboard');
          return keyboard.promise;
        },
        isVisible: () => visible,
      },
    },
    '../core/provider': {
      useClient: () => ({ store, snapshot: state.snapshot, mode: state.mode }),
    },
    '../core/message-presentation': {
      messageCanInteract: () => true,
      messageCanRecoverMedia: () => false,
      messageCanEdit: () => false,
      messageCard: () => null,
    },
    '../core/presentation': { messageText: () => '' },
    '../media/share-message': {},
    '../i18n/index.ts': { t: (key: string) => key },
    '../home/access': { homeSpacesFor: () => new Set(['space']) },
    './dialogs': {
      useDialogs: () => ({
        confirm: async () => true,
        inform: (...value: unknown[]) => calls.push(value),
      }),
    },
    './message-actions': { MessageActions: 'MessageActions' },
    './reaction-details': { ReactionDetails: 'ReactionDetails' },
  };
  const module = { exports: {} as { useMessageActions(props: any): any } };
  runInNewContext(code, {
    module,
    exports: module.exports,
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  const render = () => {
    refCursor = 0;
    stateCursor = 0;
    const result = module.exports.useMessageActions({
      spaceId: 'space',
      entries: [entry],
      onReply() {},
      onEdit() {},
    });
    effects.splice(0).forEach((run) => run());
    initialized = true;
    return result;
  };
  return {
    render,
    calls,
    keyboard,
    admission,
    request,
    entry: () => entry,
    revokeRow: () => {
      row = false;
    },
    blur: () => focusCleanup(),
    changeAccount: () => {
      state.snapshot.account.identity_id = 'other';
    },
    hideKeyboard: () => {
      visible = false;
      keyboard.resolve();
    },
    replaceEntry: () => {
      entry = { ...entry, reactions: [{ emoji: '❤️', mine: true }] };
    },
    unmount: () => {
      mountedCleanup();
    },
  };
}

test('actual compiled hook uses ONE existing keyboard clock before attaching native context', async () => {
  const f = fixture(),
    hook = f.render();
  const opening = hook.open(f.entry(), 'actions', undefined, undefined, undefined, f.request);
  assert.deepEqual(f.calls, ['keyboard']);
  assert.equal(f.render().panel.props.entry, null);
  f.hideKeyboard();
  await opening;
  assert.equal(f.render().panel.props.nativeSource, f.request);
  assert.equal(f.calls.length, 1);
});
for (const boundary of ['revokeRow', 'blur', 'changeAccount'] as const)
  test(`actual keyboard-held context cannot cross ${boundary}`, async () => {
    const f = fixture(),
      hook = f.render();
    const opening = hook.open(f.entry(), 'actions', undefined, undefined, undefined, f.request);
    f[boundary]();
    f.hideKeyboard();
    await opening;
    assert.equal(f.render().panel.props.entry, null);
  });
test('double tap awaits exact native gesture admission and then toggles the latest canonical reaction', async () => {
  const f = fixture(),
    hook = f.render();
  const request = { ...f.request, kind: 'double' };
  const opening = hook.open(f.entry(), 'reactions', undefined, undefined, undefined, request);
  assert.deepEqual(f.calls, []);
  f.replaceEntry();
  f.render();
  f.admission.resolve(true);
  await opening;
  await tick();
  assert.deepEqual(JSON.parse(JSON.stringify(f.calls)), [
    { op: 'react', space_id: 'space', message_id: 'm1', emoji: null },
  ]);
});
for (const boundary of ['revokeRow', 'blur', 'changeAccount'] as const)
  test(`native-admission-held quick reaction cannot cross ${boundary}`, async () => {
    const f = fixture(),
      hook = f.render();
    const opening = hook.open(f.entry(), 'reactions', undefined, undefined, undefined, {
      ...f.request,
      kind: 'double',
    });
    f[boundary]();
    f.admission.resolve(true);
    await opening;
    await tick();
    assert.deepEqual(f.calls, []);
  });
test('native refusal after another activation performs no canonical mutation or keyboard movement', async () => {
  const f = fixture(),
    hook = f.render();
  const opening = hook.open(f.entry(), 'reactions', undefined, undefined, undefined, {
    ...f.request,
    kind: 'double',
  });
  f.admission.resolve(false);
  await opening;
  assert.deepEqual(f.calls, []);
});
