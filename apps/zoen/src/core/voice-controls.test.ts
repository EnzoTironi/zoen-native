import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { SensitiveCacheWork } from '../media/cache-work.ts';
import { VoiceReviewOwner } from '../media/voice-review-owner.ts';
import { VoiceSession, type VoiceNativePort } from '../media/voice-session.ts';
import * as voiceEdit from '../media/voice-edit.ts';
import * as voiceSuggestions from '../media/voice-suggestions.ts';
import * as voiceTrimPreview from '../media/voice-trim-preview.ts';

type Element = { type: unknown; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
const files = ['MediaComposer', 'voice-review.ios', 'voice-waveform.ios', 'voice-dock'] as const;
const compiled = Object.fromEntries(
  await Promise.all(
    files.map(async (name) => {
      const output = await build({
        stdin: {
          contents: readFileSync(new URL(`../media/${name}.tsx`, import.meta.url), 'utf8'),
          loader: 'tsx',
        },
        bundle: true,
        platform: 'node',
        format: 'cjs',
        jsx: 'automatic',
        write: false,
        plugins: [
          {
            name: 'native-voice-ports',
            setup(builder) {
              builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
            },
          },
        ],
      });
      return [name, output.outputFiles[0]!.text];
    }),
  ),
);
function component(name: (typeof files)[number], ports: Record<string, unknown>) {
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
    },
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en', locale: 'en' },
    '../theme': { useTheme: () => ({ colors: {}, scheme: 'light' }) },
    '../components/primitives': { NativeButton: 'NativeButton', IconButton: 'IconButton' },
    '../components/composer-action': { ComposerAction: 'ComposerAction' },
    './AudioClip': { AudioClip: 'AudioClip', clipTime: String, pauseAudioPlayback() {} },
    './voice-waveform.ios': { VoiceWaveform: 'VoiceWaveform' },
    './voice-words.ios': { VoiceWords: 'VoiceWords' },
    './voice-edit.ts': voiceEdit,
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
      unmount() {
        owner.slots.forEach((slot) => slot.cleanup?.());
      },
      fork: mount,
    };
  }
  return mount();
}
function nodes(tree: unknown): Element[] {
  if (Array.isArray(tree)) return tree.flatMap(nodes);
  if (!tree || typeof tree !== 'object' || !('props' in tree)) return [];
  const element = tree as Element;
  return [element, ...nodes(element.props.children)];
}
function find(tree: Element, type: string, label?: string): Element {
  const node = nodes(tree).find(
    (node) => node.type === type && (label === undefined || node.props.label === label),
  );
  assert.ok(node, `missing ${type} ${label ?? ''}`);
  return node;
}
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function voicePort(): VoiceNativePort {
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
function recording(editable = false, platform: 'ios' | 'web' = 'ios') {
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
    '../components/composer-action': { ComposerAction: 'ComposerAction' },
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

test('audio mode restore failure does not poison stop or prevent retrying discard', async () => {
  const f = recording();
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  assert.equal(f.state.isRecording, true);
  f.failRestore(true);
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /audio mode restore failure/);
  assert.equal(f.state.isRecording, false);
  assert.equal(f.discarded.length, 0);
  // A second failed restoration must still retain ownership, rather than claiming discard.
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.equal(f.discarded.length, 0);
  f.failRestore(false);
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
  assert.equal(f.stops(), 1);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  await find(f.render(), 'VoiceDock').props.onStop();
  assert.equal(f.stops(), 2);
});

test('failed recorder stop keeps the live recording owned until a confirmed stop retry', async () => {
  const f = recording();
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failStop(true);
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.equal(f.state.isRecording, true);
  assert.equal(f.discarded.length, 0);
  f.failStop(false);
  await find(f.render(), 'VoiceDock').props.onCancel();
  assert.equal(f.state.isRecording, false);
  assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
  assert.equal(f.stops(), 2);
});

test('retry reconciles a completed native stop after its confirmation read failed', async () => {
  const f = recording();
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failConfirmation();
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /confirmation failure/);
  assert.equal(f.state.isRecording, false);
  assert.equal(f.discarded.length, 0);
  find(f.render(), 'IconButton', 'media.cancelAttachment').props.onPress();
  await tick();
  assert.equal(f.stops(), 1, 'a stopped recorder must not receive a second native stop');
  assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
});

test('a retained cleanup after unmount never calls the SDK-released recorder', async () => {
  const f = recording();
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  const cleanup = [...f.cleanups][0]!;
  f.releaseRecorder();
  f.app.unmount();
  await cleanup();
  assert.equal(f.releasedCalls(), 0);
  assert.ok(f.discarded.includes('file:///voice/capture.m4a'));
});

test('the actual dock exposes non-destructive audio-mode recovery beside a playable preview', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failRestore(true);
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /restore failure/);
  await tick();
  const props = find(f.render(), 'VoiceDock').props;
  const session = props.session;
  const captured = session.getSendable();
  const dock = component('voice-dock', {});
  const tree = dock.render(props);
  assert.ok(find(tree, 'AudioClip'));
  f.failRestore(false);
  find(tree, 'IconButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const recovered = find(f.render(), 'VoiceDock').props;
  assert.equal(recovered.failure, null);
  assert.strictEqual(recovered.session, session);
  assert.strictEqual(session.getSendable().preview, captured.preview);
  assert.deepEqual(f.discarded, []);
  await recovered.onSend(session.getSendable());
  assert.deepEqual(f.sent, [4000]);
  await tick();
});

test('a system-paused recorder is finalized before its retained voice is prepared', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.pauseRecorder();
  f.background();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(f.stops(), 1);
  assert.ok(f.events.indexOf('stop:capture') < f.events.indexOf('copy'));
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  assert.deepEqual(f.sent, [], 'interruption retains a draft and never posts it');
  f.background('active');
  await dock.props.onSend(dock.props.session.getSendable());
  assert.deepEqual(f.sent, [4000]);
});

test('the Web recorder URI published only by stop is retained as one sendable review', async () => {
  const f = recording(false, 'web');
  f.publishUriOnStop();
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  const recordingDock = find(f.render(), 'VoiceDock');
  assert.equal(recordingDock.props.uri, undefined);
  await recordingDock.props.onStop();
  const tree = f.render();
  assert.equal(nodes(tree).filter((node) => node.type === 'VoiceDock').length, 1);
  const review = find(tree, 'VoiceDock');
  assert.equal(review.props.uri, 'file:///voice/capture.m4a');
  assert.equal(review.props.durationMs, 4000);
  assert.equal(f.stops(), 1);
  await review.props.onSend();
  assert.deepEqual(f.sent, [4000]);
});

test('unsupported Web prepared cancellation cannot create a shared microphone lock for a new route', async () => {
  const f = recording(false, 'web');
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  interrupt(f, 'focus');
  prepare.resolve();
  await tick();
  assert.equal(f.records(), 0);
  assert.ok(f.stops() > 0, 'the actual Web contract rejects stop before ever recording');
  const next = f.nextMount();
  // Keep the failed owner alive: another Web route still owns an independent
  // MediaRecorder, even before the old route has unmounted or cleaned up.
  find(next.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  assert.equal(next.state.isRecording, true);
  assert.equal(f.records(), 1);
  f.releaseRecorder();
  f.app.unmount();
  await find(next.render(), 'VoiceDock').props.onCancel();
  next.unmount();
});

test('failed preparation restores the session without finalizing an unprepared recorder or locking retry', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  prepare.reject(new Error('synthetic prepare failure'));
  await tick();
  assert.equal(f.records(), 0);
  assert.equal(f.stops(), 0);
  assert.equal(f.events.filter((event) => event === 'mode:false').length, 1);
  f.waitPrepare(undefined);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  assert.equal(f.records(), 1);
  await find(f.render(), 'VoiceDock').props.onCancel();
});

test('failed paused finalization retains ownership and retries before editor copy', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.pauseRecorder();
  f.failStop(true);
  f.background();
  await tick();
  assert.equal(f.events.includes('copy'), false);
  assert.deepEqual(f.discarded, []);
  f.failStop(false);
  f.background('active');
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(f.stops(), 2);
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  await dock.props.onCancel();
});

test('post-stop status failure retains original duration and does not stop twice on retry', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failConfirmation();
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /confirmation failure/);
  assert.equal(f.state.durationMillis, 0, 'the real SDK no longer exposes pre-stop duration');
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.durationMs, 4000);
  assert.equal(f.stops(), 1);
  await dock.props.onCancel();
});

test('a failed initial status read keeps the source owned for a later successful stop', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failStatus();
  await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /initial status failure/);
  assert.equal(f.stops(), 0);
  assert.equal(f.events.includes('copy'), false);
  assert.deepEqual(f.discarded, []);
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(dock.props.durationMs, 4000);
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  await dock.props.onCancel();
});

test('a resolved stop with native capture still active re-arms finalization for a real retry', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.ineffectiveStop();
  await assert.rejects(
    find(f.render(), 'VoiceDock').props.onStop(),
    /media.theRecordingOnThisDeviceCouldNotBe/,
  );
  assert.equal(f.events.includes('copy'), false);
  assert.equal(f.state.isRecording, true);
  await find(f.render(), 'VoiceDock').props.onRetry();
  await tick();
  const dock = find(f.render(), 'VoiceDock');
  assert.equal(f.stops(), 2);
  assert.equal(f.state.isRecording, false);
  assert.equal(dock.props.session.getSnapshot().phase, 'ready');
  assert.equal(dock.props.durationMs, 4000);
  await dock.props.onCancel();
});

test('two microphone presses before React commits admit only one permission and preparation', async () => {
  const f = recording();
  const permission = deferred<{ granted: boolean }>();
  f.waitPermission(permission.promise);
  const mic = find(f.render(), 'ComposerAction', 'media.recordVoiceMessage');
  mic.props.onPress();
  mic.props.onPress();
  assert.equal(find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.disabled, true);
  await tick();
  assert.equal(f.permissionCalls(), 1);
  permission.resolve({ granted: true });
  await tick();
  assert.equal(f.prepares(), 1);
  assert.equal(f.records(), 1);
  await find(f.render(), 'VoiceDock').props.onCancel();
});

const interruptions = ['inactive', 'background', 'focus', 'suspension', 'disabled'] as const;
function interrupt(f: ReturnType<typeof recording>, kind: (typeof interruptions)[number]) {
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
for (const kind of interruptions) {
  test(`permission pending across ${kind} cannot revive without a fresh microphone press`, async () => {
    const f = recording();
    const permission = deferred<{ granted: boolean }>();
    f.waitPermission(permission.promise);
    find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
    await tick();
    interrupt(f, kind);
    permission.resolve({ granted: true });
    await tick();
    assert.equal(f.prepares(), 0);
    assert.equal(f.records(), 0);
    assert.equal(f.events.includes('mode:true'), false);
    find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
    await tick();
    assert.equal(f.records(), 1, 'a new explicit intent can record after cleanup');
    await find(f.render(), 'VoiceDock').props.onCancel();
  });
  test(`native preparation pending across ${kind} finalizes and discards instead of starting`, async () => {
    const f = recording();
    const prepare = deferred<void>();
    f.waitPrepare(prepare.promise);
    find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
    await tick();
    assert.equal(f.prepares(), 1);
    interrupt(f, kind);
    prepare.resolve();
    await tick();
    assert.equal(f.records(), 0);
    assert.equal(f.stops(), 1);
    assert.deepEqual(f.discarded, ['file:///voice/capture.m4a']);
    assert.equal(f.events.filter((event) => event === 'mode:false').length, 1);
    assert.ok(f.events.indexOf('stop:capture') < f.events.indexOf('mode:false'));
    find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
    await tick();
    assert.equal(f.records(), 1);
    await find(f.render(), 'VoiceDock').props.onCancel();
  });
}

test('unmount during preparation drains audio restoration without touching the released SharedObject', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  const cleanup = [...f.cleanups][0]!;
  f.releaseRecorder();
  f.app.unmount();
  const drained = cleanup();
  prepare.resolve();
  await drained;
  assert.equal(f.records(), 0);
  assert.equal(f.releasedCalls(), 0);
  assert.equal(f.events.filter((event) => event === 'mode:false').length, 1);
});

test('unmount while permission is pending never prepares or restores an unused audio session', async () => {
  const f = recording();
  const permission = deferred<{ granted: boolean }>();
  f.waitPermission(permission.promise);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.releaseRecorder();
  f.app.unmount();
  permission.resolve({ granted: true });
  await tick();
  assert.equal(f.prepares(), 0);
  assert.equal(f.records(), 0);
  assert.equal(f.releasedCalls(), 0);
  assert.equal(f.events.includes('mode:true'), false);
  assert.equal(f.events.includes('mode:false'), false);
});

for (const intent of ['device cleanup', 'Cancel'] as const) {
  test(`${intent} during native preparation retires the pending microphone intent before drain`, async () => {
    const f = recording(true);
    const prepare = deferred<void>();
    f.waitPrepare(prepare.promise);
    find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
    await tick();
    let cleanup: Promise<void> | undefined;
    if (intent === 'device cleanup') cleanup = [...f.cleanups][0]!();
    else find(f.render(), 'IconButton', 'media.cancelAttachment').props.onPress();
    prepare.resolve();
    await cleanup;
    await tick();
    assert.equal(f.records(), 0);
    assert.equal(f.stops(), 1);
    assert.equal(f.events.includes('copy'), false);
    assert.ok(f.discarded.includes('file:///voice/capture.m4a'));
    assert.equal(
      nodes(f.render()).some((node) => node.type === 'VoiceDock'),
      false,
    );
    find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
    await tick();
    assert.equal(f.records(), 1, 'only a new explicit press can start after cancellation');
    await find(f.render(), 'VoiceDock').props.onCancel();
  });
}

test('a failed same-composer restoration can recover without deadlocking its next recording', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failRestore(true);
  interrupt(f, 'focus');
  prepare.resolve();
  await tick();
  assert.equal(f.records(), 0);
  assert.equal(find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.disabled, false);
  f.failRestore(false);
  find(f.render(), 'NativeButton', 'media.voiceTryAgain').props.onPress();
  await tick();
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  assert.equal(f.records(), 1);
  await find(f.render(), 'VoiceDock').props.onCancel();
});

test('new route explicitly recovers departed cleanup before admission and exposes pending immediately', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failRestore(true);
  f.releaseRecorder();
  f.app.unmount();
  prepare.resolve();
  await tick();
  const next = f.nextMount();
  const restore = deferred<void>();
  f.waitRestore(restore.promise);
  f.failRestore(false);
  const mic = find(next.render(), 'ComposerAction', 'media.recordVoiceMessage');
  mic.props.onPress();
  mic.props.onPress();
  assert.equal(
    find(next.render(), 'ComposerAction', 'media.recordVoiceMessage').props.disabled,
    true,
  );
  await tick();
  assert.equal(f.permissionCalls(), 1, 'no new preparation can overtake an old restore');
  restore.resolve();
  await tick();
  assert.equal(f.records(), 1);
  assert.equal(next.state.isRecording, true);
  assert.equal(f.releasedCalls(), 0);
  const newRecord = f.events.indexOf('record:next');
  await tick();
  assert.equal(f.events.slice(newRecord + 1).includes('mode:false'), false);
  await find(next.render(), 'VoiceDock').props.onCancel();
  next.unmount();
});

test('new route cleanup failure stays retryable through its own visible recovery control', async () => {
  const f = recording();
  const prepare = deferred<void>();
  f.waitPrepare(prepare.promise);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  f.failRestore(true);
  f.releaseRecorder();
  f.app.unmount();
  prepare.resolve();
  await tick();
  const next = f.nextMount();
  find(next.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  assert.equal(f.records(), 0);
  const retry = find(next.render(), 'NativeButton', 'media.voiceTryAgain');
  assert.equal(
    retry.props.disabled,
    false,
    'a departed owner cannot leave an opaque permanent lock',
  );
  f.failRestore(false);
  retry.props.onPress();
  await tick();
  find(next.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  assert.equal(f.records(), 1);
  assert.equal(f.releasedCalls(), 0);
  await find(next.render(), 'VoiceDock').props.onCancel();
  next.unmount();
});

test('Send cannot bypass failed audio restoration and retries preserve the exact capture', async () => {
  for (const editable of [false, true]) {
    const f = recording(editable);
    find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
    await tick();
    f.failRestore(true);
    await assert.rejects(find(f.render(), 'VoiceDock').props.onStop(), /restore failure/);
    await tick();
    const props = find(f.render(), 'VoiceDock').props;
    const capture = props.session?.getSendable();
    const pending = props.onSend(capture);
    if (editable) await assert.rejects(pending, /restore failure/);
    else await pending;
    assert.deepEqual(f.sent, []);
    assert.deepEqual(f.discarded, []);
    assert.equal(f.stops(), 1);
    if (editable) assert.strictEqual(props.session.getSendable().preview, capture.preview);
    f.failRestore(false);
    await props.onSend(capture);
    assert.deepEqual(f.sent, [4000]);
    assert.equal(f.stops(), 1);
    await tick();
  }
});

test('precision slider blocks an already-rendered Send handler before React commits', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const sent: number[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = {
    session,
    initiallyEditing: true,
    onSend: async (value: { preview: { durationMs: number } }) =>
      sent.push(value.preview.durationMs),
  };
  const tree = review.render(props);
  const staleSend = find(tree, 'NativeButton', 'media.send');
  const staleDone = find(tree, 'NativeButton', 'media.voiceDoneEditing');
  const waveProps = find(tree, 'VoiceWaveform').props;
  const wave = waveform.render(waveProps);
  find(wave, 'Slider').props.onValueChange(0.25);
  staleDone.props.onPress();
  staleSend.props.onPress();
  await tick();
  assert.deepEqual(sent, [], 'the canonical old audio must not escape via a stale rendered button');
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, true);
  assert.ok(find(review.render(props), 'VoiceWaveform'), 'stale Done must not hide pending edits');
  assert.equal(
    nodes(review.render(props)).some((node) => node.type === 'AudioClip'),
    false,
  );
  const changed = waveform.render(waveProps);
  find(changed, 'Button', 'media.voiceDiscardTrim').props.onPress();
  find(review.render(props), 'NativeButton', 'media.send').props.onPress();
  await tick();
  assert.deepEqual(sent, [4000]);
  await session.close();
});

test('pending trim blocks parent send and stale sheet collapse until explicit discard', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  await find(f.render(), 'VoiceDock').props.onStop();
  await tick();
  find(f.render(), 'VoiceDock').props.onExpand();
  const parent = f.render();
  const sheet = find(parent, 'MediaSheet');
  const collapse = find(parent, 'IconButton', 'media.voiceCollapse');
  const reviewProps = find(parent, 'VoiceReview').props;
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const waveProps = find(review.render(reviewProps), 'VoiceWaveform').props;
  find(waveform.render(waveProps), 'Slider').props.onValueChange(0.25);
  collapse.props.onPress();
  sheet.props.onDismiss();
  assert.equal(find(f.render(), 'MediaSheet').props.visible, true);
  assert.equal(find(f.render(), 'MediaSheet').props.locked, true);
  await assert.rejects(reviewProps.onSend(reviewProps.session.getSendable()));
  assert.deepEqual(f.sent, []);
  find(waveform.render(waveProps), 'Button', 'media.voiceDiscardTrim').props.onPress();
  assert.equal(find(f.render(), 'MediaSheet').props.locked, false);
  await reviewProps.onSend(reviewProps.session.getSendable());
  assert.deepEqual(f.sent, [4000]);
  await tick();
});

test('canonical draft changes invalidate old previews without letting delayed callbacks unlock new ones', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const sent: number[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = {
    session,
    initiallyEditing: true,
    onSend: async (value: { preview: { durationMs: number } }) =>
      sent.push(value.preview.durationMs),
  };
  const old = find(review.render(props), 'VoiceWaveform').props;
  find(waveform.render(old), 'Slider').props.onValueChange(0.25);
  session.edit({ type: 'trim', range: { startFrame: 0, endFrame: 3500 } });
  await tick();
  const current = find(review.render(props), 'VoiceWaveform').props;
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, false);
  find(waveform.render(current), 'Slider').props.onValueChange(0.2);
  old.onTrimPreviewChange(old.draft, false);
  old.onTrimPreviewChange(old.draft, true);
  old.onEdit({ type: 'trim', range: { startFrame: 0, endFrame: 1000 } });
  assert.strictEqual(session.getSnapshot().draft, current.draft);
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, true);
  find(waveform.render(current), 'Button', 'media.voiceDiscardTrim').props.onPress();
  find(review.render(props), 'NativeButton', 'media.send').props.onPress();
  await tick();
  assert.deepEqual(sent, [3500]);
  await session.close();
});

test('cleanup of an old review cannot unlock a newer review of the same canonical draft', async () => {
  const f = recording(true);
  find(f.render(), 'ComposerAction', 'media.recordVoiceMessage').props.onPress();
  await tick();
  await find(f.render(), 'VoiceDock').props.onStop();
  await tick();
  find(f.render(), 'VoiceDock').props.onExpand();
  const props = find(f.render(), 'VoiceReview').props;
  const old = component('voice-review.ios', {});
  const current = component('voice-review.ios', {});
  const oldWave = component('voice-waveform.ios', {});
  const currentWave = component('voice-waveform.ios', {});
  const oldProps = find(old.render(props), 'VoiceWaveform').props;
  const currentProps = find(current.render(props), 'VoiceWaveform').props;
  find(oldWave.render(oldProps), 'Slider').props.onValueChange(0.25);
  find(currentWave.render(currentProps), 'Slider').props.onValueChange(0.5);
  old.unmount();
  assert.equal(find(f.render(), 'MediaSheet').props.locked, true);
  await assert.rejects(props.onSend(props.session.getSendable()));
  assert.deepEqual(f.sent, []);
  find(currentWave.render(currentProps), 'Button', 'media.voiceDiscardTrim').props.onPress();
  assert.equal(find(f.render(), 'MediaSheet').props.locked, false);
  await props.onSend(props.session.getSendable());
  assert.deepEqual(f.sent, [4000]);
  await tick();
});

test('an old source or unmounted review cannot send or unlock the current source preview', async () => {
  const oldSession = new VoiceSession(voicePort());
  const session = new VoiceSession(voicePort());
  await Promise.all([oldSession.prepare('file:///old.m4a'), session.prepare('file:///new.m4a')]);
  const sent: string[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const oldTree = review.render({
    session: oldSession,
    initiallyEditing: true,
    onSend: async () => sent.push('old'),
  });
  const oldSend = find(oldTree, 'NativeButton', 'media.send');
  const oldWave = find(oldTree, 'VoiceWaveform').props;
  const props = { session, initiallyEditing: true, onSend: async () => sent.push('current') };
  // A session change closes the old editor; reopen the current source explicitly.
  find(review.render(props), 'NativeButton', 'media.voiceEdit').props.onPress();
  const tree = review.render(props);
  const wave = find(tree, 'VoiceWaveform').props;
  find(waveform.render(wave), 'Slider').props.onValueChange(0.25);
  oldWave.onTrimPreviewChange(oldWave.draft, false);
  oldSend.props.onPress();
  await tick();
  assert.deepEqual(sent, []);
  assert.equal(find(review.render(props), 'NativeButton', 'media.send').props.disabled, true);
  const currentSend = find(review.render(props), 'NativeButton', 'media.send');
  review.unmount();
  currentSend.props.onPress();
  await tick();
  assert.deepEqual(sent, []);
  await Promise.all([oldSession.close(), session.close()]);
});

test('Apply commits the exact preview before Send can capture it', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const sent: number[] = [];
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = {
    session,
    initiallyEditing: true,
    onSend: async (value: { preview: { durationMs: number } }) =>
      sent.push(value.preview.durationMs),
  };
  const waveProps = find(review.render(props), 'VoiceWaveform').props;
  find(waveform.render(waveProps), 'Slider').props.onValueChange(0.25);
  find(waveform.render(waveProps), 'Button', 'media.voiceApplyTrim').props.onPress();
  await tick();
  find(review.render(props), 'NativeButton', 'media.send').props.onPress();
  await tick();
  assert.deepEqual(sent, [3000]);
  await session.close();
});

test('a stale Apply handler commits the latest native slider value before React renders', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = { session, initiallyEditing: true, onSend: async () => {} };
  const waveProps = find(review.render(props), 'VoiceWaveform').props;
  find(waveform.render(waveProps), 'Slider').props.onValueChange(0.25);
  const previous = waveform.render(waveProps);
  const staleApply = find(previous, 'Button', 'media.voiceApplyTrim');
  find(previous, 'Slider').props.onValueChange(0.5);
  staleApply.props.onPress();
  await tick();
  assert.equal(session.getSendable()?.preview.durationMs, 2000);
  await session.close();
});

test('start and end slider events in one render preserve both latest endpoints', async () => {
  const session = new VoiceSession(voicePort());
  await session.prepare('file:///voice/capture.m4a');
  const review = component('voice-review.ios', {});
  const waveform = component('voice-waveform.ios', {});
  const props = { session, initiallyEditing: true, onSend: async () => {} };
  const waveProps = find(review.render(props), 'VoiceWaveform').props;
  const original = waveform.render(waveProps);
  const sliders = nodes(original).filter((node) => node.type === 'Slider');
  sliders[0]!.props.onValueChange(0.25);
  sliders[1]!.props.onValueChange(0.75);
  find(waveform.render(waveProps), 'Button', 'media.voiceApplyTrim').props.onPress();
  await tick();
  assert.deepEqual(session.getSnapshot().draft?.edit.trim, { startFrame: 1000, endFrame: 3000 });
  assert.equal(session.getSendable()?.preview.durationMs, 2000);
  await session.close();
});
