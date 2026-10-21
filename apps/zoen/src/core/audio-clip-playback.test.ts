import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { AudioClipController, type AudioClipPlayback } from '../media/audio-clip-controller.ts';
import { PlaybackCoordinator } from '../media/playback-owner.ts';
import { VoiceSession, type VoiceNativePort } from '../media/voice-session.ts';
import * as voiceEdit from '../media/voice-edit.ts';
import { telegramAudio } from './telegram-audio-presentation.ts';
import { telegramPresentation } from '../theme/telegram.ts';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
const paths = ['audio-clip-playback', 'AudioClip', 'voice-preview-playback'] as const;
const compiled = Object.fromEntries(
  paths.map((name) => {
    const path = new URL(
      `../media/${name}.${name === 'audio-clip-playback' ? 'ts' : 'tsx'}`,
      import.meta.url,
    );
    const source = readFileSync(
      name === 'audio-clip-playback'
        ? (process.env.ZOEN_AUDIO_CLIP_HOOK_SOURCE ?? path)
        : name === 'AudioClip'
          ? (process.env.ZOEN_AUDIO_CLIP_CONTROLS_SOURCE ?? path)
          : path,
      'utf8',
    );
    return [
      name,
      transformSync(source, {
        filename: fileURLToPath(path),
        configFile: false,
        babelrc: false,
        presets: [require.resolve('babel-preset-expo')],
        caller: {
          name: 'metro',
          platform: 'ios',
          engine: 'hermes',
          isDev: false,
          supportsReactCompiler: true,
        },
      }).code,
    ];
  }),
);
// The negative source uses the installed SDK hooks, including their real
// initial-state and passive-listener behavior, rather than a permissive stub.
const sdkEvents = transformSync(
  readFileSync(new URL('../../node_modules/expo/src/hooks/useEvent.ts', import.meta.url), 'utf8'),
  {
    filename: 'sdk-useEvent.ts',
    configFile: false,
    babelrc: false,
    presets: [require.resolve('babel-preset-expo')],
  },
).code;
const sdkStatusSource = readFileSync(
  new URL('../../node_modules/expo-audio/src/ExpoAudio.ts', import.meta.url),
  'utf8',
).match(/export function useAudioPlayerStatus\([\s\S]*?\n\}/)![0];
const sdkStatus = transformSync(sdkStatusSource, {
  filename: 'sdk-player-status.ts',
  configFile: false,
  babelrc: false,
  presets: [require.resolve('babel-preset-expo')],
}).code;
type Element = { type: unknown; key?: string; props: Record<string, any> };
type Slot = {
  value?: any;
  deps?: readonly unknown[];
  cleanup?: () => void;
  effect?: () => void | (() => void);
  kind?: 'layout' | 'passive';
};
type Instance = { type: unknown; slots: Slot[]; cursor: number; pending: Slot[] };
type Status = {
  id: string;
  isLoaded: boolean;
  playing: boolean;
  isBuffering: boolean;
  currentTime: number;
  duration: number;
  playbackRate: number;
  error: string | null;
  playbackState: string;
};
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}

/** Public Expo player ports; actual production controller, hook, provider and controls run below. */
function fixture() {
  const coordinator = new PlaybackCoordinator();
  let active!: Instance;
  let dirty = false;
  let root: Element | null = null;
  let tree: Element[] = [];
  let visited: Instance[] = [];
  let passive: (() => void)[] = [];
  const instances = new Map<string, Instance>();
  const initial = new Map<string | null, Partial<Status>>();
  let mode = async () => {};
  let modeCalls = 0;
  let compilerCalls = 0;
  let nextPlayerId = 0;
  const native: ReturnType<typeof makePlayer>[] = [];
  const jsx = (type: unknown, props: Element['props'], key?: string) => ({ type, props, key });
  const slot = () => (active.slots[active.cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
  const memo = (read: () => unknown, deps: readonly unknown[]) => {
    const value = slot();
    if (!same(value.deps, deps)) {
      value.deps = deps;
      value.value = read();
    }
    return value.value;
  };
  const effect = (
    kind: 'layout' | 'passive',
    read: () => void | (() => void),
    deps?: readonly unknown[],
  ) => {
    const value = slot();
    if (same(value.deps, deps)) return;
    value.deps = deps;
    value.effect = read;
    value.kind = kind;
    active.pending.push(value);
  };
  const react = {
    useMemo: memo,
    useCallback: (read: unknown, deps: readonly unknown[]) => memo(() => read, deps),
    useRef: (value: unknown) => memo(() => ({ current: value }), []),
    useState(initial: unknown) {
      const value = slot();
      if (!Object.hasOwn(value, 'value'))
        value.value = typeof initial === 'function' ? initial() : initial;
      const set = (value.cleanup ??= ((next: unknown) => {
        const result = typeof next === 'function' ? next(value.value) : next;
        if (!Object.is(result, value.value)) {
          value.value = result;
          dirty = true;
        }
      }) as () => void);
      return [value.value, set];
    },
    useLayoutEffect: (read: () => void | (() => void), deps?: readonly unknown[]) =>
      effect('layout', read, deps),
    useEffect: (read: () => void | (() => void), deps?: readonly unknown[]) =>
      effect('passive', read, deps),
    useSyncExternalStore(subscribe: (read: () => void) => () => void, snapshot: () => unknown) {
      effect(
        'layout',
        () =>
          subscribe(() => {
            dirty = true;
          }),
        [subscribe],
      );
      return snapshot();
    },
    createContext(value: unknown) {
      const context = { value, Provider: null as unknown };
      context.Provider = { context };
      return context;
    },
    useContext: (context: { value: unknown }) => context.value,
  };
  function makePlayer(uri: string | null) {
    const id = `player-${nextPlayerId++}`;
    const state: Status = {
      id,
      isLoaded: false,
      playing: false,
      isBuffering: false,
      currentTime: 0,
      duration: 0,
      playbackRate: 1,
      error: null,
      playbackState: 'idle',
      ...initial.get(uri),
    };
    const listeners = new Set<(status: Status) => void>();
    const allListeners: ((status: Status) => void)[] = [];
    let seek = async (_seconds: number) => {};
    const result = {
      id,
      uri,
      released: false,
      plays: 0,
      pauses: 0,
      replacements: 0,
      seeks: [] as number[],
      tolerances: [] as number[][],
      state,
      allListeners,
      listeners,
      get playing() {
        alive();
        return state.playing;
      },
      get currentTime() {
        alive();
        return state.currentTime;
      },
      get currentStatus() {
        alive();
        return { ...state };
      },
      play() {
        alive();
        result.plays++;
        state.playing = true;
      },
      pause() {
        alive();
        result.pauses++;
        state.playing = false;
      },
      async seekTo(seconds: number, before: number, after: number) {
        alive();
        result.seeks.push(seconds);
        result.tolerances.push([before, after]);
        await seek(seconds);
        if (!result.released) state.currentTime = seconds;
      },
      setPlaybackRate(rate: number) {
        alive();
        state.playbackRate = rate;
      },
      replace(source: { uri: string }) {
        alive();
        assert.equal(source.uri, uri);
        result.replacements++;
      },
      addListener(name: string, callback: (status: Status) => void) {
        alive();
        assert.equal(name, 'playbackStatusUpdate');
        listeners.add(callback);
        allListeners.push(callback);
        return {
          remove() {
            listeners.delete(callback);
          },
        };
      },
      emit(patch: Partial<Status> = {}) {
        Object.assign(state, patch);
        for (const read of listeners) read({ ...state });
      },
      setSeek(next: typeof seek) {
        seek = next;
      },
    };
    function alive() {
      assert.equal(result.released, false, 'native object accessed after hook release');
    }
    return result;
  }
  const modules: Record<string, unknown> = {
    react,
    'react/compiler-runtime': {
      c(size: number) {
        compilerCalls++;
        return memo(() => Array(size).fill(Symbol.for('react.memo_cache_sentinel')), []);
      },
    },
    'react/jsx-runtime': { jsx, jsxs: jsx, Fragment: 'Fragment' },
    'expo-audio': {
      useAudioPlayer(source: { uri: string } | null, options: unknown) {
        assert.equal(
          JSON.stringify(options),
          JSON.stringify({ updateInterval: 100, allowsExternalPlayback: false }),
        );
        const uri = source?.uri ?? null;
        const player = memo(() => {
          const player = makePlayer(uri);
          native.push(player);
          return player;
        }, [uri]);
        effect(
          'passive',
          () => () => {
            player.released = true;
          },
          [player],
        );
        return player;
      },
      setAudioModeAsync: async () => {
        modeCalls++;
        await mode();
      },
    },
    './audio-clip-controller.ts': { AudioClipController },
    './playback-owner.ts': { mediaPlayback: coordinator },
    './voice-edit': voiceEdit,
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en' },
    '../theme': { useTheme: () => ({ colors: {}, presentation: telegramPresentation('day') }) },
    '../core/telegram-audio-presentation': { telegramAudio },
    'react-native': { Text: 'Text', View: 'View', ActivityIndicator: 'Spinner' },
    '../components/message-interaction': { MessagePressable: 'Pressable' },
    '../components/primitives': { Icon: 'Icon', NativeButton: 'NativeButton' },
    './audio-progress': { AudioProgress: 'AudioProgress' },
  };
  const sdkEventModule = { exports: {} as Record<string, any> };
  runInNewContext(sdkEvents, {
    module: sdkEventModule,
    exports: sdkEventModule.exports,
    require: (name: string) => {
      if (name.startsWith('@babel/runtime/')) return require(name);
      assert.equal(name, 'react');
      return react;
    },
  });
  modules.expo = sdkEventModule.exports;
  const sdkStatusModule = { exports: {} as Record<string, any> };
  runInNewContext(sdkStatus, {
    module: sdkStatusModule,
    exports: sdkStatusModule.exports,
    useMemo: react.useMemo,
    useEvent: sdkEventModule.exports.useEvent,
    PLAYBACK_STATUS_UPDATE: 'playbackStatusUpdate',
  });
  (modules['expo-audio'] as Record<string, unknown>).useAudioPlayerStatus =
    sdkStatusModule.exports.useAudioPlayerStatus;
  const evaluate = (name: (typeof paths)[number]) => {
    const module = { exports: {} as Record<string, any> };
    runInNewContext(compiled[name], {
      module,
      exports: module.exports,
      require(path: string) {
        assert.ok(Object.hasOwn(modules, path), `unexpected native import ${path}`);
        return modules[path];
      },
    });
    return module.exports;
  };
  const hook = evaluate('audio-clip-playback');
  modules['./audio-clip-playback'] = modules['./audio-clip-playback.ts'] = hook;
  const controls = evaluate('AudioClip');
  const provider = evaluate('voice-preview-playback');
  function expand(value: unknown, path: string): void {
    if (Array.isArray(value)) {
      value.forEach((child, i) => expand(child, `${path}.${i}`));
      return;
    }
    if (!value || typeof value !== 'object' || !('props' in value)) return;
    const node = value as Element;
    const address = `${path}:${node.key ?? ''}`;
    if (typeof node.type === 'function') {
      let instance = instances.get(address);
      if (instance && instance.type !== node.type) {
        cleanup(instance);
        instances.delete(address);
        instance = undefined;
      }
      if (!instance) {
        instance = { type: node.type, slots: [], cursor: 0, pending: [] };
        instances.set(address, instance);
      }
      active = instance;
      instance.cursor = 0;
      instance.pending = [];
      visited.push(instance);
      const output = node.type(node.props);
      expand(output, `${address}.child`);
      return;
    }
    if (node.type && typeof node.type === 'object' && 'context' in node.type) {
      const context = (node.type as { context: { value: unknown } }).context;
      const previous = context.value;
      context.value = node.props.value;
      expand(node.props.children, `${address}.context`);
      context.value = previous;
      return;
    }
    tree.push(node);
    expand(node.props.children, `${address}.children`);
  }
  function cleanup(instance: Instance) {
    for (const value of instance.slots)
      if (value.kind === 'layout') {
        value.cleanup?.();
        value.cleanup = undefined;
      }
    for (const value of instance.slots)
      if (value.kind === 'passive')
        passive.push(() => {
          value.cleanup?.();
          value.cleanup = undefined;
        });
  }
  function render(element = root!, flushPassive = true) {
    root = element;
    let attempts = 0;
    do {
      assert.ok(++attempts < 20, 'effects must converge without re-creating a playback facade');
      dirty = false;
      tree = [];
      visited = [];
      expand(root, 'root');
      const current = new Set(visited);
      for (const [path, owner] of instances)
        if (!current.has(owner)) {
          cleanup(owner);
          instances.delete(path);
        }
      for (const owner of [...visited].reverse())
        for (const value of owner.pending) {
          const commit = () => {
            value.cleanup?.();
            value.cleanup = value.effect?.() ?? undefined;
          };
          if (value.kind === 'layout') commit();
          else passive.push(commit);
        }
    } while (dirty);
    if (flushPassive) flush();
    assert.ok(compilerCalls > 0, 'exercise production React Compiler output');
    return tree;
  }
  function flush() {
    const pending = passive;
    passive = [];
    pending.forEach((commit) => commit());
  }
  function unmount() {
    for (const owner of [...instances.values()].reverse()) cleanup(owner);
    instances.clear();
    flush();
  }
  return {
    jsx,
    react,
    hook,
    controls,
    provider,
    native,
    initial,
    coordinator,
    render,
    flush,
    unmount,
    nodes: () => tree,
    mode: (value: typeof mode) => {
      mode = value;
    },
    modeCalls: () => modeCalls,
  };
}

test('compiled hook binds initial/status state to the exact player and drops late A before passive release', async () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner({ uri }: { uri: string }) {
    playback = f.hook.useAudioClipPlayback({ uri });
    return null;
  }
  f.initial.set('A', { isLoaded: true, duration: 12, currentTime: 5, playbackRate: 1.5 });
  f.render(f.jsx(Owner, { uri: 'A' }));
  const previous = playback;
  assert.equal(playback.getSnapshot().loaded, true);
  assert.equal(playback.getSnapshot().currentSeconds, 5);
  const a = f.native[0]!;
  const oldCallback = a.allListeners[0]!;
  f.render(f.jsx(Owner, { uri: 'B' }), false);
  assert.notEqual(playback, previous);
  assert.equal(a.released, false, 'exercise the gap before passive SDK cleanup');
  assert.equal(playback.getSnapshot().loaded, false, 'B must not inherit A loaded status');
  assert.equal(a.listeners.size, 0);
  oldCallback({ ...a.state, currentTime: 9, duration: 25, isLoaded: true });
  assert.equal(playback.getSnapshot().loaded, false);
  assert.equal(playback.getSnapshot().durationSeconds, 0);
  assert.equal(playback.getSnapshot().currentSeconds, 0);
  await playback.playFrom(2);
  assert.equal(f.native[1]!.plays, 0);
  f.native[1]!.emit({ isLoaded: true, duration: 4, currentTime: 1 });
  assert.equal(playback.getSnapshot().durationSeconds, 4);
  f.render();
  const next = playback;
  f.render();
  assert.equal(playback, next, 'status renders retain the source facade');
  f.flush();
  assert.equal(a.released, true);
  f.unmount();
});

test('a failure that predates listener installation exposes Retry and retries only that same source', () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner() {
    playback = f.hook.useAudioClipPlayback({ uri: 'broken' });
    return f.jsx(f.controls.AudioClipControls, { playback });
  }
  f.initial.set('broken', { error: 'invalid audio', playbackState: 'failed' });
  f.render(f.jsx(Owner, {}));
  assert.equal(playback.getSnapshot().loadFailed, true);
  const retry = f
    .nodes()
    .find((node) => node.type === 'NativeButton' && node.props.label === 'media.retryAudio');
  assert.ok(retry);
  retry.props.onPress();
  assert.equal(f.native[0]!.replacements, 1);
  assert.equal(f.native[0]!.plays, 0);
  f.unmount();
});

test('compiled controls allow Pause to revoke pending mode/seek rather than disabling cancellation', async () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner() {
    playback = f.hook.useAudioClipPlayback({ uri: 'A' });
    return f.jsx(f.controls.AudioClipControls, { playback });
  }
  f.initial.set('A', { isLoaded: true, duration: 10 });
  f.render(f.jsx(Owner, {}));
  const mode = deferred();
  f.mode(() => mode.promise);
  const pending = playback.playFrom(2);
  await tick();
  f.render();
  const pause = f
    .nodes()
    .find(
      (node) => node.type === 'Pressable' && node.props.accessibilityLabel === 'media.pauseAudio',
    );
  assert.ok(pause);
  assert.equal(pause.props.disabled, false);
  pause.props.onPress();
  mode.resolve();
  await pending;
  await tick();
  assert.equal(f.native[0]!.plays, 0);
  assert.equal(playback.getSnapshot().busy, false);
  f.unmount();
});

function voicePort(): VoiceNativePort {
  let render = 0;
  return {
    prepare: async () => ({
      id: 'voice',
      uri: 'original',
      sampleRate: 1000,
      frameCount: 10_000,
      durationMs: 10_000,
      levels: [0.2, 0.8],
    }),
    render: async (_id, ranges) => {
      const frames = ranges.reduce((total, range) => total + range.endFrame - range.startFrame, 0);
      return {
        uri: `render-${++render}`,
        mime: 'audio/mp4',
        frameCount: frames,
        durationMs: frames,
        levels: [0.2, 0.8],
      };
    },
    transcribe: async () => ({ status: 'unavailable', text: '', words: [] }),
    releaseRender: async () => {},
    cancel: async () => {},
    release: async () => {},
  };
}

test('real compiled provider preserves one playing native owner through compact/detail and status updates', async () => {
  const f = fixture();
  const session = new VoiceSession(voicePort());
  await session.prepare('original');
  const uri = session.getSendable()!.preview.uri;
  f.initial.set(uri, { isLoaded: true, duration: 10 });
  let binding: any;
  function Presentation({ compact }: { compact: boolean }) {
    binding = f.provider.useVoicePreviewPlayback();
    return binding
      ? f.jsx(f.controls.AudioClipControls, { playback: binding.playback, compact })
      : null;
  }
  const current = () => true;
  const tree = (compact: boolean) =>
    f.jsx(f.provider.VoicePreviewPlayback, {
      session,
      ownerKey: 'owner',
      disabled: false,
      isCurrent: current,
      children: f.jsx(Presentation, { compact }),
    });
  f.render(tree(true));
  const playback = binding.playback as AudioClipPlayback;
  await playback.playFrom(3.125);
  playback.setRate(1.5);
  f.native[0]!.emit({ currentTime: 3.4, playbackRate: 1.5, playing: true });
  const pauses = f.native[0]!.pauses;
  f.render(tree(false));
  assert.equal(f.native.length, 1, 'presentation changes allocate no additional player');
  assert.equal(binding.playback, playback);
  assert.equal(playback.getSnapshot().currentSeconds, 3.4);
  assert.equal(playback.getSnapshot().rate, 1.5);
  assert.equal(playback.getSnapshot().playing, true);
  assert.equal(
    f.native[0]!.pauses,
    pauses,
    'neither context re-publication nor status rendering pauses',
  );
  f.native[0]!.emit({ currentTime: 3.6 });
  f.render(tree(true));
  assert.equal(binding.playback, playback);
  assert.equal(f.native[0]!.pauses, pauses);
  f.unmount();
  await session.close();
});

test('real session pending trim synchronously revokes a delayed audition before source lease cleanup and ABA', async () => {
  const f = fixture();
  const session = new VoiceSession(voicePort());
  await session.prepare('original');
  f.initial.set(session.getSendable()!.preview.uri, { isLoaded: true, duration: 10 });
  let binding: any;
  function Presentation() {
    binding = f.provider.useVoicePreviewPlayback();
    return null;
  }
  f.render(
    f.jsx(f.provider.VoicePreviewPlayback, {
      session,
      ownerKey: 'owner',
      disabled: false,
      isCurrent: () => true,
      children: f.jsx(Presentation, {}),
    }),
  );
  const playback = binding.playback as AudioClipPlayback;
  const gate = deferred();
  f.native[0]!.setSeek(() => gate.promise);
  binding.playFromSelection(session.getSnapshot().draft!, { startFrame: 2000, endFrame: 4000 });
  await tick();
  assert.deepEqual(f.native[0]!.seeks, [2]);
  const draft = session.getSnapshot().draft!;
  session.setPendingTrim(draft, { startFrame: 1000, endFrame: 9000 });
  assert.equal(playback.getSnapshot().busy, false, 'revoked synchronously without a React render');
  session.setPendingTrim(draft, null);
  gate.resolve();
  await tick();
  assert.equal(f.native[0]!.plays, 0, 'discarding the trim cannot revive the retired command');
  assert.deepEqual(f.native[0]!.tolerances, [[0, 0]]);
  f.unmount();
  await session.close();
});

test('provider unmount disconnects before SDK passive release and late mode resolution never plays', async () => {
  const f = fixture();
  let playback!: AudioClipPlayback;
  function Owner() {
    playback = f.hook.useAudioClipPlayback({ uri: 'A' });
    return null;
  }
  f.initial.set('A', { isLoaded: true, duration: 10 });
  f.render(f.jsx(Owner, {}));
  const gate = deferred();
  f.mode(() => gate.promise);
  const pending = playback.playFrom();
  await tick();
  f.unmount();
  assert.equal(f.native[0]!.released, true);
  gate.resolve();
  await pending;
  await tick();
  assert.equal(f.native[0]!.plays, 0);
});
