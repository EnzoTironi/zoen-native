import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { callElapsed } from '../calls/presentation.ts';

const output = await build({
  stdin: {
    contents: readFileSync(new URL('../calls/return-layout.ios.tsx', import.meta.url), 'utf8'),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'call-return-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
type Element = { type: string; props: Record<string, any> };
function fixture(minimized: boolean, reduced = false, mode = 'account') {
  let shows = 0;
  const jsx = (type: string, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    'react/jsx-runtime': { jsx, jsxs: jsx },
    react: { useSyncExternalStore: (_subscribe: unknown, snapshot: () => number) => snapshot() },
    'react-native': { View: 'View', Text: 'Text', Pressable: 'Pressable' },
    'react-native-safe-area-context': {
      SafeAreaProvider: 'SafeAreaProvider',
      useSafeAreaInsets: () => ({ top: 62, bottom: 34, left: 0, right: 0 }),
    },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: 'AnimatedView' },
      useReducedMotion: () => reduced,
      ReduceMotion: { Never: 'never' },
      FadeIn: {
        duration: (duration: number) => ({
          reduceMotion: (policy: string) => ({ duration, policy }),
        }),
      },
    },
    'expo-symbols': { SymbolView: 'Symbol' },
    './provider': {
      useNativeCalls: () => ({
        activeCall: { minimized, state: 'connected', connected_at_ms: 1000 },
        showActiveCall: async () => {
          shows += 1;
        },
      }),
    },
    './presentation.ts': { callElapsed },
    '../theme': { useTheme: () => ({ colors: { accent: '#07f', onAccent: '#fff' } }) },
    '../core/provider': { useClient: () => ({ mode }) },
    '../i18n/index.ts': { t: (key: string) => key },
  };
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(output.outputFiles[0]!.text, {
    exports: module.exports,
    module,
    require(name: string) {
      assert.ok(name in ports, `Unexpected ${name}`);
      return ports[name];
    },
  });
  const tree = module.exports.CallReturnLayout({ children: 'ChatCanvas' }) as Element;
  return { tree, shows: () => shows };
}

test('minimized call reserves a real slot before the measured remaining safe area and returns on tap', async () => {
  const { tree, shows } = fixture(true);
  const [bar, body] = tree.props.children as Element[];
  assert.equal(bar.type, 'AnimatedView');
  assert.equal(bar.props.style.paddingTop, 62);
  const button = bar.props.children as Element;
  assert.equal(button.props.style({ pressed: false }).minHeight, 44);
  assert.equal(button.props.accessibilityRole, 'button');
  assert.equal(body.type, 'SafeAreaProvider');
  assert.equal(body.props.children, 'ChatCanvas');
  button.props.onPress();
  await Promise.resolve();
  assert.equal(shows(), 1);
  assert.equal(bar.props.entering.duration, 200);
});

test('visible call and demo reserve no banner height; reduced motion uses a gentle opacity entrance', () => {
  assert.equal(fixture(false).tree.props.children[0], false);
  assert.equal(fixture(true, false, 'demo').tree.props.children[0], false);
  const bar = fixture(true, true).tree.props.children[0] as Element;
  assert.equal(bar.props.entering.duration, 150);
  assert.equal(bar.props.entering.policy, 'never'); // fade remains gentle; no position transform
});
