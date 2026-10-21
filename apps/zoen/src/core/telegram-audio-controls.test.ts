import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as presentation from './telegram-audio-presentation.ts';
import * as composerPresentation from './telegram-composer-presentation.ts';
import * as composerMotion from './telegram-composer-motion.ts';
import { VoiceHoldIntent } from '../media/voice-hold-intent.ts';
import { telegramPresentation } from '../theme/telegram.ts';

type Element = { type: any; props: Record<string, any> };
type Slot = { value?: any; deps?: unknown[]; cleanup?: () => void };
const sources = {
  playback: '../media/AudioClip.tsx',
  waveform: '../media/audio-progress.tsx',
  transcript: '../media/voice-transcript.tsx',
  microphone: '../media/voice-record-button.tsx',
  accessory: '../components/message-context.tsx',
  composer: '../components/message-composer.tsx',
  composerMotion: '../components/composer-action-motion.tsx',
  composerAction: '../components/composer-action.ios.tsx',
} as const;
const compiled = Object.fromEntries(
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
function nodes(tree: any): Element[] {
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (!tree?.props) return [];
  return [tree, ...nodes(tree.props.children)];
}
function fixture(name: keyof typeof sources, dependencies: Record<string, unknown> = {}) {
  const scopes = new Map<string, Slot[]>([['root', []]]);
  let slots = scopes.get('root')!;
  let cursor = 0;
  let effects: (() => void)[] = [];
  const next = () => (slots[cursor++] ??= {});
  const jsx = (type: any, props: Element['props']) => ({ type, props });
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
function interactiveNodes(tree: any): Element[] {
  if (Array.isArray(tree)) return tree.flatMap(interactiveNodes);
  if (!tree?.props || tree.props.accessibilityElementsHidden) return [];
  return [tree, ...interactiveNodes(tree.props.children)];
}
const byLabel = (tree: Element, label: string) => {
  const node = interactiveNodes(tree).find(
    (node) => node.props.accessibilityLabel === label || node.props.label === label,
  );
  assert.ok(node, label);
  return node;
};
const tick = () => new Promise<void>((done) => setImmediate(done));

test('actual playback controls retain play/pause, realseek andrate ports without allocating another player', () => {
  const events: unknown[] = [];
  const status = {
    durationSeconds: 40,
    currentSeconds: 12,
    rate: 1,
    loaded: true,
    loadFailed: false,
    busy: false,
    buffering: false,
    playing: false,
    error: null,
  };
  const playback = {
    subscribe() {},
    getSnapshot: () => status,
    pause: () => events.push('pause'),
    playFrom: async () => {
      events.push('play');
    },
    seekSeconds: async (s: number) => {
      events.push(s);
    },
    setRate: (rate: number) => {
      events.push(rate);
      status.rate = rate;
    },
  };
  const f = fixture('playback');
  let tree = f.render('AudioClipControls', {
    playback,
    compact: true,
    appearance: 'mine',
    levels: [1, 0],
  });
  byLabel(tree, 'media.playAudio').props.onPress();
  status.playing = true;
  tree = f.render('AudioClipControls', {
    playback,
    compact: true,
    appearance: 'mine',
    levels: [1, 0],
  });
  byLabel(tree, 'media.pauseAudio').props.onPress();
  const progress = nodes(tree).find((node) => node.type === 'AudioProgress')!;
  progress.props.onSeek(0.75);
  for (let i = 0; i < 3; i++) {
    tree = f.render('AudioClipControls', { playback, compact: true, appearance: 'mine' });
    byLabel(tree, 'media.playbackRateLabel').props.onPress();
  }
  assert.deepEqual(events, ['play', 'pause', 30, 1.5, 2, 1]);
  assert.equal(byLabel(tree, 'media.pauseAudio').props.style({ pressed: false }).width, 44);
  status.busy = true;
  status.playing = false;
  tree = f.render('AudioClipControls', { playback, compact: true });
  byLabel(tree, 'media.pauseAudio').props.onPress();
  assert.equal(events.at(-1), 'pause');
});

test('waveform gesture preserves timeline yield, exactseek and rejects queued resize/retired callbacks', () => {
  const seeks: number[] = [];
  const f = fixture('waveform');
  const props = {
    levels: [1],
    progress: 0.2,
    duration: 40,
    ink: '#fff',
    compact: true,
    disabled: false,
    onSeek: (at: number) => seeks.push(at),
  };
  let tree = f.render('AudioProgress', props);
  nodes(tree)
    .find((node) => node.props.onLayout)!
    .props.onLayout({ nativeEvent: { layout: { width: 120 } } });
  tree = f.render('AudioProgress', props);
  let [pan, tap] = tree.props.gesture;
  assert.equal(pan.block, 'message-pan');
  assert.deepEqual(Array.from(pan.failOffsetY), [-12, 12]);
  pan.onBegin();
  pan.onDeactivate({ canceled: false, x: 90 });
  pan.onFinalize();
  tap.onBegin();
  tap.onDeactivate({ canceled: false, x: 30 });
  tap.onFinalize();
  assert.deepEqual(seeks, [0.75, 0.25]);
  pan.onBegin();
  tap.onBegin();
  nodes(tree)
    .find((node) => node.props.onLayout)!
    .props.onLayout({ nativeEvent: { layout: { width: 240 } } });
  tree = f.render('AudioProgress', props);
  pan.onDeactivate({ canceled: false, x: 90 });
  tap.onDeactivate({ canceled: false, x: 30 });
  assert.deepEqual(
    seeks,
    [0.75, 0.25],
    'old native callback cannot seek with the previous viewport',
  );
  [pan] = tree.props.gesture;
  pan.onBegin();
  f.unmount();
  pan.onDeactivate({ canceled: false, x: 120 });
  assert.deepEqual(seeks, [0.75, 0.25]);
});

test('actual transcript preserves accepted text, interruption and reduced-motion geometry', () => {
  const f = fixture('transcript');
  const props = { text: 'The accepted transcript', ink: '#fff', message: true };
  let tree = f.render('VoiceTranscript', props);
  const toggle = byLabel(tree, 'media.voiceTranscript');
  assert.equal(toggle.props['aria-expanded'], false);
  toggle.props.onPress();
  tree = f.render('VoiceTranscript', props);
  assert.equal(byLabel(tree, 'media.voiceTranscript').props['aria-expanded'], true);
  assert.ok(
    nodes(tree).some(
      (node) => node.type === 'Text' && node.props.selectable && node.props.children === props.text,
    ),
  );
  const entered = nodes(tree).find((node) => node.props.entering)!;
  assert.equal(entered.props.entering.ms, 200);
  f.theme.reducedMotion = true;
  tree = f.render('VoiceTranscript', props);
  assert.equal(tree.props.layout, undefined);
  assert.equal(
    nodes(tree).some((node) => node.props.entering),
    false,
  );
  byLabel(tree, 'media.voiceTranscript').props.onPress();
  tree = f.render('VoiceTranscript', props);
  assert.equal(
    nodes(tree).some((node) => node.props.selectable),
    false,
  );
});

test('microphone holds190ms, preserves pointer throughbusy, locks once and has an accessible tap alternative', async () => {
  const events: string[] = [];
  const f = fixture('microphone');
  const props = {
    disabled: false,
    active: false,
    locked: false,
    onStart: async (held: boolean) => {
      events.push(`start:${held}`);
      return true;
    },
    onReview: async () => {
      events.push('review');
    },
    onCancel: async () => {
      events.push('cancel');
    },
    onLock: () => events.push('lock'),
    onError: () => events.push('error'),
  };
  let tree = f.render('VoiceRecordButton', props);
  let [pan] = tree.props.gesture;
  assert.equal(pan.activateAfterLongPress, 190);
  pan.onActivate();
  await tick();
  tree = f.render('VoiceRecordButton', { ...props, disabled: true, active: true });
  [pan] = tree.props.gesture;
  assert.equal(pan.enabled, true);
  pan.onUpdate({ translationX: 0, translationY: -111 });
  pan.onDeactivate({
    canceled: false,
    translationX: 0,
    translationY: -111,
    velocityX: 0,
    velocityY: 0,
  });
  assert.deepEqual(events, ['start:true', 'lock']);
  tree = f.render('VoiceRecordButton', { ...props, active: false });
  byLabel(tree, 'media.recordVoiceMessage').props.onAccessibilityAction({
    nativeEvent: { actionName: 'activate' },
  });
  await tick();
  assert.deepEqual(events, ['start:true', 'lock', 'lock', 'start:false']);
  byLabel(tree, 'media.recordVoiceMessage').props.onAccessibilityTap();
  await tick();
  assert.deepEqual(
    events,
    ['start:true', 'lock', 'lock', 'start:false', 'lock', 'start:false'],
    'Fabric default activation uses onAccessibilityTap, while custom actions remain available',
  );
});

test('composer uses the same field for reply/edit/search/caption and retains realAI/arrows/send/voice callbacks', async () => {
  const f = fixture('composer');
  const calls: string[] = [];
  const props: any = {
    spaceId: 'one',
    value: '',
    onChangeText: () => {},
    onBlur: () => {},
    onSend: () => calls.push('send'),
    sending: false,
    context: { entry: { id: 'reply' }, onCancel: () => calls.push('cancel') },
  };
  const controls = (next: any, files?: any) => {
    const owner = nodes(f.render('MessageComposer', next)).find(
      (node) => node.type === 'MediaComposer',
    )!;
    return owner.props.renderControls({
      attachment: { type: 'Attachment', props: {} },
      voice: f.microphone({
        disabled: Boolean(next.editing || next.search),
        active: false,
        locked: false,
        onStart: async (held: boolean) => {
          calls.push(`voice:${held}`);
          return true;
        },
        onReview: async () => calls.push('voice-review'),
        onCancel: async () => calls.push('voice-cancel'),
        onLock: () => calls.push('voice-lock'),
        onError: () => calls.push('voice-error'),
      }),
      files,
    });
  };
  let tree = controls(props);
  const field = nodes(tree).find((node) => node.type === 'ComposerTextField')!;
  assert.equal(nodes(tree).filter((node) => node.type === 'ComposerTextField').length, 1);
  assert.equal(field.props.context, 'one::normal');
  assert.ok(nodes(tree).some((node) => node.type === 'MessageContext'));
  assert.equal(
    interactiveNodes(tree).some((node) => node.props.accessibilityLabel === 'chat.sendMessage'),
    false,
    'retained hidden Send must not masquerade as an available action',
  );
  for (const label of ['chat.sendMessage', 'chat.olderSearchResult', 'chat.newerSearchResult']) {
    const hidden = nodes(tree).find((node) => node.props.accessibilityLabel === label)!;
    assert.equal(hidden.props.accessible, false);
    assert.equal(hidden.props.accessibilityElementsHidden, true);
    assert.equal(hidden.props.importantForAccessibility, 'no-hide-descendants');
  }
  const oldVoiceTap = byLabel(tree, 'media.recordVoiceMessage').props.onAccessibilityTap;
  oldVoiceTap();
  await tick();
  tree = controls({ ...props, value: 'text', editing: true });
  assert.equal(
    nodes(tree).find((node) => node.type === 'ComposerTextField')!.props.context,
    'one::edit',
  );
  assert.ok(nodes(tree).some((node) => node.props.testID === 'record-voice'));
  assert.equal(
    interactiveNodes(tree).some((node) => node.props.testID === 'record-voice'),
    false,
    'the actual recording gesture stays mounted while editing retires its hit target',
  );
  oldVoiceTap();
  byLabel(tree, 'chat.saveEdit').props.onPress();
  const search = {
    status: 'none',
    state: {
      query: 'question',
      completed: true,
      loading: false,
      error: null,
      hits: [],
      cursor: null,
      index: -1,
    },
    onAsk: () => calls.push('AI'),
    onMove: (v: string) => calls.push(v),
    onClose: () => calls.push('close'),
    onSearch: () => calls.push('search'),
  };
  tree = controls({ ...props, search });
  const oldAsk = byLabel(tree, 'chat.askAboutChat').props.onPress;
  oldAsk();
  search.state.hits = [{}, {}, {}] as never;
  search.state.index = 1;
  tree = controls({ ...props, search });
  assert.ok(byLabel(tree, 'chat.olderSearchResult'));
  assert.ok(byLabel(tree, 'chat.newerSearchResult'));
  byLabel(tree, 'chat.olderSearchResult').props.onPress();
  byLabel(tree, 'chat.newerSearchResult').props.onPress();
  assert.equal(
    interactiveNodes(tree).some((node) => node.props.accessibilityLabel === 'chat.askAboutChat'),
    false,
    'the retained AI action is retired when real results replace it',
  );
  oldAsk();
  tree = controls(props, {
    value: 'caption',
    position: 1,
    total: 2,
    editable: true,
    remaining: 2,
    scope: {},
    onChange: () => {},
    onSend: () => calls.push('files'),
  });
  const caption = nodes(tree).find((node) => node.type === 'ComposerTextField')!;
  assert.equal(caption.type, field.type);
  assert.equal(caption.props.value, 'caption');
  const oldAttachmentSend = byLabel(tree, 'media.attachmentSendCount').props.onPress;
  oldAttachmentSend();
  controls(props);
  oldAttachmentSend();
  assert.deepEqual(calls, ['voice-lock', 'voice:false', 'send', 'AI', 'older', 'newer', 'files']);
});

test('reply/edit accessory uses the pinned45pt frame, source typography and real Cancel for unavailable entries too', () => {
  const f = fixture('accessory');
  let cancelled = 0;
  for (const editing of [false, true]) {
    const tree = f.render('MessageContext', {
      editing,
      entry: { id: 'quote', author: { name: 'Ana' }, text: 'Original accepted message' },
      onCancel: () => cancelled++,
    });
    assert.equal(tree.props.style.height, 45);
    assert.equal(
      tree.props.style.paddingLeft + 8,
      55,
      'the composer already owns8pt of the viewport',
    );
    const title = nodes(tree).find((node) => node.type === 'Text')!;
    assert.equal(title.props.style.fontSize, 15);
    assert.equal(title.props.style.fontWeight, '500');
    const icon = nodes(tree).find((node) => node.props.entering?.kind === 'zoom-in')!;
    assert.equal(icon.props.entering.ms, 200);
    assert.equal(icon.props.entering.initial.transform[0].scale, 0.001);
    byLabel(tree, 'chat.cancel').props.onPress();
  }
  f.theme.reducedMotion = true;
  const unavailable = f.render('MessageContext', { onCancel: () => cancelled++ });
  assert.ok(nodes(unavailable).some((node) => node.props.children === 'chat.messageUnavailable'));
  assert.equal(
    nodes(unavailable).some((node) => node.props.entering),
    false,
  );
  byLabel(unavailable, 'chat.cancel').props.onPress();
  assert.equal(cancelled, 3);
});
