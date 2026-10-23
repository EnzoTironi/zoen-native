import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as scope from './account-scope.ts';
import { deferred } from './store.fixtures.ts';

type Element = { type: any; props: Record<string, any> };
const compiled = await build({
  stdin: {
    contents: readFileSync(new URL('../components/authorize-device.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  external: ['*'],
});
function nodes(value: unknown): Element[] {
  if (Array.isArray(value)) return value.flatMap(nodes);
  if (!value || typeof value !== 'object' || !('props' in value)) return [];
  const node = value as Element;
  return [node, ...nodes(node.props.children)];
}
async function settle() {
  for (let n = 0; n < 20; ++n) await Promise.resolve();
}
function fixture() {
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: 'https://relay.test',
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
    },
  };
  const listeners = new Set<() => void>(),
    preflights = new Set<() => void>();
  const foreground = new Set<(value: string) => void>();
  const calls: { op: string; owner: string; epoch: number; code?: string }[] = [];
  let linked = 0,
    launched = 0,
    dismissed = 0;
  let barcode: ((value: { data: string }) => void) | undefined;
  let confirm: () => Promise<boolean> = async () => true;
  let permission: () => Promise<{ granted: boolean }> = async () => ({ granted: true });
  let rpc: (command: any) => Promise<any> = async (command) => {
    if (command.op === 'link_code_check') return '123456';
    if (command.op === 'link_device') return { device_id: 'peer' };
    if (command.op === 'link_progress') return [1, 1];
    if (command.op === 'send_history') return 1;
    throw new Error(`Unexpected command ${command.op}`);
  };
  const store = {
    getSnapshot: () => state,
    sessionEpoch: 1,
    isSessionRetiring: false,
    driver: {
      request: async (command: any) => {
        calls.push({
          op: command.op,
          owner: state.snapshot.account.identity_id,
          epoch: store.sessionEpoch,
          code: command.code,
        });
        return rpc(command);
      },
    },
    subscribe: (fn: () => void) => {
      listeners.add(fn);
      return () => {
        listeners.delete(fn);
      };
    },
    registerSessionPreflight: (fn: () => void) => {
      preflights.add(fn);
      return () => {
        preflights.delete(fn);
      };
    },
  };
  const app = {
    currentState: 'active',
    addEventListener: (_: string, fn: (value: string) => void) => {
      foreground.add(fn);
      return {
        remove: () => {
          foreground.delete(fn);
        },
      };
    },
  };
  const slots: any[] = [];
  let cursor = 0;
  const effects: (() => void)[] = [],
    cleanups = new Set<() => void>();
  const memo = (factory: () => unknown, deps: unknown[]) => {
    const i = cursor++;
    if (!slots[i] || !deps.every((v, n) => Object.is(v, slots[i].deps[n])))
      slots[i] = { value: factory(), deps };
    return slots[i].value;
  };
  const effect = (action: () => void | (() => void), deps: unknown[]) => {
    const i = cursor++;
    if (!slots[i] || !deps.every((v, n) => Object.is(v, slots[i].deps[n]))) {
      const prior = slots[i];
      slots[i] = { deps };
      effects.push(() => {
        if (prior?.cleanup) {
          prior.cleanup();
          cleanups.delete(prior.cleanup);
        }
        const cleanup = action();
        slots[i].cleanup = cleanup;
        if (cleanup) cleanups.add(cleanup);
      });
    }
  };
  let focus: (() => void | (() => void)) | undefined, nextFocus: typeof focus;
  let focusCleanup: (() => void) | undefined;
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    require(name: string) {
      if (name === 'react/jsx-runtime')
        return {
          jsx: (type: unknown, props: object) => ({ type, props }),
          jsxs: (type: unknown, props: object) => ({ type, props }),
          Fragment: 'Fragment',
        };
      if (name === 'react')
        return {
          useState: (initial: unknown) => {
            const i = cursor++;
            if (!(i in slots)) slots[i] = initial;
            return [
              slots[i],
              (value: unknown) => {
                slots[i] = value;
              },
            ];
          },
          useRef: (value: unknown) => memo(() => ({ current: value }), []),
          useCallback: (fn: () => unknown, deps: unknown[]) => memo(() => fn, deps),
          useEffect: effect,
        };
      if (name === 'expo-router')
        return {
          useFocusEffect: (fn: typeof focus) => {
            if (fn !== focus) nextFocus = fn;
          },
        };
      if (name === 'react-native') return { Text: 'Text', View: 'View', AppState: app };
      if (name === 'expo-camera')
        return {
          useCameraPermissions: () => [null, () => permission()],
          CameraView: {
            isModernBarcodeScannerAvailable: true,
            onModernBarcodeScanned: (fn: typeof barcode) => {
              barcode = fn;
              return {
                remove: () => {
                  barcode = undefined;
                },
              };
            },
            launchScanner: async () => {
              ++launched;
            },
            dismissScanner: async () => {
              ++dismissed;
            },
          },
        };
      if (name === '../core/provider') return { useClient: () => ({ store }) };
      if (name === '../core/account-scope') return scope;
      if (name === './dialogs') return { useDialogs: () => ({ confirm: () => confirm() }) };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === './primitives')
        return { Field: 'Field', NativeButton: 'Button', Surface: 'Surface' };
      throw new Error(`Unexpected module ${name}`);
    },
  });
  const render = () => {
    cursor = 0;
    const tree = module.exports.AuthorizeDevice({ onLinked: () => ++linked });
    if (nextFocus) {
      focusCleanup?.();
      focus = nextFocus;
      nextFocus = undefined;
      focusCleanup = focus?.() || undefined;
    }
    while (effects.length) effects.shift()!();
    return tree;
  };
  render();
  const button = (label: string) => {
    const node = nodes(render()).find((v) => v.type === 'Button' && v.props.label === label);
    assert.ok(node, `button ${label}`);
    return node;
  };
  const enter = () => {
    const field = nodes(render()).find((v) => v.type === 'Field')!;
    field.props.onChangeText('fixture-code');
  };
  return {
    state,
    store,
    calls,
    render,
    button,
    enter,
    async check() {
      enter();
      button('access.checkCode').props.onPress();
      await settle();
    },
    async link() {
      await this.check();
      button('access.matchingDigitsAuthorize').props.onPress();
      await settle();
    },
    rpc(fn: typeof rpc) {
      rpc = fn;
    },
    confirm(fn: typeof confirm) {
      confirm = fn;
    },
    permission(fn: typeof permission) {
      permission = fn;
    },
    linked: () => linked,
    launched: () => launched,
    dismissed: () => dismissed,
    scan(data: string) {
      barcode?.({ data });
    },
    changed() {
      for (const fn of listeners) fn();
    },
    preflight() {
      for (const fn of preflights) fn();
    },
    background() {
      app.currentState = 'background';
      for (const fn of foreground) fn('background');
    },
    blur() {
      focusCleanup?.();
      focusCleanup = undefined;
    },
    refocus() {
      focusCleanup = focus?.() || undefined;
    },
    close() {
      focusCleanup?.();
      for (const fn of cleanups) fn();
      cleanups.clear();
    },
  };
}

test('real authorization coalesces taps and links/transfers only the current reviewed account', async () => {
  const f = fixture();
  f.enter();
  const same = f.button('access.checkCode');
  same.props.onPress();
  same.props.onPress();
  await settle();
  assert.equal(f.calls.filter((c) => c.op === 'link_code_check').length, 1);
  const authorize = f.button('access.matchingDigitsAuthorize');
  authorize.props.onPress();
  authorize.props.onPress();
  await settle();
  const transfer = f.button('access.sendHistory');
  transfer.props.onPress();
  transfer.props.onPress();
  await settle();
  assert.deepEqual(
    f.calls.map((c) => c.op),
    ['link_code_check', 'link_device', 'link_progress', 'send_history'],
  );
  assert.equal(f.linked(), 2);
  f.close();
});

test('late real confirmations cannot link after account, epoch, focus or preflight retirement', async () => {
  for (const change of [
    'identity',
    'device',
    'relay',
    'epoch',
    'blur',
    'preflight',
    'background',
    'unmount',
  ]) {
    const f = fixture();
    await f.check();
    const confirmation = deferred<boolean>();
    f.confirm(() => confirmation.promise);
    f.button('access.matchingDigitsAuthorize').props.onPress();
    if (change === 'identity') f.state.snapshot.account.identity_id = 'replacement';
    if (change === 'device') f.state.snapshot.account.device_id = 'replacement-phone';
    if (change === 'relay') f.state.snapshot.account.relay_url = 'https://replacement.test';
    if (change === 'epoch') ++f.store.sessionEpoch;
    if (change === 'blur') f.blur();
    if (change === 'preflight') f.preflight();
    if (change === 'background') f.background();
    if (change === 'unmount') f.close();
    f.changed();
    confirmation.resolve(true);
    await settle();
    assert.equal(f.calls.filter((c) => c.op === 'link_device').length, 0, change);
    assert.equal(f.linked(), 0, change);
    f.close();
  }
});

test('late check and link responses cannot publish or continue a replacement account flow', async () => {
  for (const op of ['link_code_check', 'link_device']) {
    const f = fixture();
    if (op === 'link_device') await f.check();
    const response = deferred<any>();
    f.rpc(() => response.promise);
    if (op === 'link_code_check') f.enter();
    f.button(
      op === 'link_device' ? 'access.matchingDigitsAuthorize' : 'access.checkCode',
    ).props.onPress();
    await settle();
    ++f.store.sessionEpoch;
    f.changed();
    response.resolve(op === 'link_device' ? { device_id: 'peer' } : '123456');
    await settle();
    assert.equal(f.linked(), 0);
    assert.ok(!nodes(f.render()).some((v) => v.props.label === 'access.sendHistory'));
    f.close();
  }
});

test('history progress and final response cannot continue after the owner retires', async () => {
  for (const cut of ['link_progress', 'send_history']) {
    const f = fixture();
    await f.link();
    const response = deferred<any>();
    f.rpc(async (command) => (command.op === cut ? response.promise : [1, 1]));
    f.button('access.sendHistory').props.onPress();
    await settle();
    f.background();
    response.resolve(cut === 'link_progress' ? [1, 1] : 20);
    await settle();
    assert.equal(
      f.calls.filter((c) => c.op === 'send_history').length,
      cut === 'link_progress' ? 0 : 1,
    );
    assert.equal(f.linked(), 1);
    f.close();
  }
});

test('permission replies and scanner callbacks are retired before any late code can be used', async () => {
  const f = fixture(),
    response = deferred<{ granted: boolean }>();
  f.permission(() => response.promise);
  f.button('access.scanQr').props.onPress();
  f.background();
  response.resolve({ granted: true });
  await settle();
  assert.equal(f.launched(), 0);
  f.close();
  const g = fixture();
  g.button('access.scanQr').props.onPress();
  await settle();
  assert.equal(g.launched(), 1);
  g.background();
  g.scan('late-private-code');
  assert.ok(g.dismissed() > 0);
  assert.equal(nodes(g.render()).find((v) => v.type === 'Field')?.props.value, '');
  g.close();
});

test('old rejected work cannot clear a replacement focus busy lease or show its error', async () => {
  const f = fixture(),
    old = deferred<string>(),
    fresh = deferred<string>();
  f.rpc(() => old.promise);
  f.enter();
  f.button('access.checkCode').props.onPress();
  f.blur();
  f.refocus();
  f.rpc(() => fresh.promise);
  f.enter();
  f.button('access.checkCode').props.onPress();
  old.reject(new Error('retired-secret-error'));
  await settle();
  assert.equal(f.button('access.wait').props.disabled, true);
  assert.ok(!JSON.stringify(f.render()).includes('retired-secret-error'));
  fresh.resolve('654321');
  await settle();
  assert.equal(f.button('access.matchingDigitsAuthorize').props.disabled, false);
  f.close();
});

test('an already queued edit cannot replace the code while its digits are being checked', async () => {
  const f = fixture(),
    checked = deferred<string>();
  f.enter();
  const field = nodes(f.render()).find((node) => node.type === 'Field')!;
  f.rpc(async (command) =>
    command.op === 'link_code_check' ? checked.promise : { device_id: 'peer' },
  );
  f.button('access.checkCode').props.onPress();
  field.props.onChangeText('different-unreviewed-code');
  checked.resolve('123456');
  await settle();
  assert.equal(
    nodes(f.render()).find((node) => node.type === 'Field')?.props.value,
    'fixture-code',
  );
  f.button('access.matchingDigitsAuthorize').props.onPress();
  await settle();
  assert.equal(f.calls.find((command) => command.op === 'link_device')?.code, 'fixture-code');
  f.close();
});

test('buttons and edits from a retired render cannot borrow the replacement focus lease', async () => {
  for (const action of ['authorize', 'history']) {
    const f = fixture();
    if (action === 'history') await f.link();
    else await f.check();
    const stale = f.button(
      action === 'history' ? 'access.sendHistory' : 'access.matchingDigitsAuthorize',
    );
    const field = nodes(f.render()).find((node) => node.type === 'Field');
    const before = f.calls.length;
    f.blur();
    f.refocus();
    stale.props.onPress();
    field?.props.onChangeText('retired-code');
    await settle();
    assert.equal(f.calls.length, before, action);
    assert.equal(nodes(f.render()).find((node) => node.type === 'Field')?.props.value, '');
    f.close();
  }
});
