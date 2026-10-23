import assert from 'node:assert/strict';
import { runInNewContext } from 'node:vm';
import { SensitiveCacheWork } from '../media/cache-work.ts';
import { VoiceDraftOwner, frozenVoicePreview } from '../media/voice-draft-owner.ts';
import * as voiceEdit from '../media/voice-edit.ts';
import { VoiceReviewOwner } from '../media/voice-review-owner.ts';
import type { VoiceSession } from '../media/voice-session.ts';
import { VoiceSessionFailure, type VoiceNativePort } from '../media/voice-session.ts';
import * as voiceSuggestions from '../media/voice-suggestions.ts';
import * as voiceTrimPreview from '../media/voice-trim-preview.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { telegramAudio } from './telegram-audio-presentation.ts';
import type { files } from './voice-controls-compile.ts';
import { compiled } from './voice-controls-compile.ts';

export type Element = { type: unknown; props: Record<string, any> };
export type Slot = {
  value?: any;
  deps?: readonly unknown[];
  cleanup?: () => void;
  effect?: () => void | (() => void);
};
export function component(name: (typeof files)[number], ports: Record<string, unknown>) {
  let slots: Slot[] = [];
  type Instance = {
    slots: Slot[];
    focused: boolean;
    focusEffect?: () => void | (() => void);
    focusCleanup?: () => void;
  };
  let instance: Instance;
  let cursor = 0;
  let effects: (() => void)[] = [];
  const next = () => (slots[cursor++] ??= {});
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const slot = next();
    if (!slot.deps || !deps.every((value, index) => Object.is(value, slot.deps![index]))) {
      slot.value = read();
      slot.deps = deps;
    }
    return slot.value;
  };
  const effect = (read: () => void | (() => void), deps: readonly unknown[]) => {
    const slot = next();
    if (slot.deps && deps.every((value, index) => Object.is(value, slot.deps![index]))) return;
    slot.deps = deps;
    slot.effect = read;
    effects.push(() => {
      slot.cleanup?.();
      slot.cleanup = read() ?? undefined;
    });
  };
  const jsx = (type: unknown, props: Element['props']) => ({ type, props });
  const module = { exports: {} as Record<string, (props: Element['props']) => Element> };
  const dependencies = {
    react: {
      useState(initial: unknown) {
        const slot = next();
        if (!Object.hasOwn(slot, 'value'))
          slot.value = typeof initial === 'function' ? initial() : initial;
        return [
          slot.value,
          (value: unknown) => {
            slot.value = typeof value === 'function' ? value(slot.value) : value;
          },
        ];
      },
      useRef(initial: unknown) {
        const slot = next();
        return (slot.value ??= { current: initial });
      },
      useMemo: memo,
      useCallback: (callback: unknown, deps: readonly unknown[]) => memo(() => callback, deps),
      useEffect: effect,
      useLayoutEffect: effect,
      useSyncExternalStore: (_subscribe: unknown, snapshot: () => unknown) => snapshot(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'react-native': {
      Text: 'Text',
      View: 'View',
      ScrollView: 'ScrollView',
      ActivityIndicator: 'ActivityIndicator',
      Platform: { OS: 'ios' },
      AppState: { currentState: 'active', addEventListener: () => ({ remove() {} }) },
    },
    'expo-router': {
      useFocusEffect(read: () => void | (() => void)) {
        const owner = instance;
        effect(() => {
          owner.focusEffect = read;
          if (owner.focused) owner.focusCleanup = read() ?? undefined;
          return () => {
            owner.focusCleanup?.();
            owner.focusCleanup = undefined;
            owner.focusEffect = undefined;
          };
        }, [read]);
      },
    },
    'expo-crypto': { randomUUID: () => 'synthetic-voice-id' },
    './voice-draft-owner': { VoiceDraftOwner, frozenVoicePreview },
    './voice-session': { VoiceSessionFailure },
    '@expo/ui': { Host: 'Host' },
    '@expo/ui/swift-ui': Object.fromEntries(
      ['Button', 'DisclosureGroup', 'HStack', 'Slider', 'Text', 'VStack'].map((key) => [key, key]),
    ),
    '@expo/ui/swift-ui/modifiers': new Proxy(
      {},
      { get: (_, key) => (value: unknown) => [key, value] },
    ),
    'react-native-gesture-handler': {
      GestureDetector: 'GestureDetector',
      usePanGesture: (options: unknown) => options,
      useTapGesture: (options: unknown) => options,
      useExclusiveGestures: (...gestures: unknown[]) => gestures,
    },
    'react-native-reanimated': {
      __esModule: true,
      default: { View: 'AnimatedView' },
      useSharedValue: (initial: number) => ({ value: initial, set() {} }),
      useAnimatedStyle: (read: () => unknown) => read(),
      withSpring: (value: unknown) => value,
      withTiming: (value: unknown) => value,
      Easing: {
        ease: 'ease',
        inOut: (value: unknown) => value,
        bezier: (...values: number[]) => values,
      },
    },
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en', locale: 'en' },
    '../theme': {
      useTheme: () => ({ colors: {}, scheme: 'light', presentation: telegramPresentation('day') }),
    },
    '../components/primitives': { NativeButton: 'NativeButton', IconButton: 'IconButton' },
    '../components/composer-action': { ComposerAction: 'ComposerAction' },
    './voice-record-button': { VoiceRecordButton: 'VoiceRecordButton' },
    '../core/telegram-audio-presentation': { telegramAudio },
    './audio-progress': { AudioWaveform: 'AudioWaveform' },
    './AudioClip': {
      AudioClip: 'AudioClip',
      AudioClipControls: 'AudioClipControls',
      clipTime: String,
      pauseAudioPlayback() {},
    },
    './voice-preview-playback': {
      VoicePreviewPlayback: 'VoicePreviewPlayback',
      useVoicePreviewPlayback: () => null,
    },
    './voice-waveform.ios': { VoiceWaveform: 'VoiceWaveform' },
    './voice-words.ios': { VoiceWords: 'VoiceWords' },
    './voice-edit.ts': voiceEdit,
    './voice-edit': voiceEdit,
    './voice-suggestions.ts': voiceSuggestions,
    './voice-trim-preview': voiceTrimPreview,
    ...ports,
  };
  runInNewContext(compiled[name]!, {
    module,
    exports: module.exports,
    require(path: string) {
      assert.ok(Object.hasOwn(dependencies, path), `unmocked native import ${path}`);
      return dependencies[path as keyof typeof dependencies];
    },
  });
  const render = Object.values(module.exports)[0]!;
  function mount() {
    const owner: Instance = { slots: [], focused: true };
    return {
      render(props: Element['props']) {
        instance = owner;
        slots = owner.slots;
        cursor = 0;
        effects = [];
        const tree = render(props);
        effects.forEach((commit) => commit());
        return tree;
      },
      blur() {
        owner.focused = false;
        owner.focusCleanup?.();
        owner.focusCleanup = undefined;
      },
      focus() {
        owner.focused = true;
        owner.focusCleanup = owner.focusEffect?.() ?? undefined;
      },
      replayEffects() {
        const committed = owner.slots.filter((slot) => slot.effect);
        for (const slot of committed) {
          slot.cleanup?.();
          slot.cleanup = undefined;
        }
        for (const slot of committed) slot.cleanup = slot.effect!() ?? undefined;
      },
      unmount() {
        owner.slots.forEach((slot) => slot.cleanup?.());
      },
      fork: mount,
    };
  }
  return mount();
}
export function nodes(tree: unknown): Element[] {
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (!tree || typeof tree !== 'object' || !('props' in tree)) return [];
  const element = tree as Element;
  return [element, ...nodes(element.props.children)];
}
export function find(tree: Element, type: string, label?: string): Element {
  const node = nodes(tree).find(
    (node) => node.type === type && (label === undefined || node.props.label === label),
  );
  assert.ok(node, `missing ${type} ${label ?? ''}`);
  return node;
}
export const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
export function voicePort(): VoiceNativePort {
  let renders = 0;
  return {
    prepare: async () => ({
      id: 'synthetic-voice',
      uri: 'file:///voice/original.m4a',
      sampleRate: 1000,
      frameCount: 4000,
      durationMs: 4000,
      levels: [0.2, 0.8],
    }),
    render: async (_id, ranges) => {
      const durationMs = ranges.reduce((sum, range) => sum + range.endFrame - range.startFrame, 0);
      return {
        uri: `file:///voice/render-${++renders}.m4a`,
        frameCount: durationMs,
        durationMs,
        mime: 'audio/mp4',
        levels: [0.2, 0.8],
      };
    },
    transcribe: async () => ({ status: 'unavailable', text: '', words: [] }),
    releaseRender: async () => {},
    cancel: async () => {},
    release: async () => {},
  };
}
export function recording(editable = false, platform: 'ios' | 'web' = 'ios') {
  let stopFailure = false;
  let restoreFailure = false;
  let confirmationFailure = false;
  let statusFailure = false;
  let ineffectiveStop = false;
  let releasedCalls = 0;
  let permission: Promise<{ granted: boolean }> | undefined;
  let preparation: Promise<void> | undefined;
  let restoration: Promise<void> | undefined;
  let permissionCalls = 0;
  let prepares = 0;
  let records = 0;
  const events: string[] = [];
  const discarded: string[] = [];
  const sent: number[] = [];
  const appListeners = new Set<(state: string) => void>();
  const cleanups = new Set<() => Promise<void>>();
  const work = new SensitiveCacheWork();
  function makeRecorder(name: string) {
    const state = { isRecording: false, durationMillis: 4000 };
    let released = false;
    let finalized = true;
    let uriAvailable = true;
    let recorded = false;
    let stops = 0;
    function admitted() {
      if (!released) return;
      ++releasedCalls;
      throw new Error('synthetic released recorder');
    }
    return {
      state,
      isFinalized: () => finalized,
      publishUriOnStop: () => (uriAvailable = false),
      stops: () => stops,
      pause() {
        events.push(`pause:${name}`);
        state.isRecording = false;
      },
      release() {
        released = true;
        finalized = true;
        state.isRecording = false;
      },
      sdk: {
        pause() {
          admitted();
          events.push(`pause:${name}`);
          state.isRecording = false;
        },
        get uri() {
          admitted();
          return uriAvailable ? `file:///voice/${name}.m4a` : null;
        },
        getStatus() {
          admitted();
          if (statusFailure) {
            statusFailure = false;
            throw new Error('synthetic initial status failure');
          }
          if (confirmationFailure && stops > 0 && !state.isRecording) {
            confirmationFailure = false;
            throw new Error('synthetic stop confirmation failure');
          }
          return state;
        },
        async prepareToRecordAsync() {
          admitted();
          ++prepares;
          events.push(`prepare:${name}`);
          await preparation;
          if (!released) finalized = false;
        },
        record() {
          admitted();
          events.push(`record:${name}`);
          ++records;
          recorded = true;
          finalized = false;
          state.durationMillis = 4000;
          state.isRecording = true;
        },
        async stop() {
          admitted();
          ++stops;
          events.push(`stop:${name}`);
          if (stopFailure) throw new Error('synthetic recorder stop failure');
          if (platform === 'web' && !recorded)
            throw new Error('synthetic Web MediaRecorder inactive');
          if (ineffectiveStop) {
            ineffectiveStop = false;
            return;
          }
          state.isRecording = false;
          finalized = true;
          uriAvailable = true;
          // SDK58 resets duration on stop. A retry must retain the pre-stop metadata.
          state.durationMillis = 0;
        },
      },
    };
  }
  const recorder = makeRecorder('capture');
  let activeRecorder = recorder;
  const voice = voicePort();
  const prepareVoice = voice.prepare;
  voice.prepare = async (uri) => {
    events.push('copy');
    assert.equal(recorder.isFinalized(), true, 'editor must never copy an unfinished M4A');
    return prepareVoice(uri);
  };
  const inform = async () => {};
  const appState = {
    currentState: 'active',
    addEventListener(_name: string, listener: (state: string) => void) {
      appListeners.add(listener);
      return { remove: () => appListeners.delete(listener) };
    },
  };
  const store = {
    driver: {
      media: {
        voiceEditor: editable ? voice : undefined,
        sendVoice: async (value: { durationMs: number }) => sent.push(value.durationMs),
      },
    },
    registerDeviceCleanup: (cleanup: () => Promise<void>) => {
      cleanups.add(cleanup);
      return () => cleanups.delete(cleanup);
    },
    refresh: async () => {},
  };
  const app = component('MediaComposer', {
    'react-native': {
      Text: 'Text',
      View: 'View',
      ScrollView: 'ScrollView',
      Platform: { OS: platform },
      AppState: appState,
    },
    'expo-document-picker': {},
    'expo-audio': {
      setIsAudioActiveAsync: async (_active: boolean) => {},
      AudioModule: {
        requestRecordingPermissionsAsync: () => {
          ++permissionCalls;
          events.push('permission');
          return permission ?? Promise.resolve({ granted: true });
        },
      },
      RecordingPresets: { HIGH_QUALITY: {} },
      useAudioRecorder: () => activeRecorder.sdk,
      useAudioRecorderState: () => activeRecorder.state,
      setAudioModeAsync: async (mode: { allowsRecording: boolean }) => {
        events.push(`mode:${mode.allowsRecording}`);
        if (mode.allowsRecording === false) {
          await restoration;
          if (restoreFailure) throw new Error('synthetic audio mode restore failure');
        }
      },
    },
    '../components/dialogs': { useDialogs: () => ({ inform }) },
    '../core/provider': {
      useClient: () => ({ store }),
    },
    './cache-files': {
      discardCacheCopy: (uri?: string) => {
        if (uri) {
          events.push('discard');
          discarded.push(uri);
        }
      },
    },
    './cache-work': { sensitiveCacheWork: work },
    './AudioClip': {
      AudioClip: 'AudioClip',
      AudioClipControls: 'AudioClipControls',
      clipTime: String,
      pauseAudioPlayback: () => events.push('playback:pause'),
    },
    '../components/composer-action': { ComposerAction: 'ComposerAction' },
    './voice-record-button': { VoiceRecordButton: 'VoiceRecordButton' },
    '../core/telegram-audio-presentation': { telegramAudio },
    './audio-progress': { AudioWaveform: 'AudioWaveform' },
    './voice-review-owner': { VoiceReviewOwner },
    './voice-review': { VoiceReview: 'VoiceReview' },
    './voice-dock': { VoiceDock: 'VoiceDock' },
    './media-sheet': { MediaSheet: 'MediaSheet' },
    './media-actions': { MediaActions: 'MediaActions' },
    './attachment-composer': { useAttachmentComposer: () => ({ canAdd: true }) },
  });
  return {
    state: recorder.state,
    discarded,
    sent,
    events,
    cleanups,
    app,
    render: (props: Element['props'] = {}) => {
      activeRecorder = recorder;
      return app.render({ spaceId: 'synthetic-space', ...props });
    },
    nextMount() {
      const next = makeRecorder('next');
      const mounted = app.fork();
      return {
        render() {
          activeRecorder = next;
          return mounted.render({ spaceId: 'another-synthetic-space' });
        },
        state: next.state,
        unmount: mounted.unmount,
      };
    },
    background(state = 'background') {
      appState.currentState = state;
      appListeners.forEach((listener) => listener(state));
    },
    waitPermission: (value: Promise<{ granted: boolean }> | undefined) => (permission = value),
    waitPrepare: (value: Promise<void> | undefined) => (preparation = value),
    waitRestore: (value: Promise<void> | undefined) => (restoration = value),
    permissionCalls: () => permissionCalls,
    prepares: () => prepares,
    records: () => records,
    pauseRecorder: recorder.pause,
    publishUriOnStop: recorder.publishUriOnStop,
    failRestore: (value: boolean) => (restoreFailure = value),
    failStop: (value: boolean) => (stopFailure = value),
    failConfirmation: () => (confirmationFailure = true),
    failStatus: () => (statusFailure = true),
    ineffectiveStop: () => (ineffectiveStop = true),
    releaseRecorder: recorder.release,
    releasedCalls: () => releasedCalls,
    stops: recorder.stops,
  };
}

export async function stagedPreview(f: ReturnType<typeof recording>) {
  find(f.render(), 'VoiceRecordButton').props.onStart(false);
  await tick();
  await find(f.render(), 'VoiceDock').props.onStop();
  await tick();
  const tree = f.render();
  const preview = find(tree, 'VoicePreviewPlayback');
  const session = preview.props.session as VoiceSession;
  assert.ok(session?.getSendable(), 'the real native prepare/render boundary must be ready');
  return { tree, preview, session };
}

export const interruptions = ['inactive', 'background', 'focus', 'suspension', 'disabled'] as const;
export function interrupt(f: ReturnType<typeof recording>, kind: (typeof interruptions)[number]) {
  if (kind === 'focus') {
    f.app.blur();
    f.app.focus();
  } else if (kind === 'suspension' || kind === 'disabled') {
    f.render(kind === 'suspension' ? { suspended: true } : { disabled: true });
    f.render();
  } else {
    f.background(kind);
    f.background('active');
  }
}

export const auditionLabels = {
  position: 'Original audio position',
  hint: 'Seek in kept audio. Removed audio can be restored by tapping.',
  playFromSelection: 'Play from selection',
  seekBackward: 'Back five seconds in edited audio',
  seekForward: 'Forward five seconds in edited audio',
};
export function auditionWave(props: Element['props']) {
  const waveform = component('voice-waveform.ios', {});
  const initial = waveform.render(props);
  const track = nodes(initial).find((node) => node.props.accessibilityRole === 'adjustable');
  assert.ok(track);
  track.props.onLayout({ nativeEvent: { layout: { width: 244 } } });
  return { waveform, tree: waveform.render(props) };
}
