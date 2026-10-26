import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { VoiceHoldIntent } from '../media/voice-hold-intent.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import * as presentation from './telegram-audio-presentation.ts';
import * as composerMotion from './telegram-composer-motion.ts';
import * as composerPresentation from './telegram-composer-presentation.ts';
import { composerOwnerFixture } from './composer-owner.fixtures.ts';
import { homeOwnerFor } from '../home/access.ts';
import { createRef } from 'react';

export type Element = { type: any; props: Record<string, any> };
export type Slot = { value?: any; deps?: unknown[]; cleanup?: () => void };
export const sources = {
  playback: '../media/AudioClip.tsx',
  waveform: '../media/audio-progress.tsx',
  transcript: '../media/voice-transcript.tsx',
  microphone: '../media/voice-record-button.tsx',
  accessory: '../components/message-context.tsx',
  composer: '../components/message-composer.tsx',
  composerMotion: '../components/composer-action-motion.tsx',
  composerAction: '../components/composer-action.ios.tsx',
} as const;
export const compiled = Object.fromEntries(
  await Promise.all(
    Object.entries(sources).map(async ([key, path]) => {
      const result = await build({
        stdin: { contents: readFileSync(new URL(path, import.meta.url), 'utf8'), loader: 'tsx' },
        bundle: true,
        write: false,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        plugins: [
          {
            name: 'native-ports',
            setup(b) {
              b.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return [key, result.outputFiles[0]!.text];
    }),
  ),
);
export function nodes(tree: any): Element[] {
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (!tree?.props) return [];
  return [tree, ...nodes(tree.props.children)];
}
export function fixture(name: keyof typeof sources, dependencies: Record<string, unknown> = {}) {
  const client = composerOwnerFixture();
  const scopes = new Map<string, Slot[]>([['root', []]]);
  let slots = scopes.get('root')!;
  let cursor = 0;
  let effects: (() => void)[] = [];
  const next = () => (slots[cursor++] ??= {});
  const jsx = (type: any, props: Element['props']) => {
    if (type === 'ComposerTextField') props.bindSend?.(props.onSend);
    return { type, props };
  };
  const effect = (read: () => any, deps?: unknown[]) => {
    const slot = next();
    if (
      deps &&
      slot.deps?.length === deps.length &&
      deps.every((v, i) => Object.is(v, slot.deps![i]))
    )
      return;
    slot.deps = deps;
    effects.push(() => {
      slot.cleanup?.();
      slot.cleanup = read();
    });
  };
  const motion = (kind: string) => ({
    duration: (ms: number) => ({
      kind,
      ms,
      easing(curve: unknown) {
        return { ...this, curve };
      },
      withInitialValues(values: unknown) {
        return { ...this, initial: values };
      },
      withTargetValues(values: unknown) {
        return { ...this, target: values };
      },
    }),
  });
  const theme = {
    presentation: telegramPresentation('night'),
    colors: { surface: 'surface', secondary: 'secondary', danger: 'danger', text: 'text' },
    scheme: 'dark',
    reducedMotion: false,
  };
  const ports: Record<string, any> = {
    react: {
      createRef,
      createContext: (value: unknown) => ({ fixtureContext: true, value }),
      useContext: (context: { value: unknown }) => context.value,
      useRef(value: any) {
        const slot = next();
        return (slot.value ??= { current: value });
      },
      useState(initial: any) {
        const slot = next();
        if (!('value' in slot)) slot.value = typeof initial === 'function' ? initial() : initial;
        return [
          slot.value,
          (value: any) => {
            slot.value = typeof value === 'function' ? value(slot.value) : value;
          },
        ];
      },
      useEffect: effect,
      useLayoutEffect: effect,
      useCallback(callback: unknown, deps: unknown[]) {
        const slot = next();
        if (!slot.deps || deps.some((value, index) => !Object.is(value, slot.deps![index]))) {
          slot.deps = deps;
          slot.value = callback;
        }
        return slot.value;
      },
      useSyncExternalStore: (_: unknown, read: () => any) => read(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react-native': {
      View: 'View',
      Text: 'Text',
      Pressable: 'Pressable',
      ActivityIndicator: 'ActivityIndicator',
    },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: 'AnimatedView' },
      useSharedValue: (initial: any) => {
        const slot = next();
        return (slot.value ??= {
          value: initial,
          get() {
            return this.value;
          },
          set(value: any) {
            this.value = value;
          },
        });
      },
      useAnimatedStyle: (read: () => any) => read(),
      withTiming: (value: any) => value,
      ReduceMotion: { System: 'system' },
      Easing: { ease: 'ease', inOut: (v: any) => v, bezier: (...v: number[]) => v },
      FadeIn: motion('fade-in'),
      FadeOut: motion('fade-out'),
      ZoomIn: motion('zoom-in'),
      ZoomOut: motion('zoom-out'),
      LinearTransition: {
        duration: (ms: number) => ({
          easing: (curve: any) => ({ ms, curve, reduceMotion: () => ({ ms, curve }) }),
        }),
      },
    },
    'react-native-gesture-handler': {
      GestureDetector: 'GestureDetector',
      usePanGesture: (config: any) => ({ kind: 'pan', ...config }),
      useTapGesture: (config: any) => ({ kind: 'tap', ...config }),
      useExclusiveGestures: (...gestures: any[]) => gestures,
    },
    'expo-glass-effect': {
      GlassContainer: 'GlassContainer',
      GlassView: 'GlassView',
      isLiquidGlassAvailable: () => true,
    },
    'expo-haptics': { selectionAsync: async () => {} },
    '../theme': { useTheme: () => theme },
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en' },
    '../components/message-interaction': {
      MessagePressable: 'Pressable',
      useMessagePan: () => 'message-pan',
      useMessagePressConsumption: () => () => {},
    },
    '../components/primitives': {
      Icon: 'Icon',
      IconButton: 'IconButton',
      NativeButton: 'NativeButton',
    },
    './primitives': { Icon: 'Icon', IconButton: 'IconButton', NativeButton: 'NativeButton' },
    '../core/telegram-audio-presentation': presentation,
    '../core/telegram-composer-presentation': composerPresentation,
    '../core/telegram-composer-motion': composerMotion,
    '../core/provider': { useClient: () => client },
    '../home/access': { homeOwnerFor },
    '../components/composer-control-surface': {
      ComposerControlSurface: 'ComposerControlSurface',
      ComposerGlyph: 'ComposerGlyph',
    },
    './composer-control-surface': {
      ComposerControlSurface: 'ComposerControlSurface',
      ComposerGlyph: 'ComposerGlyph',
    },
    '../media/attachment-keyboard-context': {
      AttachmentKeyboardProvider: 'AttachmentKeyboardProvider',
      useAttachmentKeyboard: () => null,
    },
    './voice-hold-intent': { VoiceHoldIntent },
    './audio-progress': { AudioProgress: 'AudioProgress' },
    './playback-owner.ts': { mediaPlayback: { pauseAll() {} } },
    './audio-clip-playback.ts': {
      useAudioClipPlayback: () => {
        throw Error('controls must not allocate a player');
      },
    },
    '../core/presentation': { messageText: (entry: any) => entry.text },
    'react-native-safe-area-context': {
      useSafeAreaInsets: () => ({ bottom: 34, top: 62, left: 0, right: 0 }),
    },
    './keyboard-motion': { useKeyboardMotion: () => ({ progress: { value: 0 } }) },
    '../core/conversation-search': { conversationQueryFits: () => true },
    '../media/MediaComposer': { MediaComposer: 'MediaComposer' },
    '../media/attachment-reply-context': { AttachmentReplyContext: 'AttachmentReplyContext' },
    './message-context': { MessageContext: 'MessageContext' },
    './composer-action': { ComposerAction: 'ComposerAction' },
    './composer-glass': {
      ComposerGlassContainer: 'ComposerGlassContainer',
      ComposerGlassSurface: 'ComposerGlassSurface',
    },
    './conversation-composer-access': { ConversationComposerAccess: 'ConversationComposerAccess' },
    './composer-text-field': { ComposerTextField: 'ComposerTextField' },
    ...dependencies,
  };
  const load = (source: keyof typeof sources) => {
    const module = { exports: {} as Record<string, (props: any) => Element> };
    runInNewContext(compiled[source]!, {
      module,
      exports: module.exports,
      process: { env: { EXPO_OS: 'ios' } },
      require: (path: string) => {
        assert.ok(path in ports, path);
        return ports[path];
      },
      Intl,
    });
    return module.exports;
  };
  const actionMotion = load('composerMotion');
  ports['./composer-action-motion'] = actionMotion;
  const action = load('composerAction');
  ports['./composer-action'] = action;
  const microphone = load('microphone');
  const exports = load(name);
  const components = new Set([
    actionMotion.ComposerMotionSlot,
    action.ComposerAction,
    microphone.VoiceRecordButton,
  ]);
  // Run retained native action children with their own hook slots and provider
  // context. Reading all JSX indiscriminately would also find hidden Send/arrows.
  const expand = (tree: any, path = 'root'): any => {
    if (Array.isArray(tree)) return tree.map((child, index) => expand(child, `${path}/${index}`));
    if (!tree?.props) return tree;
    if (tree.type?.fixtureContext) {
      const previous = tree.type.value;
      tree.type.value = tree.props.value;
      const children = expand(tree.props.children, `${path}/context`);
      tree.type.value = previous;
      return { ...tree, props: { ...tree.props, children } };
    }
    if (components.has(tree.type)) {
      const previousSlots = slots;
      const previousCursor = cursor;
      const key = `${path}/${tree.type.name}`;
      if (!scopes.has(key)) scopes.set(key, []);
      slots = scopes.get(key)!;
      cursor = 0;
      const result = tree.type(tree.props);
      slots = previousSlots;
      cursor = previousCursor;
      return expand(result, key);
    }
    if (tree.type === 'MediaComposer')
      return {
        ...tree,
        props: {
          ...tree.props,
          renderControls: (args: any) => {
            effects = [];
            const result = expand(tree.props.renderControls(args), `${path}/controls`);
            effects.forEach((read) => read());
            return result;
          },
        },
      };
    return {
      ...tree,
      props: { ...tree.props, children: expand(tree.props.children, `${path}/children`) },
    };
  };
  return {
    theme,
    microphone: (props: Record<string, unknown>): Element => ({
      type: microphone.VoiceRecordButton,
      props,
    }),
    render(exportName: string, props: any) {
      slots = scopes.get('root')!;
      cursor = 0;
      effects = [];
      const tree = expand(exports[exportName]!(props));
      effects.forEach((read) => read());
      return tree;
    },
    unmount() {
      scopes.forEach((slots) => slots.forEach((slot) => slot.cleanup?.()));
    },
  };
}
export function interactiveNodes(tree: any): Element[] {
  if (Array.isArray(tree)) return tree.flatMap(interactiveNodes);
  if (!tree?.props || tree.props.accessibilityElementsHidden) return [];
  return [tree, ...interactiveNodes(tree.props.children)];
}
export const byLabel = (tree: Element, label: string) => {
  const node = interactiveNodes(tree).find(
    (node) => node.props.accessibilityLabel === label || node.props.label === label,
  );
  assert.ok(node, label);
  return node;
};
export const tick = () => new Promise<void>((done) => setImmediate(done));
