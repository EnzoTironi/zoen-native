import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { PlaybackCoordinator } from '../media/playback-owner.ts';
const compiled = (
  await build({
    stdin: {
      contents: `${readFileSync(process.env.ZOEN_VIDEO_ALBUM_SOURCE ?? new URL('../media/VideoClip.tsx', import.meta.url), 'utf8')}\nexport { VideoClipContent };`,
      loader: 'tsx',
    },
    bundle: true,
    platform: 'node',
    format: 'cjs',
    jsx: 'automatic',
    write: false,
    plugins: [
      {
        name: 'video-native-ports',
        setup(builder) {
          builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
        },
      },
    ],
  })
).outputFiles[0]!.text;
type Element = { type: unknown; props: Record<string, any> };
function fixture() {
  let cursor = 0,
    unavailable = 0;
  const slots: { value?: any; deps?: unknown[]; cleanup?: () => void }[] = [];
  let effects: (() => void)[] = [];
  let state: { status: string; error?: { message: string } } = { status: 'readyToPlay' };
  const coordinator = new PlaybackCoordinator();
  const player = {
    status: 'readyToPlay',
    availableVideoTracks: [],
    duration: 10,
    playing: false,
    allowsExternalPlayback: true,
    pause() {},
    play() {},
  };
  const callback = () => {
    unavailable++;
  };
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useRef: (value: unknown) => {
        const s = (slots[cursor++] ??= {});
        return (s.value ??= { current: value });
      },
      useState: (value: unknown) => {
        const s = (slots[cursor++] ??= {});
        s.value ??= value;
        return [
          s.value,
          (next: any) => {
            s.value = typeof next === 'function' ? next(s.value) : next;
          },
        ];
      },
      useEffect: (read: () => void | (() => void), deps: unknown[]) => {
        const s = (slots[cursor++] ??= {});
        if (
          s.deps?.length === deps.length &&
          deps.every((value, index) => Object.is(value, s.deps![index]))
        )
          return;
        s.deps = deps;
        effects.push(() => {
          s.cleanup?.();
          s.cleanup = read() ?? undefined;
        });
      },
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { View: 'View', Text: 'Text', ActivityIndicator: 'ActivityIndicator' },
    expo: {
      useEvent: (_player: unknown, event: string, fallback: unknown) =>
        event === 'statusChange' ? state : fallback,
      useEventListener() {},
    },
    'expo-video': {
      VideoView: 'VideoView',
      useVideoPlayer: (_source: unknown, setup: (value: unknown) => void) => {
        setup(player);
        return player;
      },
    },
    'react-native-gesture-handler': {
      GestureDetector: 'GestureDetector',
      useNativeGesture: () => ({}),
    },
    '../components/message-interaction': {
      useMessagePan: () => undefined,
      useMessagePressConsumption: () => () => {},
    },
    '../components/primitives': { NativeButton: 'NativeButton' },
    '../theme': { useTheme: () => ({ colors: { danger: '#f00' } }) },
    '../i18n/index.ts': { t: (key: string) => key },
    './playback-owner.ts': { mediaPlayback: coordinator },
  };
  const module = {
    exports: {} as { VideoClip(props: any): Element; VideoClipContent(props: any): Element },
  };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  return {
    callback,
    outer: () => {
      cursor = 0;
      return module.exports.VideoClip({
        uri: 'file:///owned/clip.mp4',
        tile: { width: 100, height: 80 },
        onInlineUnavailable: callback,
      });
    },
    render: () => {
      cursor = 0;
      effects = [];
      const tree = module.exports.VideoClipContent({
        uri: 'file:///owned/clip.mp4',
        compact: true,
        tile: { width: 100, height: 80 },
        onInlineUnavailable: callback,
        onRetry() {},
      });
      effects.forEach((work) => work());
      return tree;
    },
    fail: (withStatus: boolean) => {
      state = withStatus
        ? { status: 'error' }
        : { status: 'readyToPlay', error: { message: 'Synthetic player failure' } };
    },
    unavailable: () => unavailable,
    retire: () => {
      slots.forEach((s) => s.cleanup?.());
    },
  };
}
test('actual VideoClip forwards the optional album escape callback to its keyed owned player', () => {
  const f = fixture();
  assert.strictEqual(f.outer().props.onInlineUnavailable, f.callback);
  f.retire();
});
for (const withStatus of [false, true])
  test(`actual player ${withStatus ? 'error status' : 'error payload'} releases its clipped album cell so the ordinary Retry remains available`, () => {
    const f = fixture();
    f.render();
    assert.equal(f.unavailable(), 0);
    f.fail(withStatus);
    f.render();
    assert.equal(f.unavailable(), 1);
    f.render();
    assert.equal(
      f.unavailable(),
      1,
      'Unchanged player failure does not repeatedly publish a fallback',
    );
    f.retire();
  });
