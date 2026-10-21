import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as motionContract from './telegram-composer-motion.ts';
import * as presentation from './telegram-composer-presentation.ts';

const compiled = await Promise.all(
  ['composer-action-motion.tsx', 'composer-action.ios.tsx'].map(async (name) => {
    const output = await build({
      stdin: {
        contents: readFileSync(new URL(`../components/${name}`, import.meta.url), 'utf8'),
        loader: 'tsx',
      },
      bundle: true,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
      plugins: [
        {
          name: 'native-ports',
          setup(builder) {
            builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
          },
        },
      ],
    });
    return output.outputFiles[0]!.text;
  }),
);

type Element = { type: unknown; props: Record<string, any> };
type Timing = { target: number; config: Record<string, any> };
function fixture() {
  type Shared = { value: number; animation?: Timing & { from: number } };
  const shared: Shared[] = [];
  const slots: any[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  const context = { active: true, imageStyle: {} };
  const theme = {
    reducedMotion: false,
    presentation: { chat: { input: { action: '#00f', actionText: '#fff' } } },
  };
  const slot = (initial: () => any) => {
    const i = cursor++;
    return (slots[i] ??= initial());
  };
  const ports: Record<string, any> = {
    react: {
      createContext: () => 'MotionContext',
      useContext: () => context,
      useRef: (value: unknown) => slot(() => ({ current: value })),
      useCallback: (callback: unknown) => slot(() => callback),
      useLayoutEffect: (callback: () => void, deps: unknown[]) => {
        const current = slot(() => ({ deps: undefined }));
        if (!current.deps || deps.some((value, index) => !Object.is(value, current.deps[index]))) {
          current.deps = deps;
          effects.push(callback);
        }
      },
    },
    'react/jsx-runtime': {
      jsx: (type: unknown, props: any) => ({ type, props }),
      jsxs: (type: unknown, props: any) => ({ type, props }),
    },
    'react-native': { Pressable: 'Pressable' },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: 'AnimatedView' },
      Easing: { bezier: (...values: number[]) => values },
      ReduceMotion: { System: 'System' },
      LinearTransition: {
        duration: (duration: number) => ({
          easing: (curve: unknown) => ({
            reduceMotion: (reduce: unknown) => ({ duration, curve, reduce }),
          }),
        }),
      },
      useSharedValue: (initial: number) =>
        slot(() => {
          let current = initial;
          const state = {
            get value(): number {
              return current;
            },
            set value(value: number | Timing) {
              if (typeof value === 'number') {
                current = value;
                state.animation = undefined;
              } else {
                state.animation = { ...value, from: current };
                if (value.config.duration === 0) current = value.target;
              }
            },
            animation: undefined as Shared['animation'],
          };
          Object.assign(state, {
            get: () => state.value,
            set: (value: number | Timing) => {
              state.value = value;
            },
          });
          shared.push(state);
          return state;
        }),
      useAnimatedStyle: (read: () => unknown) => read(),
      withTiming: (target: number, config: Record<string, any>): Timing => ({ target, config }),
    },
    '../core/telegram-composer-motion': motionContract,
    '../core/telegram-composer-presentation': presentation,
    '../theme': { useTheme: () => theme },
    './composer-action-motion': { useComposerActionMotion: () => context },
    './composer-control-surface': { ComposerControlSurface: 'Surface', ComposerGlyph: 'Glyph' },
  };
  const modules = compiled.map((source) => {
    const module = { exports: {} as Record<string, any> };
    runInNewContext(source, {
      exports: module.exports,
      module,
      require: (name: string) => {
        assert.ok(name in ports, name);
        return ports[name];
      },
    });
    return module.exports;
  });
  const render = (read: () => Element) => {
    cursor = 0;
    effects = [];
    const tree = read();
    for (const effect of effects) effect();
    return tree;
  };
  return {
    shared,
    context,
    theme,
    slot: (props: Record<string, any>) =>
      render(() => modules[0]!.ComposerMotionSlot({ children: 'retained-control', ...props })),
    action: (props: Record<string, any> = {}) =>
      render(() =>
        modules[1]!.ComposerAction({
          name: 'arrow.up',
          label: 'Send',
          prominent: true,
          onPress() {},
          ...props,
        }),
      ),
    frame: () => modules[0]!.composerFrameTransition(theme.reducedMotion),
  };
}
const nativeSlot = (tree: Element) => tree.props.children as Element;

test('a hidden retained control is immediately excluded from touch and accessibility during its fade', () => {
  const f = fixture();
  const shown = f.slot({ kind: 'send', active: true });
  assert.equal(nativeSlot(shown).props.pointerEvents, 'box-none');
  const hidden = f.slot({ kind: 'send', active: false });
  assert.equal(hidden.props.value.active, false);
  assert.equal(nativeSlot(hidden).props.pointerEvents, 'none');
  assert.equal(nativeSlot(hidden).props.accessibilityElementsHidden, true);
  assert.equal(nativeSlot(hidden).props.importantForAccessibility, 'no-hide-descendants');
  assert.equal(nativeSlot(hidden).props.children, nativeSlot(shown).props.children);
  assert.equal(
    f.shared[0]!.value,
    1,
    'presentation can still be visible while actions are already retired',
  );
  assert.equal(f.shared[0]!.animation!.target, 0);
});

test('rapid Send reversals retarget current presentation rather than restarting a hidden keyframe', () => {
  const f = fixture();
  f.slot({ kind: 'send', active: false });
  f.slot({ kind: 'send', active: true });
  assert.equal(f.shared[3]!.animation!.from, -22);
  assert.equal(f.shared[4]!.animation!.from, 18);
  f.shared[0]!.value = 0.6;
  f.shared[1]!.value = 0.7;
  f.shared[2]!.value = 7;
  f.shared[3]!.value = -8;
  f.shared[4]!.value = 6;
  f.slot({ kind: 'send', active: false });
  assert.equal(f.shared[0]!.animation!.from, 0.6);
  assert.equal(f.shared[1]!.animation!.from, 0.7);
  assert.equal(f.shared[2]!.animation!.from, 7);
  f.slot({ kind: 'send', active: true });
  assert.equal(f.shared[3]!.animation!.from, -8, 'mid-fade entry must not reset to -22');
  assert.equal(f.shared[4]!.animation!.from, 6);
  assert.equal(f.shared[1]!.animation!.config.duration, 400);
  assert.deepEqual(Array.from(f.shared[1]!.animation!.config.easing), [0.38, 0.7, 0.125, 1]);
  assert.equal(f.shared[0]!.animation!.config.duration, 200);
});

test('retained Send callbacks cannot send a retired caption or act through a hidden control', () => {
  const f = fixture();
  const calls: string[] = [];
  const first = f.action({ onPress: () => calls.push('caption-A') });
  const oldPress = first.props.onPress;
  f.context.active = false;
  const hidden = f.action({ onPress: () => calls.push('normal') });
  assert.equal(hidden.props.disabled, true);
  oldPress();
  assert.equal(calls.length, 0);
  f.context.active = true;
  f.action({ onPress: () => calls.push('caption-B') });
  oldPress();
  assert.deepEqual(calls, ['caption-B']);
  f.action({ disabled: true, onPress: () => calls.push('forbidden') });
  oldPress();
  assert.deepEqual(calls, ['caption-B']);
});

test('ordinary Send uses upstream .75 hold scale and resumes from the displayed scale on release', () => {
  const f = fixture();
  const button = f.action();
  button.props.onPressIn();
  assert.equal(f.shared[0]!.animation!.target, 0.75);
  assert.equal(f.shared[0]!.animation!.config.duration, 400);
  f.shared[0]!.value = 0.86;
  button.props.onPressOut();
  assert.equal(f.shared[0]!.animation!.from, 0.86);
  assert.equal(f.shared[0]!.animation!.target, 1);
  assert.equal(f.shared[0]!.animation!.config.duration, 250);
  f.action({ disabled: true });
  assert.equal(f.shared[0]!.value, 1, 'retiring a held button clears its highlight');
});

test('text moves the retained mic offscreen without inventing a fade; accessory changes use .25s alpha', () => {
  const mic = fixture();
  mic.slot({ kind: 'media', active: true, visible: true });
  const hidden = mic.slot({ kind: 'media', active: false, visible: true });
  assert.equal(mic.shared[0]!.animation!.target, 1);
  assert.equal(mic.shared[2]!.animation!.target, 56);
  assert.equal(nativeSlot(hidden).props.pointerEvents, 'none');
  const accessory = fixture();
  accessory.slot({ kind: 'accessory', active: true });
  accessory.slot({ kind: 'accessory', active: false });
  assert.equal(accessory.shared[0]!.animation!.config.duration, 250);
  assert.equal(accessory.shared[1]!.animation!.config.duration, 250);
});

test('Reduce Motion applies the same terminal frames without an entrance or ongoing movement', () => {
  const f = fixture();
  f.theme.reducedMotion = true;
  f.slot({ kind: 'send', active: false });
  f.slot({ kind: 'send', active: true });
  assert.equal(f.shared[1]!.value, 1);
  assert.equal(f.shared[2]!.value, 0);
  assert.equal(f.shared[3]!.animation!.from, 0);
  assert.equal(f.shared[3]!.animation!.config.duration, 0);
  assert.equal(f.frame().duration, 0);
});

test('the media frame and accessory reserve the original positions while Zoen search keeps its own arrows', () => {
  assert.equal(motionContract.composerMotionTarget('send', false).x, 17);
  assert.equal(motionContract.composerMotionTarget('media', false).x, 56);
  assert.equal(motionContract.composerMotionTarget('accessory', true, true).x, -46);
  for (const [searching, hasSend, hasVoice, expected] of [
    [false, false, true, 46],
    [false, true, true, 0],
    [false, false, false, 0],
    [true, false, true, 96],
    [true, true, true, 0],
  ] as const)
    assert.equal(motionContract.composerExternalWidth({ searching, hasSend, hasVoice }), expected);
});
