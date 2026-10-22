import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { AudioClipController } from '../media/audio-clip-controller.ts';
import { PlaybackCoordinator } from '../media/playback-owner.ts';
import * as voiceEdit from '../media/voice-edit.ts';
import { type VoiceNativePort } from '../media/voice-session.ts';
import { telegramPresentation } from '../theme/telegram.ts';
import { telegramAudio } from './telegram-audio-presentation.ts';

export const require = createRequire(import.meta.url);
export const { transformSync } = require('@babel/core') as {
  transformSync(source: string, options: Record<string, unknown>): { code: string };
};
export const paths = ['audio-clip-playback', 'AudioClip', 'voice-preview-playback'] as const;
export const compiled = Object.fromEntries(
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
export const sdkEvents = transformSync(
  readFileSync(new URL('../../node_modules/expo/src/hooks/useEvent.ts', import.meta.url), 'utf8'),
  {
    filename: 'sdk-useEvent.ts',
    configFile: false,
    babelrc: false,
    presets: [require.resolve('babel-preset-expo')],
  },
).code;
export const sdkStatusSource = readFileSync(
  new URL('../../node_modules/expo-audio/src/ExpoAudio.ts', import.meta.url),
  'utf8',
).match(/export function useAudioPlayerStatus\([\s\S]*?\n\}/)![0];
export const sdkStatus = transformSync(sdkStatusSource, {
  filename: 'sdk-player-status.ts',
  configFile: false,
  babelrc: false,
  presets: [require.resolve('babel-preset-expo')],
}).code;
export type Element = { type: unknown; key?: string; props: Record<string, any> };
export type Slot = {
  value?: any;
  deps?: readonly unknown[];
  cleanup?: () => void;
  effect?: () => void | (() => void);
  kind?: 'layout' | 'passive';
};
export type Instance = { type: unknown; slots: Slot[]; cursor: number; pending: Slot[] };
export type Status = {
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
export const tick = () => new Promise<void>((resolve) => setImmediate(resolve));
export function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}

/** Public Expo player ports; actual production controller, hook, provider and controls run below. */
export function fixture() {
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

export function voicePort(): VoiceNativePort {
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
