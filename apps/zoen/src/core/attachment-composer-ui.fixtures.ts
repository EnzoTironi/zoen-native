import { build } from 'esbuild';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { telegramPresentation } from '../theme/telegram.ts';
import { composerVerticalInsets } from './telegram-audio-presentation.ts';
import * as composerMotion from './telegram-composer-motion.ts';
import * as composerPresentation from './telegram-composer-presentation.ts';
import type { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
import { keyboardFieldHook } from './attachment-keyboard-field.fixtures.ts';

export type Element = { type: unknown; props: Record<string, unknown> };
export type Handler = (...args: unknown[]) => unknown;
export const source =
  process.env.ZOEN_COMPOSER_UI_SOURCE ??
  new URL('../components/message-composer.tsx', import.meta.url);
export const output = await build({
  stdin: { contents: readFileSync(source, 'utf8'), loader: 'tsx' },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'composer-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
export const require = createRequire(import.meta.url);
export const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
export const fieldPath = fileURLToPath(
  new URL('../components/composer-text-field.tsx', import.meta.url),
);
export type Platform = 'ios' | 'android' | 'web';
export async function compileField(platform: Platform) {
  const compiledField = transformSync(readFileSync(fieldPath, 'utf8'), {
    filename: fieldPath,
    configFile: false,
    babelrc: false,
    presets: [require.resolve('babel-preset-expo')],
    caller: {
      name: 'metro',
      platform,
      engine: platform === 'web' ? undefined : 'hermes',
      isDev: false,
      supportsReactCompiler: true,
    },
  }).code;
  const fieldPorts = new Set([
    'react',
    'react/compiler-runtime',
    'react/jsx-runtime',
    'react-native',
    '../../modules/zoen-core/src/ZoenCoreModule',
    '../media/attachment-keyboard-context',
  ]);
  const fieldOutput = await build({
    stdin: { contents: compiledField, resolveDir: dirname(fieldPath), loader: 'js' },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    write: false,
    plugins: [
      {
        name: 'actual-composer-field-native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) =>
            fieldPorts.has(path) || path.startsWith('react-native-web/dist/exports/')
              ? { path, external: true }
              : undefined,
          );
        },
      },
    ],
  });
  return fieldOutput.outputFiles[0]!.text;
}
export const fieldOutputs = new Map(
  await Promise.all(
    (['ios', 'android', 'web'] as const).map(
      async (platform) => [platform, await compileField(platform)] as const,
    ),
  ),
);
export function fixture(
  platform: Platform = 'ios',
  fontScale = 1,
  liquidGlass = false,
  keyboardSession?: AttachmentKeyboardSession,
) {
  type Slot = {
    value?: unknown;
    deps?: readonly unknown[];
    cleanup?: () => void;
    set?: (next: unknown) => void;
  };
  type Effect = () => void | (() => void);
  const slots: Slot[] = [];
  const caches: unknown[][] = [];
  let cursor = 0;
  let cacheCursor = 0;
  let effects: (() => void)[] = [];
  let dirty = false;
  let fieldElement: Element | null = null;
  let nativeHandleReads = 0;
  const measurements: {
    tag: number;
    text: string;
    label: string;
    placeholder: string;
    width: number;
    fontSize: number;
    nonce: number;
  }[] = [];
  const theme = { scheme: 'dark', colors: {}, presentation: telegramPresentation('night') };
  const keyboardFields: unknown[][] = [];
  const keyboard = {
    kind: 'system',
    toggleEmoji: () => {
      keyboard.kind = keyboard.kind === 'emoji' ? 'system' : 'emoji';
    },
  };
  const pendingFits: { request: Record<string, any>; resolve: (value: any) => void }[] = [];
  let deferFits = false;
  const nativeField = { focus() {}, blur() {} };
  const slot = () => (slots[cursor++] ??= {});
  const same = (left?: readonly unknown[], right?: readonly unknown[]) =>
    Boolean(
      left &&
      right &&
      left.length === right.length &&
      left.every((value, index) => Object.is(value, right[index])),
    );
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const current = slot();
    if (!same(current.deps, deps)) {
      current.deps = deps;
      current.value = read();
    }
    return current.value;
  };
  const effect = (read: Effect, deps?: readonly unknown[]) => {
    const current = slot();
    if (same(current.deps, deps)) return;
    current.deps = deps;
    effects.push(() => {
      current.cleanup?.();
      current.cleanup = read() ?? undefined;
    });
  };
  const bindKeyboardField = keyboardSession ? keyboardFieldHook(keyboardSession, effect) : null;
  const fieldModule = {
    exports: {} as { ComposerTextField: (props: Record<string, unknown>) => Element },
  };
  function renderField(props: Record<string, unknown>): Element {
    let result!: Element;
    let passes = 0;
    do {
      assert.ok(++passes <= 5, 'actual field mounting must settle');
      cursor = 0;
      cacheCursor = 0;
      effects = [];
      dirty = false;
      result = fieldModule.exports.ComposerTextField(props);
      assert.equal(result.type, 'TextInput');
      (result.props.ref as (node: unknown) => void)(nativeField);
      (result.props.onLayout as Handler)({ nativeEvent: { layout: { width: 264, height: 52 } } });
      for (const commit of effects) commit();
    } while (dirty);
    fieldElement = result;
    return result;
  }
  const jsx = (type: unknown, props: Record<string, unknown>): Element => {
    if (type === fieldModule.exports.ComposerTextField) return renderField(props);
    if (typeof type === 'function' && type.name === 'ComposerInputAccessory') return type(props);
    return { type, props };
  };
  const scope = {
    mode: 'account',
    identityId: 'me',
    deviceId: 'device',
    spaceId: 'space',
    replyTo: null,
    thread: false,
  };
  const calls: string[] = [];
  const file = {
    scope,
    targetFrozen: false,
    value: '',
    position: 1,
    total: 2,
    editable: true,
    sending: false,
    remaining: 2,
    retry: false,
    onChange: (text: unknown) => calls.push(`caption:${text}`),
    onSend: () => calls.push('send-files'),
  };
  const ports: Record<string, unknown> = {
    react: {
      useRef: (value: unknown) => {
        const current = slot();
        current.value ??= { current: value };
        return current.value;
      },
      useState: (initial: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value'))
          current.value = typeof initial === 'function' ? initial() : initial;
        current.set ??= (next: unknown) => {
          const value = typeof next === 'function' ? next(current.value) : next;
          if (!Object.is(value, current.value)) {
            current.value = value;
            dirty = true;
          }
        };
        return [current.value, current.set];
      },
      useMemo: memo,
      useCallback: (callback: unknown, deps: readonly unknown[]) => memo(() => callback, deps),
      useLayoutEffect: effect,
      useImperativeHandle: (
        ref: { current: unknown } | undefined,
        create: () => unknown,
        deps?: readonly unknown[],
      ) =>
        effect(() => {
          if (ref) ref.current = create();
          return () => {
            if (ref) ref.current = null;
          };
        }, deps),
    },
    'react/compiler-runtime': {
      c: (size: number) =>
        (caches[cacheCursor++] ??= Array(size).fill(Symbol.for('react.memo_cache_sentinel'))),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react-native': {
      Text: 'Text',
      View: 'View',
      Pressable: 'Pressable',
      TextInput: 'TextInput',
      findNodeHandle: () => {
        ++nativeHandleReads;
        assert.equal(platform, 'ios', 'native tag lookup is unavailable outside iOS fitting');
        return 101;
      },
      useWindowDimensions: () => ({ fontScale }),
      StyleSheet: {
        flatten: (style: any): any =>
          Array.isArray(style) ? Object.assign({}, ...style.map((part: any) => part || {})) : style,
      },
    },
    '../../modules/zoen-core/src/ZoenCoreModule': {
      __esModule: true,
      default: {
        measureComposer: async (
          tag: number,
          text: string,
          label: string,
          placeholder: string,
          width: number,
          fontSize: number,
          nonce: number,
        ) => {
          const request = { tag, text, label, placeholder, width, fontSize, nonce };
          measurements.push(request);
          if (deferFits) return new Promise((resolve) => pendingFits.push({ request, resolve }));
          return { tag, nonce, width, height: text ? 74 : 52, fontSize };
        },
      },
    },
    'react-native-safe-area-context': { useSafeAreaInsets: () => ({ top: 59, bottom: 34 }) },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: 'AnimatedView' },
      useAnimatedStyle: (read: () => unknown) => read(),
    },
    'expo-glass-effect': {
      GlassView: 'GlassView',
      GlassContainer: 'GlassContainer',
      isLiquidGlassAvailable: () => liquidGlass,
    },
    '../media/attachment-keyboard-context': {
      AttachmentKeyboardProvider: 'AttachmentKeyboardProvider',
      useAttachmentKeyboard: () => keyboard,
      useAttachmentKeyboardField: (...args: Parameters<NonNullable<typeof bindKeyboardField>>) => {
        keyboardFields.push(args);
        bindKeyboardField?.(...args);
      },
    },
    './composer-control-surface': { ComposerGlyph: 'ComposerGlyph' },
    '../core/telegram-composer-presentation': composerPresentation,
    './keyboard-motion': { useKeyboardMotion: () => ({ progress: { value: 0 } }) },
    '../theme': { useTheme: () => theme },
    '../i18n/index.ts': { t: (key: string) => key },
    '../core/conversation-search': { conversationQueryFits: () => true },
    '../core/telegram-audio-presentation': { composerVerticalInsets },
    '../media/MediaComposer': { MediaComposer: 'MediaComposer' },
    '../media/attachment-reply-context': { AttachmentReplyContext: 'AttachmentReplyContext' },
    './message-context': { MessageContext: 'MessageContext' },
    './composer-action': { ComposerAction: 'ComposerAction' },
    './composer-glass': {
      ComposerGlassContainer: 'ComposerGlassContainer',
      ComposerGlassSurface: 'ComposerGlassSurface',
    },
    './composer-action-motion': {
      ComposerMotionSlot: 'ComposerMotionSlot',
      composerFrameTransition: () => 'SourceFrameTransition',
      useComposerActionMotion: () => ({ active: true }),
    },
    '../core/telegram-composer-motion': composerMotion,
    './conversation-composer-access': { ConversationComposerAccess: 'ConversationComposerAccess' },
    './primitives': { NativeButton: 'NativeButton' },
  };
  // The real Expo Web transform rewrites RN imports to individual default exports.
  for (const [name, value] of Object.entries(ports['react-native'] as Record<string, unknown>))
    ports[`react-native-web/dist/exports/${name}`] = { __esModule: true, default: value };
  runInNewContext(fieldOutputs.get(platform)!, {
    exports: fieldModule.exports,
    module: fieldModule,
    process: { env: { EXPO_OS: platform } },
    requestAnimationFrame: () => 1,
    cancelAnimationFrame: () => {},
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  ports['./composer-text-field'] = fieldModule.exports;
  const module = {
    exports: {} as { MessageComposer: (props: Record<string, unknown>) => Element },
  };
  runInNewContext(output.outputFiles[0]!.text, {
    exports: module.exports,
    module,
    process: { env: { EXPO_OS: platform } },
    require: (path: string) => {
      assert.ok(path in ports, path);
      return ports[path];
    },
  });
  const props = {
    spaceId: 'space',
    value: 'My unsent normal text',
    sending: false,
    onChangeText: (text: unknown) => calls.push(`normal:${text}`),
    onBlur: () => {},
    onSend: () => calls.push('send-normal'),
  };
  function render(files: unknown = undefined, patch: Record<string, unknown> = {}) {
    const root = module.exports.MessageComposer({ ...props, ...patch });
    const media = find(root, (node) => node.type === 'MediaComposer');
    return (media.props.renderControls as Handler)({
      attachment: null,
      voice: 'Microphone',
      files,
    }) as Element;
  }
  return {
    file,
    calls,
    measurements,
    theme,
    keyboard,
    keyboardFields,
    pendingFits,
    deferFits: () => {
      deferFits = true;
    },
    nativeField,
    nativeHandleReads: () => nativeHandleReads,
    unmount: () => {
      assert.ok(fieldElement);
      (fieldElement.props.ref as (node: unknown) => void)(null);
      for (const current of slots) current.cleanup?.();
    },
    render,
    renderRoot: (patch: Record<string, unknown>) =>
      module.exports.MessageComposer({ ...props, ...patch }),
  };
}
export function find(root: unknown, predicate: (element: Element) => boolean): Element {
  const result = scan(root).find(predicate);
  assert.ok(result, 'actual composer element missing');
  return result;
}
export function scan(root: unknown): Element[] {
  if (Array.isArray(root)) return root.flatMap(scan);
  if (!root || typeof root !== 'object' || !('props' in root)) return [];
  const element = root as Element;
  return [element, ...scan(element.props.children)];
}
export const byId = (id: string) => (node: Element) => node.props.testID === id;

export const styleOf = (node: Element): Record<string, any> =>
  Object.assign({}, ...[node.props.style].flat().filter(Boolean));
export const parentOf = (tree: unknown, child: Element): Element =>
  find(tree, (node) => [node.props.children].flat().includes(child));
