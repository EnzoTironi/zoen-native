import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
const compiled = await Promise.all(
  ['../components/message-context-control.tsx', '../components/message-interaction.tsx'].map(
    async (name) =>
      (
        await build({
          stdin: { contents: readFileSync(new URL(name, import.meta.url), 'utf8'), loader: 'tsx' },
          bundle: true,
          format: 'cjs',
          platform: 'node',
          jsx: 'automatic',
          write: false,
          plugins: [
            {
              name: 'context-control-ports',
              setup(builder) {
                builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
              },
            },
          ],
        })
      ).outputFiles[0]!.text,
  ),
);
const deferred = () => {
  let resolve!: () => void, reject!: () => void;
  const promise = new Promise<void>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
const tick = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};
function fixture(nativeOwnsPress = false, barrier = true) {
  const removed = deferred(),
    owns = {},
    commands = {},
    control = {};
  const calls: string[] = [];
  let panned = false;
  const dismiss = () => {
    calls.push('dismiss');
    return removed.promise;
  };
  const ports: Record<string, any> = {
    react: {
      createContext: (value: unknown) => ({ value }),
      use: (context: any) =>
        context === owns
          ? nativeOwnsPress
          : context === commands
            ? dismiss
            : context === control
              ? barrier
                ? dismiss
                : null
              : { get: () => panned },
    },
    'react/jsx-runtime': { jsx: (type: unknown, props: unknown) => ({ type, props }) },
    'react-native': { View: 'View', Pressable: 'Pressable' },
    './message-context-ports': {
      MessageContextOwnsPress: owns,
      MessageContextStageCommands: commands,
      MessageContextControlBarrier: control,
    },
    '../core/message-swipe.ts': {},
  };
  const modules = compiled.map((code) => {
    const module = { exports: {} as any };
    runInNewContext(code, {
      module,
      exports: module.exports,
      process: { env: {} },
      require: (path: string) => ports[path] ?? {},
    });
    return module.exports;
  });
  const pressable = () =>
    modules[1].MessagePressable({
      onPress: () => calls.push('profile'),
      onLongPress: () => calls.push('hold'),
      onAccessibilityTap: () => calls.push('accessible'),
      testID: 'existing-profile',
    });
  return {
    removed,
    calls,
    pressable,
    panned: () => {
      panned = true;
    },
    control: (props: any) => modules[0].MessageContextControl(props),
    event: { persist: () => calls.push('persist') },
  };
}
test('control wrapper keeps the SAME real child and resets bubble press suppression', () => {
  const f = fixture(),
    child = { type: 'OriginalProfilePressable', onPress() {} };
  const tree = f.control({ controlKey: 'author', children: child, style: { height: 20 } });
  assert.equal(tree.props.value, false);
  const marker = tree.props.children.props.children;
  assert.equal(marker.type, 'View');
  assert.equal(marker.props.testID, 'message-context-control:author');
  assert.equal(marker.props.collapsable, false);
  assert.equal(marker.props.children, child);
  assert.equal(marker.props.style.height, 20);
});
test('real MessagePressable profile waits actual native removal before using its original callback', async () => {
  const f = fixture();
  f.pressable().props.onPress(f.event);
  assert.deepEqual(f.calls, ['persist', 'dismiss']);
  f.removed.resolve();
  await tick();
  assert.deepEqual(f.calls, ['persist', 'dismiss', 'profile']);
});
test('a pan consumed during return cancels the queued original control callback', async () => {
  const f = fixture();
  f.pressable().props.onPress(f.event);
  f.panned();
  f.removed.resolve();
  await tick();
  assert.ok(!f.calls.includes('profile'));
});
test('failed native removal cannot start navigation behind an extracted message', async () => {
  const f = fixture();
  f.pressable().props.onPress(f.event);
  f.removed.reject();
  await tick();
  assert.ok(!f.calls.includes('profile'));
});
test('native-owned bubble has no duplicate physical tap/hold but retains its accessible action', async () => {
  const f = fixture(true, false),
    node = f.pressable();
  assert.equal(node.props.onPress, undefined);
  assert.equal(node.props.onLongPress, undefined);
  node.props.onAccessibilityTap(f.event);
  assert.deepEqual(f.calls, ['accessible']);
});
test('VoiceOver control also waits removal; plain fallback control remains synchronous', async () => {
  const f = fixture();
  f.pressable().props.onAccessibilityTap(f.event);
  assert.ok(!f.calls.includes('accessible'));
  f.removed.resolve();
  await tick();
  assert.equal(f.calls.at(-1), 'accessible');
  const fallback = fixture(false, false);
  fallback.pressable().props.onPress(fallback.event);
  assert.deepEqual(fallback.calls, ['profile']);
});

test('VoiceOver bypasses consumed physical swipe but still awaits the actual context removal barrier', async () => {
  const f = fixture();
  f.panned();
  f.pressable().props.onAccessibilityTap();
  assert.deepEqual(f.calls, ['dismiss']);
  f.removed.resolve();
  await tick();
  assert.deepEqual(f.calls, ['dismiss', 'accessible']);
  const noMenu = fixture(false, false);
  noMenu.panned();
  noMenu.pressable().props.onAccessibilityTap();
  assert.deepEqual(noMenu.calls, ['accessible']);
});
