import * as attachmentCategories from '../media/attachment-categories.ts';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { AttachmentPhotoSession } from '../media/attachment-photo-session.ts';
import { photoSendOptions } from '../media/attachment-photo-options.ts';
import type { PhotoPickerSubmission } from '../media/attachment-photo-session';
import type { PhotoLibraryItem } from '../media/attachment-photo-library';
import type { AttachmentAsset } from '../media/attachment-draft';
import {
  AttachmentRecentSession,
  recentAttachmentId,
  type RecentAttachmentConfiguration,
} from '../media/attachment-recent-session.ts';

type Tree = { type: unknown; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
const compile = async (name: string) =>
  (
    await build({
      stdin: { contents: readFileSync(new URL(name, import.meta.url), 'utf8'), loader: 'tsx' },
      bundle: true,
      platform: 'node',
      format: 'cjs',
      jsx: 'automatic',
      write: false,
      plugins: [
        {
          name: 'actual-standalone-native-ports',
          setup(builder) {
            builder.onResolve({ filter: /^\.\/attachment-photo-keyboard-actions$/ }, () => ({
              path: fileURLToPath(
                new URL('../media/attachment-photo-keyboard-actions.ts', import.meta.url),
              ),
            }));
            builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
          },
        },
      ],
    })
  ).outputFiles[0]!.text;
const [hookCode, panelCode, bindingCode] = await Promise.all([
  compile('../media/attachment-photo-picker.ios.tsx'),
  compile('../media/attachment-photo-keyboard.ios.tsx'),
  compile('../media/keyboard-panel-native.ios.tsx'),
]);
const tick = async () => {
  for (let i = 0; i < 16; i++) await Promise.resolve();
};
const item = (id: string) =>
  ({
    id: `ph://${id}`,
    filename: `${id}.jpg`,
    mediaType: 'image',
    width: 100,
    height: 200,
    duration: null,
    creationTime: null,
    modificationTime: null,
    isFavorite: false,
  }) as PhotoLibraryItem;
const file = (id: string): AttachmentAsset => ({
  uri: `file:///owned/${id}`,
  name: `${id}.jpg`,
  mime: 'image/jpeg',
  size: 12,
});
function runtime(ports: Record<string, unknown>) {
  const slots: Slot[] = [];
  let cursor = 0,
    effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[]) =>
    !!a && a.length === b.length && a.every((v, i) => Object.is(v, b[i]));
  const effect = (read: () => void | (() => void), deps: readonly unknown[]) => {
    const s = slot();
    if (same(s.deps, deps)) return;
    s.deps = deps;
    effects.push(() => {
      s.cleanup?.();
      s.cleanup = read() ?? undefined;
    });
  };
  const jsx = (type: unknown, props: Record<string, any>): Tree => ({ type, props });
  const values: Record<string, unknown> = {
    './attachment-categories': attachmentCategories,
    ...ports,
    react: {
      useState: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value'))
          s.value = typeof initial === 'function' ? initial() : initial;
        return [
          s.value,
          (next: unknown) => {
            s.value = typeof next === 'function' ? next(s.value) : next;
          },
        ];
      },
      useRef: (initial: unknown) => {
        const s = slot();
        return (s.value ??= { current: initial });
      },
      useMemo: (read: () => unknown, deps: readonly unknown[]) => {
        const s = slot();
        if (!same(s.deps, deps)) {
          s.deps = deps;
          s.value = read();
        }
        return s.value;
      },
      useEffect: effect,
      useLayoutEffect: effect,
      useSyncExternalStore: (_subscribe: unknown, get: () => unknown) => get(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
  };
  return {
    values,
    load: (code: string) => {
      const module = { exports: {} as any };
      runInNewContext(code, {
        module,
        exports: module.exports,
        require: (name: string) => {
          assert.ok(name in values, name);
          return values[name];
        },
      });
      return module.exports;
    },
    render: <T>(read: () => T): T => {
      cursor = 0;
      effects = [];
      const result = read();
      effects.forEach((run) => run());
      return result;
    },
    unmount: () => {
      for (const s of slots) {
        s.cleanup?.();
        s.cleanup = undefined;
      }
    },
  };
}
type Hook = {
  pick(remaining: number, current: () => boolean): Promise<AttachmentAsset[]>;
  view: Tree | null;
  submission(): PhotoPickerSubmission | null;
  preferredUri(): string | null;
  cancel(): void;
};
function fixture(recentConfiguration?: RecentAttachmentConfiguration) {
  let serial = 0,
    current = true,
    width = 402,
    height = 874,
    observed = 0,
    stopped = 0,
    permissions = 0;
  let background: ((value: string) => void) | undefined;
  let capture: () => Promise<AttachmentAsset[]> = async () => [];
  let browse: () => Promise<AttachmentAsset[]> = async () => [];
  const calls: string[] = [],
    copies: string[][] = [],
    discarded: string[] = [];
  const nativePorts: Record<string, unknown> = {
    './attachment-categories': attachmentCategories,
    './attachment-recent-session': { AttachmentRecentSession, recentAttachmentId },
    './attachment-photo-options': { photoSendOptions },
    '../../modules/zoen-core/src/ZoenCoreModule': { releaseLocalFile: async () => {} },
    'expo-crypto': { randomUUID: () => `standalone-${++serial}` },
    expo: { requireNativeView: (_module: string, name: string) => name },
    'react-native': {
      useWindowDimensions: () => ({ width, height }),
      Keyboard: { metrics: () => undefined },
      Linking: { openSettings: () => {} },
      AppState: {
        addEventListener: (_name: string, listener: (value: string) => void) => {
          background = listener;
          return {
            remove: () => {
              background = undefined;
            },
          };
        },
      },
    },
    '../theme': {
      useTheme: () => ({
        presentation: {
          actionSheet: { background: '#17212b' },
          text: '#ffffff',
          secondary: '#aaaaaa',
          accent: '#0088ff',
          separator: '#334455',
        },
      }),
    },
    '../i18n': { t: (key: string) => key },
    'expo-glass-effect': { isLiquidGlassAvailable: () => true },
    'expo-haptics': {
      selectionAsync: async () => {
        calls.push('haptic');
      },
    },
  };
  const hookRuntime = runtime({
    ...nativePorts,
    './attachment-photo-session': { AttachmentPhotoSession },
    './attachment-photo-library': {
      photoLibrary: {
        permission: async () => {
          permissions++;
          return { granted: true, status: 'granted', canAskAgain: true, accessPrivileges: 'all' };
        },
        requestPermission: async () => {
          throw new Error('Granted permission');
        },
        albums: async () => [],
        load: async () => [item('a'), item('b')],
        manage: async () => {},
        materialize: async (items: readonly PhotoLibraryItem[]) => {
          copies.push(items.map((value) => value.id));
          return items.map((value) => file(value.id.slice(5)));
        },
        discard: (uri: string) => {
          discarded.push(uri);
        },
        subscribe: () => {
          observed++;
          return () => {
            stopped++;
          };
        },
      },
    },
    './attachment-picker': {
      pickAttachmentAssets: async (kind: string) => {
        calls.push(kind);
        return kind === 'camera' ? capture() : browse();
      },
    },
    './attachment-keyboard-context': { useAttachmentKeyboardSession: () => null },
    './attachment-photo-keyboard.ios': { AttachmentPhotoKeyboard: 'AttachmentPhotoKeyboard' },
  });
  const usePicker = hookRuntime.load(hookCode).useAttachmentPhotoPicker as (
    configuration?: RecentAttachmentConfiguration,
  ) => Hook;
  const panelRuntime = runtime(nativePorts);
  panelRuntime.values['./keyboard-panel-native.ios'] = panelRuntime.load(bindingCode);
  const component = panelRuntime.load(panelCode).AttachmentPhotoKeyboard as (
    props: Record<string, any>,
  ) => Tree;
  let hook = hookRuntime.render(() => usePicker(recentConfiguration));
  const renderHook = () => {
    hook = hookRuntime.render(() => usePicker(recentConfiguration));
    return hook;
  };
  const panel = () => {
    const latest = renderHook();
    assert.ok(latest.view);
    assert.equal(latest.view.type, 'AttachmentPhotoKeyboard');
    return panelRuntime.render(() => component(latest.view!.props));
  };
  const act = (tree: Tree, action: string, id?: string, payload: Record<string, unknown> = {}) => {
    const binding = JSON.parse(tree.props.binding),
      model = JSON.parse(tree.props.model);
    tree.props.onAction({
      nativeEvent: {
        token: binding.token,
        context: binding.context,
        revision: model.revision,
        action,
        id,
        ...payload,
      },
    });
  };
  const close = (tree: Tree, payload: Record<string, unknown> = {}) => {
    const binding = JSON.parse(tree.props.binding);
    return tree.props.onClosed({
      nativeEvent: { token: binding.token, context: binding.context, reason: 'closed', ...payload },
    });
  };
  return {
    panel,
    act,
    close,
    renderHook,
    calls,
    copies,
    discarded,
    pick: (remaining = 3) => hook.pick(remaining, () => current),
    state: () => {
      const view = renderHook().view;
      assert.ok(view);
      return (view.props.session as AttachmentPhotoSession).getSnapshot();
    },
    capture: (value: () => Promise<AttachmentAsset[]>) => {
      capture = value;
    },
    browse: (value: () => Promise<AttachmentAsset[]>) => {
      browse = value;
    },
    background: (value: string) => background?.(value),
    observers: () => ({ observed, stopped, permissions }),
    rotate: () => {
      width = 874;
      height = 402;
    },
    retire: () => {
      current = false;
      hook.cancel();
    },
    unmount: () => {
      hookRuntime.unmount();
      panelRuntime.unmount();
    },
  };
}

test('standalone Add Photos uses the same native panel without a composer tag or any mount-time Photos query', async () => {
  const f = fixture();
  assert.equal(f.renderHook().view, null);
  assert.deepEqual(f.observers(), { observed: 0, stopped: 0, permissions: 0 });
  const result = f.pick();
  await tick();
  const tree = f.panel(),
    binding = JSON.parse(tree.props.binding),
    model = JSON.parse(tree.props.model);
  assert.equal(tree.type, 'ZoenStandaloneAttachmentPanelHost');
  assert.equal(binding.standalone, true);
  assert.equal(Object.hasOwn(binding, 'tag'), false);
  assert.equal(binding.active, true);
  assert.equal(model.columns, 3);
  assert.equal(model.screen, 'grid');
  assert.equal(model.hasAudio, false);
  assert.equal(f.observers().observed, 1);
  f.act(tree, 'select', 'ph://b');
  f.rotate();
  const rotated = JSON.parse(f.panel().props.model);
  assert.equal(rotated.columns, 5);
  assert.deepEqual(rotated.selected, ['ph://b']);
  f.retire();
  assert.deepEqual(await result, []);
  f.renderHook();
  assert.equal(f.observers().stopped, 1);
  f.unmount();
});

test('standalone preview retains selection and explicit gallery Send waits actual native removal before releasing owned files', async () => {
  const f = fixture(),
    result = f.pick();
  let resolved = false;
  void result.then(() => {
    resolved = true;
  });
  await tick();
  f.act(f.panel(), 'select', 'ph://b');
  f.act(f.panel(), 'preview', 'ph://a');
  const tree = f.panel();
  assert.equal(JSON.parse(tree.props.model).screen, 'preview');
  assert.deepEqual(
    f.state().selected.map((value) => value.id),
    ['ph://b'],
  );
  assert.deepEqual(f.copies, []);
  f.act(tree, 'send', 'ph://a', { text: 'Final standalone caption!' });
  await tick();
  assert.equal(resolved, false);
  assert.deepEqual(f.copies, [['ph://b', 'ph://a']]);
  assert.deepEqual(Array.from(f.renderHook().submission()?.captions ?? []), [
    '',
    'Final standalone caption!',
  ]);
  await f.close(f.panel());
  await tick();
  assert.deepEqual(await result, [file('b'), file('a')]);
  assert.equal(f.renderHook().view, null);
  f.unmount();
});

test('standalone Camera cancel resumes one Photos request with a fresh native owner and ignores old duplicate close', async () => {
  const f = fixture(),
    result = f.pick();
  await tick();
  f.act(f.panel(), 'select', 'ph://b');
  f.act(f.panel(), 'caption', undefined, { text: 'Keep me' });
  const tree = f.panel(),
    oldToken = JSON.parse(tree.props.binding).token,
    photosToken = f.state().token;
  f.act(tree, 'camera');
  assert.equal(f.calls.includes('camera'), false);
  await f.close(tree);
  await tick();
  const resumed = f.panel();
  assert.notEqual(JSON.parse(resumed.props.binding).token, oldToken);
  assert.equal(f.state().token, photosToken);
  assert.deepEqual(
    f.state().selected.map((value) => value.id),
    ['ph://b'],
  );
  assert.equal(JSON.parse(resumed.props.model).caption.value, 'Keep me');
  await f.close(tree);
  f.act(tree, 'send', undefined, { text: 'Stale' });
  await tick();
  assert.equal(f.state().visible, true);
  assert.deepEqual(
    f.calls.filter((value) => value === 'camera'),
    ['camera'],
  );
  assert.deepEqual(f.copies, []);
  f.retire();
  assert.deepEqual(await result, []);
  f.unmount();
});

test('standalone Files tab is retained and Browse launches the existing document picker only after native removal', async () => {
  const f = fixture();
  f.browse(async () => [file('document')]);
  const result = f.pick();
  await tick();
  const photos = f.panel();
  f.act(photos, 'document');
  assert.deepEqual(f.calls, []);
  f.act(photos, 'category', 'document');
  assert.equal(f.state().visible, true);
  assert.deepEqual(f.calls, []);
  assert.deepEqual(f.copies, []);
  const files = f.panel();
  f.act(files, 'document');
  assert.equal(f.state().visible, false);
  assert.deepEqual(f.calls, []);
  await f.close(files);
  await tick();
  assert.deepEqual(f.calls, ['document']);
  assert.deepEqual(await result, [file('document')]);
  f.unmount();
});

test('standalone native retirement revokes a queued Camera or document Browse before either system picker is launched', async () => {
  for (const action of ['camera', 'document']) {
    const f = fixture(),
      result = f.pick();
    await tick();
    if (action === 'document') f.act(f.panel(), 'category', 'document');
    const tree = f.panel();
    f.act(tree, action);
    await f.close(tree, { reason: 'retired' });
    await tick();
    assert.deepEqual(await result, []);
    assert.deepEqual(f.calls, []);
    assert.deepEqual(f.copies, []);
    f.unmount();
  }
});

test('background cancels the standalone request and removes Photos observation without opening a replacement panel', async () => {
  const f = fixture(),
    result = f.pick();
  await tick();
  const tree = f.panel();
  assert.equal(f.observers().observed, 1);
  f.background('inactive');
  assert.equal(f.state().visible, true);
  f.background('background');
  await tick();
  assert.deepEqual(await result, []);
  assert.equal(f.renderHook().view, null);
  assert.equal(f.observers().stopped, 1);
  f.act(tree, 'select', 'ph://a');
  await f.close(tree);
  assert.deepEqual(f.copies, []);
  assert.deepEqual(f.calls, []);
  f.unmount();
});

test('route retirement cancels an in-flight recent preview immediately without waiting for unchanged photo token', async () => {
  let signal: AbortSignal | null = null;
  let finish!: () => void;
  const pending = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const row = {
    reference: {
      space_id: 'source',
      message_id: 'message',
      item_id: 'item',
      version: 1,
      sha256: 'a'.repeat(64),
    },
    name: 'file.pdf',
    mime: 'application/pdf',
    bytes: '12',
    at_ms: '10',
    author_title: 'Author',
    space_title: 'Source',
    ready: true,
  };
  const f = fixture({
    scope: {
      expected_identity_id: 'account',
      expected_device_id: 'device',
      space_id: 'space',
      thread_root_id: null,
    },
    port: {
      list: async () => ({ items: [row], next_cursor: null }),
      materialize: async () => {
        throw new Error('Preview cannot select');
      },
      release: async () => {},
      preview: async (_scope, _kind, _reference, value) => {
        signal = value;
        await pending;
      },
    },
  });
  const result = f.pick();
  await tick();
  f.act(f.panel(), 'category', 'document');
  f.panel();
  await tick();
  const files = f.panel();
  f.act(files, 'recentPreview', recentAttachmentId(row));
  f.close(files);
  await tick();
  assert.ok(signal);
  assert.equal((signal as AbortSignal).aborted, false);
  // Native removal already set visible=false. Blur must cancel directly, even
  // though the busy PhotoSession retains its same token/category until drain.
  f.retire();
  assert.equal((signal as AbortSignal).aborted, true);
  finish();
  await tick();
  assert.deepEqual(await result, []);
  assert.equal(f.renderHook().view, null);
  f.unmount();
});
