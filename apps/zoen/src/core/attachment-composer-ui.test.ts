import { composerVerticalInsets } from './telegram-audio-presentation.ts';
import * as composerPresentation from './telegram-composer-presentation.ts';
import * as composerMotion from './telegram-composer-motion.ts';
import { telegramPresentation, DEFAULT_THEME_CUSTOMIZATION } from '../theme/telegram.ts';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

type Element = { type: unknown; props: Record<string, unknown> };
type Handler = (...args: unknown[]) => unknown;
const source =
  process.env.ZOEN_COMPOSER_UI_SOURCE ??
  new URL('../components/message-composer.tsx', import.meta.url);
const output = await build({
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
const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const fieldPath = fileURLToPath(new URL('../components/composer-text-field.tsx', import.meta.url));
type Platform = 'ios' | 'android' | 'web';
async function compileField(platform: Platform) {
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
const fieldOutputs = new Map(
  await Promise.all(
    (['ios', 'android', 'web'] as const).map(
      async (platform) => [platform, await compileField(platform)] as const,
    ),
  ),
);
function fixture(platform: Platform = 'ios', fontScale = 1, liquidGlass = false) {
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
      useAttachmentKeyboardField: (...args: unknown[]) => keyboardFields.push(args),
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
function find(root: unknown, predicate: (element: Element) => boolean): Element {
  const result = scan(root).find(predicate);
  assert.ok(result, 'actual composer element missing');
  return result;
}
function scan(root: unknown): Element[] {
  if (Array.isArray(root)) return root.flatMap(scan);
  if (!root || typeof root !== 'object' || !('props' in root)) return [];
  const element = root as Element;
  return [element, ...scan(element.props.children)];
}
const byId = (id: string) => (node: Element) => node.props.testID === id;

test('native attachment mode uses the real message field and send button without posting the normal text draft', () => {
  const f = fixture();
  const root = f.render(f.file);
  const input = find(root, byId('message-input'));
  assert.equal(input.props.value, '');
  assert.equal(input.props.editable, true);
  assert.equal(
    f.measurements.at(-1)!.text,
    '',
    'actual compiled field requests native caption fitting',
  );
  (input.props.onChangeText as Handler)('First photo caption');
  const send = find(root, byId('send-attachments'));
  assert.equal(send.props.disabled, false, 'an attachment is sendable without a caption');
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['caption:First photo caption', 'send-files']);
  assert.equal(find(f.render(), byId('message-input')).props.value, 'My unsent normal text');
});

for (const platform of ['web', 'android'] as const) {
  test(`${platform} mounts, changes mode and detaches the real input ref without iOS tag lookup or fitting`, () => {
    const f = fixture(platform);
    const inputRef = { current: null as unknown };
    const first = find(f.render(undefined, { inputRef }), byId('message-input'));
    assert.equal(inputRef.current, f.nativeField);
    assert.equal(first.props.value, 'My unsent normal text');
    const editing = find(
      f.render(undefined, { inputRef, editing: true, value: 'Edit\nsecond line' }),
      byId('message-input'),
    );
    assert.equal(inputRef.current, f.nativeField, 'mode changes preserve the mounted editor');
    assert.equal(editing.props.ref, first.props.ref);
    assert.equal(editing.props.value, 'Edit\nsecond line');
    const restored = find(f.render(undefined, { inputRef }), byId('message-input'));
    assert.equal(restored.props.value, 'My unsent normal text');
    (restored.props.onChangeText as Handler)('Still editable');
    (restored.props.onContentSizeChange as Handler)({
      nativeEvent: { contentSize: { width: 264, height: 74 } },
    });
    assert.deepEqual(f.calls, ['normal:Still editable']);
    assert.equal(f.nativeHandleReads(), 0);
    assert.equal(f.measurements.length, 0);
    f.unmount();
    assert.equal(inputRef.current, null);
    assert.equal(f.nativeHandleReads(), 0, 'detaching must not invoke native tag lookup either');
  });
}

test('ambiguous attachment retry keeps the caption readonly and sends only through the batch owner', () => {
  const f = fixture();
  const root = f.render({
    ...f.file,
    editable: false,
    targetFrozen: true,
    value: 'Exact attempted caption',
    retry: true,
    remaining: 1,
  });
  assert.equal(find(root, byId('message-input')).props.editable, false);
  const send = find(root, byId('send-attachments'));
  assert.equal(send.props.label, 'media.attachmentsRetry');
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['send-files']);
});

test('attachment reply UI displays the frozen batch target rather than a later normal-draft reply', () => {
  const f = fixture();
  const scope = { ...f.file.scope, replyTo: 'captured-original' };
  const context = { entry: { id: 'later-reply' }, onCancel: () => {} };
  const root = f.render({ ...f.file, scope, targetFrozen: true }, { context });
  const quote = find(root, (node) => node.type === 'AttachmentReplyContext');
  assert.equal((quote.props.scope as typeof scope).replyTo, 'captured-original');
  assert.equal(quote.props.onCancel, undefined);
  assert.equal(
    scan(root).some((node) => node.type === 'MessageContext'),
    false,
  );
  assert.equal(
    find(f.render(undefined, { context }), (node) => node.type === 'MessageContext').props.entry,
    context.entry,
  );
});

test('search takes over the message field while attachment drafts stay outside its action path', () => {
  const f = fixture();
  const search = {
    state: { active: true, completed: true, loading: false, hits: [{}], index: 0, cursor: null },
    status: '1/1',
    onClose: () => {},
    onMove: () => {},
    onAsk: () => {},
    onSearch: () => {},
  };
  const root = f.render(f.file, { search, value: 'Search text' });
  assert.equal(find(root, byId('message-input')).props.value, 'Search text');
  assert.equal(find(root, byId('message-input')).props.autoCorrect, false);
  assert.equal(find(root, byId('message-input')).props.autoCapitalize, 'none');
  assert.equal(scan(root).some(byId('send-attachments')), false);
  assert.ok(find(root, byId('older-search-result')));
  const normal = find(f.render(), byId('message-input'));
  assert.equal(normal.props.autoCorrect, undefined, 'Normal messages keep native correction');
  assert.equal(normal.props.autoCapitalize, undefined, 'Normal messages keep sentence casing');
});

test('a sent file caption can be cleared without allowing empty ordinary text edits', () => {
  const f = fixture();
  const emptyCaption = f.render(undefined, { value: '', editing: true, allowEmptyEdit: true });
  assert.equal(find(emptyCaption, byId('send-message')).props.disabled, false);
  assert.equal(
    (find(emptyCaption, byId('message-input')).props.style as { height?: number }[]).at(-1)!.height,
    40,
  );
  const emptyText = f.render(undefined, { value: '', editing: true });
  assert.equal(find(emptyText, byId('send-message')).props.disabled, true);
});

test('an unread native draft cannot be overwritten by an initially empty field; retry stays visible', () => {
  const f = fixture();
  const draft = { ready: false, error: true, onRetry: () => f.calls.push('retry-draft') };
  const root = f.render(undefined, { value: '', draft });
  assert.equal(find(root, byId('message-input')).props.editable, false);
  const status = f.renderRoot({ draft });
  (find(status, byId('retry-message-draft')).props.onPress as Handler)();
  assert.deepEqual(f.calls, ['retry-draft']);
  const loaded = f.render(undefined, { draft: { ...draft, ready: true } });
  assert.equal(find(loaded, byId('message-input')).props.editable, true);
  assert.equal(find(loaded, byId('message-input')).props.value, 'My unsent normal text');
});

test('iOS keeps the exact send intent inside the field while empty audio and search navigation stay outside', () => {
  const f = fixture();
  const surface = (tree: Element) => find(tree, byId('message-composer-field-surface'));
  let tree = f.render();
  assert.equal(
    scan(surface(tree)).some(byId('send-message')),
    true,
    'The real Send callback must be rendered inside the native field surface',
  );
  const send = find(surface(tree), byId('send-message'));
  (send.props.onPress as Handler)();
  assert.deepEqual(f.calls, ['send-normal']);
  assert.equal(scan(tree).filter(byId('send-message')).length, 1);

  tree = f.render(undefined, { value: '' });
  assert.equal(
    scan(surface(tree)).some(byId('send-message')),
    true,
    'Send remains mounted while hidden',
  );
  assert.equal(
    find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'send').props
      .active,
    false,
  );
  assert.equal(
    find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'media').props
      .children,
    'Microphone',
  );
  assert.equal(
    scan(surface(tree)).some((node) => node.props.children === 'Microphone'),
    false,
  );

  tree = f.render(f.file);
  (find(surface(tree), byId('send-attachments')).props.onPress as Handler)();
  assert.deepEqual(f.calls, ['send-normal', 'send-files']);
  assert.equal(scan(tree).some(byId('send-message')), false);
  const moves: unknown[] = [];
  const search = {
    state: {
      active: true,
      completed: true,
      loading: false,
      hits: [{}],
      index: 0,
      cursor: null,
      query: 'term',
    },
    status: '1/1',
    onClose: () => {},
    onMove: (direction: unknown) => moves.push(direction),
    onAsk: () => moves.push('ai'),
    onSearch: () => {},
  };
  tree = f.render(f.file, { search, value: 'term' });
  assert.equal(scan(surface(tree)).some(byId('older-search-result')), false);
  assert.ok(find(tree, byId('older-search-result')).props.disabled);
  assert.ok(find(tree, byId('newer-search-result')).props.disabled);
  assert.equal(scan(tree).some(byId('send-attachments')), false);

  tree = f.render(f.file, {
    search: { ...search, state: { ...search.state, hits: [], index: -1 } },
    value: 'term',
  });
  (find(surface(tree), byId('ask-conversation')).props.onPress as Handler)();
  assert.deepEqual(moves, ['ai']);
  assert.deepEqual(f.calls, ['send-normal', 'send-files']);
});

test('the mounted iOS field uses one font for display and scoped native fitting at larger system text sizes', async () => {
  const f = fixture('ios', 1.8);
  const inputRef = { current: null as unknown };
  const root = f.render(undefined, {
    inputRef,
    context: { entry: { id: 'reply' }, onCancel: () => {} },
  });
  const field = find(root, byId('message-input'));
  const textStyle = (field.props.style as Record<string, unknown>[])[0]!;
  assert.equal(textStyle.fontSize, 17);
  assert.equal(textStyle.minHeight, 40);
  assert.equal(textStyle.maxHeight, 160);
  assert.equal(textStyle.paddingTop, 9.5);
  assert.equal(textStyle.paddingBottom, 9.5);
  await new Promise<void>((resolve) => setImmediate(resolve));
  assert.ok(f.measurements.length);
  assert.ok(f.measurements.every((request) => Math.abs(request.fontSize - 30.6) < 1e-9));
  assert.equal(inputRef.current, f.nativeField);
  assert.equal(
    find(root, (node) => node.type === 'MessageContext').props.entry &&
      scan(root).filter((node) => node.type === 'MessageContext').length,
    1,
  );
  const outer = find(f.renderRoot({}), (node) => node.type === 'AnimatedView');
  const outerStyle = Object.assign({}, ...(outer.props.style as object[]));
  assert.equal(
    outerStyle.paddingBottom,
    34,
    'the native viewport already applies the keyboard correction',
  );
  assert.equal(outerStyle.paddingTop, 0);
  assert.equal(outerStyle.paddingHorizontal, 8);
  f.render(f.file, { inputRef });
  assert.equal(
    inputRef.current,
    f.nativeField,
    'caption mode must keep the same real editor handle',
  );
});

test('Android and Web keep the existing outside-send layout and composer sizing', () => {
  for (const platform of ['android', 'web'] as const) {
    const f = fixture(platform);
    const tree = f.render();
    const surface = find(tree, byId('message-composer-field-surface'));
    assert.equal(scan(surface).some(byId('send-message')), false);
    assert.equal(scan(tree).filter(byId('send-message')).length, 1);
    const style = (find(tree, byId('message-input')).props.style as Record<string, unknown>[])[0]!;
    assert.equal(style.minHeight, 52);
    assert.equal(style.fontSize, 16);
    assert.equal(f.measurements.length, 0);
  }
});

test('a theme font change measures the displayed font and rejects late fitting of the old font', async () => {
  const f = fixture('ios', 1.5);
  f.deferFits();
  let tree = f.render();
  assert.ok(f.pendingFits.length);
  const first = f.pendingFits.map((fit) => fit);
  assert.ok(first.every((fit) => fit.request.fontSize === 25.5));
  f.theme.presentation = telegramPresentation('night', {
    ...DEFAULT_THEME_CUSTOMIZATION,
    textSize: 23,
  });
  tree = f.render();
  const actual = find(tree, (node) => node.type === 'TextInput');
  const flattened = Object.assign({}, ...(actual.props.style as any[]));
  assert.equal(flattened.fontSize, 23);
  const latest = f.pendingFits.at(-1)!;
  assert.equal(latest.request.fontSize, 34.5);
  for (const fit of first) fit.resolve({ ...fit.request, height: 154 });
  await new Promise((done) => setImmediate(done));
  tree = f.render();
  assert.notEqual(
    Object.assign({}, ...(find(tree, (node) => node.type === 'TextInput').props.style as any[]))
      .height,
    154,
  );
  latest.resolve({ ...latest.request, height: 88 });
  await new Promise((done) => setImmediate(done));
  tree = f.render();
  assert.equal(
    Object.assign({}, ...(find(tree, (node) => node.type === 'TextInput').props.style as any[]))
      .height,
    88,
  );
});

const styleOf = (node: Element): Record<string, any> =>
  Object.assign({}, ...[node.props.style].flat().filter(Boolean));
const parentOf = (tree: unknown, child: Element): Element =>
  find(tree, (node) => [node.props.children].flat().includes(child));

test('compiled iOS renderer keeps a full-width field and reserves the source inset only once beside absolute controls', () => {
  const f = fixture();
  let tree = f.render();
  const field = find(tree, byId('message-input'));
  const surface = find(tree, byId('message-composer-field-surface'));
  const fieldStyle = styleOf(field);
  assert.equal(fieldStyle.fontSize, 17);
  assert.equal(fieldStyle.flex, undefined, 'vertical flex must not collapse multiline fitting');
  assert.equal(fieldStyle.minWidth, 0);
  assert.equal(fieldStyle.width, '100%', 'full-width editor is not shortened by the send slot');
  assert.equal(
    fieldStyle.paddingRight,
    67,
    '32pt mode control measurement +46pt send are reserved once',
  );
  assert.equal(fieldStyle.paddingLeft, 12);
  assert.equal(fieldStyle.paddingTop, 9.5);
  assert.equal(fieldStyle.paddingBottom, 9.5);
  assert.equal(field.props.selectionColor, f.theme.presentation.chat.input.action);
  assert.equal(styleOf(find(tree, byId('message-composer-field-frame'))).flex, 1);
  assert.notEqual(
    styleOf(surface).flexDirection,
    'row',
    'children must not shrink the field horizontally',
  );
  const mode = find(tree, byId('composer-input-mode'));
  assert.equal(styleOf(mode).width, 32);
  assert.equal(styleOf(mode).height, 40);
  assert.deepEqual(styleOf(parentOf(tree, mode)), { position: 'absolute', bottom: 0, right: 4 });
  assert.equal(
    parentOf(tree, mode).props.hasSend,
    true,
    'mode translates with the retained Send slot',
  );
  const send = find(tree, byId('send-message'));
  assert.equal(send.props.minimumHeight, 40);
  assert.deepEqual(styleOf(parentOf(tree, send)), { position: 'absolute', bottom: 0, right: 0 });
  tree = f.render(undefined, { value: '' });
  assert.equal(styleOf(find(tree, byId('message-input'))).paddingRight, 21);
  const emptyMode = find(tree, byId('composer-input-mode'));
  assert.deepEqual(styleOf(parentOf(tree, emptyMode)), {
    position: 'absolute',
    bottom: 0,
    right: 4,
  });
  assert.equal(
    find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'media').props
      .children,
    'Microphone',
  );
  assert.equal(styleOf(find(tree, byId('message-composer-field-frame'))).marginRight, 46);
});

test('compiled field preserves its actual native owner while input-mode, caption, edit and search change', () => {
  const f = fixture();
  const inputRef = { current: null as unknown };
  let tree = f.render(undefined, { inputRef });
  const first = find(tree, byId('message-input'));
  const firstMode = find(tree, byId('composer-input-mode'));
  assert.equal(find(firstMode, (node) => node.type === 'ComposerGlyph').props.name, 'face.smiling');
  (firstMode.props.onPress as Handler)();
  tree = f.render(undefined, { inputRef });
  assert.equal(f.keyboard.kind, 'emoji');
  assert.equal(
    find(find(tree, byId('composer-input-mode')), (node) => node.type === 'ComposerGlyph').props
      .name,
    'keyboard',
  );
  assert.equal(find(tree, byId('message-input')).props.ref, first.props.ref);
  assert.equal(inputRef.current, f.nativeField);
  const cases = [
    { files: f.file, patch: {}, context: 'space::caption:1', value: '' },
    {
      files: undefined,
      patch: { editing: true, value: 'Edited draft' },
      context: 'space::edit',
      value: 'Edited draft',
    },
    {
      files: undefined,
      patch: {
        search: {
          state: {
            active: true,
            completed: true,
            loading: false,
            hits: [{}],
            index: 0,
            cursor: null,
          },
          status: '1/1',
        },
        value: 'term',
      },
      context: 'space::search',
      value: 'term',
    },
    { files: undefined, patch: {}, context: 'space::normal', value: 'My unsent normal text' },
  ];
  for (const current of cases) {
    tree = f.render(current.files, { inputRef, ...current.patch });
    const field = find(tree, byId('message-input'));
    assert.equal(field.props.ref, first.props.ref);
    assert.equal(field.props.value, current.value);
    assert.equal(scan(tree).filter(byId('message-input')).length, 1);
    assert.equal(
      find(tree, (node) => node.type === 'ComposerMotionSlot' && node.props.kind === 'media').props
        .children,
      'Microphone',
      'normal, caption, edit and search retain the exact media gesture path',
    );
    assert.equal(inputRef.current, f.nativeField);
    assert.ok(
      f.keyboardFields.some(([tag, context]) => tag === 101 && context === current.context),
    );
    if (current.context.endsWith('search'))
      assert.equal(parentOf(tree, find(tree, byId('composer-input-mode'))).props.active, false);
  }
  const providerFor = (patch: Record<string, unknown>) =>
    find(f.renderRoot(patch), (node) => node.type === 'AttachmentKeyboardProvider');
  const provider = providerFor({ search: { state: { completed: false } } });
  assert.equal(provider.type, 'AttachmentKeyboardProvider');
  assert.equal(provider.props.scope, 'space:');
  assert.equal(provider.props.enabled, false, 'search retires accessory input authority');
  assert.equal(providerFor({ suspended: true }).props.enabled, false);
  assert.equal(providerFor({}).props.enabled, true);
});

test('compiled source minimum tracks larger named font geometry for field, mode and send without changing the native ref', () => {
  const f = fixture();
  const inputRef = { current: null as unknown };
  const first = find(f.render(undefined, { inputRef }), byId('message-input'));
  f.theme.presentation = telegramPresentation('night', {
    ...DEFAULT_THEME_CUSTOMIZATION,
    textSize: 23,
  });
  const tree = f.render(undefined, { inputRef });
  const field = find(tree, byId('message-input'));
  assert.equal(field.props.ref, first.props.ref);
  assert.equal(styleOf(field).fontSize, 23);
  assert.equal(styleOf(field).minHeight, 47);
  assert.equal(find(tree, byId('message-composer-field-surface')).props.cornerRadius, 23);
  assert.equal(styleOf(find(tree, byId('composer-input-mode'))).height, 47);
  assert.equal(find(tree, byId('send-message')).props.minimumHeight, 47);
  assert.equal(styleOf(field).paddingRight, 67);
  assert.equal(inputRef.current, f.nativeField);
});

test('compiled composer routes its exact field children through the reusable original glass surfaces', () => {
  for (const available of [true, false]) {
    const f = fixture('ios', 1, available);
    const tree = f.render();
    const row = find(tree, byId('message-composer-row'));
    const field = find(tree, byId('message-composer-field-surface'));
    assert.equal(row.type, 'ComposerGlassContainer');
    assert.equal(field.type, 'ComposerGlassSurface');
    assert.equal(field.props.cornerRadius, 20);
    assert.equal(scan(field).filter(byId('message-input')).length, 1);
    assert.equal(
      styleOf(field).backgroundColor,
      undefined,
      'the original renderer owns its fill and stroke',
    );
  }
});

test('the shared composer guards both main and thread content while retaining query-only search', () => {
  for (const threadRootId of [undefined, 'thread-root']) {
    const f = fixture();
    const root = f.renderRoot({ threadRootId });
    assert.equal(root.type, 'ConversationComposerAccess');
    assert.equal(root.props.spaceId, 'space');
    assert.equal(root.props.searching, false);
    assert.equal(
      find(root, (node) => node.type === 'MediaComposer').props.threadRootId,
      threadRootId ?? null,
    );
    const search = f.renderRoot({ search: { state: {} } });
    assert.equal(search.props.searching, true);
  }
});
