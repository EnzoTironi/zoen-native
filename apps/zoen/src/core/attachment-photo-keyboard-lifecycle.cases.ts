import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import * as attachmentCategories from '../media/attachment-categories.ts';
import type { AttachmentAsset } from '../media/attachment-draft';
import { AttachmentKeyboardSession } from '../media/attachment-keyboard-session.ts';
import type { PhotoLibraryItem } from '../media/attachment-photo-library';
import { AttachmentPhotoSession } from '../media/attachment-photo-session.ts';
import { AttachmentRecentSession, recentAttachmentId } from '../media/attachment-recent-session.ts';
import { deferred, fixture, item, pickerCode, tick } from './attachment-photo-keyboard.fixtures.ts';

test('queued same-token old model actions cannot override newer album/selection projection', async () => {
  const f = await fixture(),
    old = f.render();
  f.photos.select(item('b'));
  f.render();
  f.act(old, 'select', 'ph://a');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((i) => i.id),
    ['ph://b'],
  );
  f.act(old, 'preview', 'ph://a');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((i) => i.id),
    ['ph://b'],
  );
  assert.deepEqual(f.copies, []);
  f.retire();
  await tick();
});
test('retained native camera close consults CURRENT session state, waits removal then captures once', async () => {
  const f = await fixture(),
    tree = f.render();
  f.act(tree, 'camera');
  assert.equal(f.photos.getSnapshot().visible, false);
  assert.deepEqual(f.captures, []);
  // Intentionally no React render: callback still closes over visible=true.
  f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [3]);
  assert.equal((f.picked() as any[])[0].uri, 'file:///owned/camera');
  assert.deepEqual(f.calls, ['photo', 'photo-drained']);
});
test('native preview navigates without selecting or preparing the viewed item', async () => {
  const f = await fixture();
  f.photos.select(item('b'));
  const tree = f.render();
  f.act(tree, 'preview', 'ph://a');
  await tick();
  assert.deepEqual(f.copies, []);
  assert.equal(f.preferred(), null);
  assert.equal(f.photos.getSnapshot().screen, 'preview');
  assert.equal(f.photos.getSnapshot().previewId, 'ph://a');
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  assert.equal(f.picked(), undefined);
  f.act(f.render(), 'back');
  assert.equal(f.photos.getSnapshot().screen, 'grid');
  assert.equal(f.photos.getSnapshot().previewId, null);
  f.retire();
  await tick();
});
test('retained category selection never closes or copies, and Browse waits native close before the existing Files picker', async () => {
  const f = await fixture(),
    tree = f.render();
  f.act(tree, 'document');
  assert.equal(
    f.keyboard.getSnapshot().presentation?.closing,
    false,
    'Browse cannot be invoked from the Photos child',
  );
  f.act(tree, 'category', 'document');
  assert.equal(f.photos.getSnapshot().category, 'document');
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.keyboard.getSnapshot().presentation?.closing, false);
  assert.deepEqual(f.calls, ['photo']);
  assert.deepEqual(f.copies, []);
  assert.equal(f.picked(), undefined);
  const files = f.render();
  assert.equal(JSON.parse(files.props.model).activeCategory, 'document');
  f.act(files, 'document');
  assert.deepEqual(f.calls, ['photo']);
  assert.equal(f.keyboard.getSnapshot().presentation?.closing, false);
  assert.equal(f.photos.getSnapshot().visible, false);
  await f.close(files);
  await tick();
  assert.deepEqual(f.calls, ['photo', 'browse', 'photo-drained']);
  assert.equal(f.keyboard.getSnapshot().presentation, null);
  assert.equal((f.picked() as AttachmentAsset[])[0]?.mime, 'application/pdf');
  const g = await fixture();
  g.photos.select(item('a'));
  const selected = g.render();
  g.act(selected, 'category', 'audio');
  g.act(selected, 'audio');
  assert.equal(g.photos.getSnapshot().category, 'photos');
  assert.equal(g.keyboard.getSnapshot().presentation?.closing, false);
  g.retire();
  await tick();
});
test('cancelled mounted Browse resumes the same Files category and Photos owner with a fresh native token', async () => {
  const f = await fixture();
  f.browseNext(async () => []);
  f.act(f.render(), 'category', 'document');
  const files = f.render(),
    original = f.request.id;
  f.act(files, 'document');
  assert.equal(f.photos.getSnapshot().visible, false);
  assert.deepEqual(f.calls, ['photo']);
  await f.close(files);
  await tick();
  assert.deepEqual(f.calls, ['photo', 'browse']);
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.equal(f.photos.getSnapshot().category, 'document');
  assert.equal(f.keyboard.ownsPicker(original), true);
  const resumed = f.render();
  assert.notEqual(f.request.id, original);
  assert.equal(JSON.parse(resumed.props.model).activeCategory, 'document');
  assert.deepEqual(f.copies, []);
  assert.equal(f.picked(), undefined);
  await f.close(files);
  assert.deepEqual(f.calls, ['photo', 'browse']);
  f.retire();
  await tick();
});
test('foreign action identity and retired ownership cannot select, prepare or handoff', async () => {
  const f = await fixture(),
    tree = f.render();
  tree.props.onAction({
    nativeEvent: {
      token: 'other',
      context: f.request.field.context,
      revision: JSON.parse(tree.props.model).revision,
      action: 'select',
      id: 'ph://a',
    },
  });
  f.retire();
  f.act(tree, 'preview', 'ph://a');
  f.close(tree);
  await tick();
  assert.deepEqual(f.copies, []);
  assert.deepEqual(f.captures, []);
  assert.deepEqual(f.calls, ['photo', 'photo-drained']);
});

test('native account retirement cannot approve pending Camera or category handoff', async () => {
  for (const action of ['camera', 'document']) {
    const f = await fixture();
    if (action === 'document') f.act(f.render(), 'category', 'document');
    const tree = f.render();
    f.act(tree, action);
    tree.props.onClosed({
      nativeEvent: { token: f.request.id, context: f.request.field.context, reason: 'retired' },
    });
    await tick();
    assert.deepEqual(f.captures, []);
    assert.deepEqual(f.calls, ['photo', 'photo-drained']);
    assert.equal(f.keyboard.getSnapshot().presentation, null);
  }
});

test('cancelled Camera resumes the existing Photos selection and caption with a fresh native token', async () => {
  const f = await fixture(true);
  f.photos.select(item('b'));
  f.captureNext(async () => []);
  f.act(f.render(), 'caption', undefined, { text: 'Keep the selected photo and caption' });
  const tree = f.render(),
    original = f.request.id,
    photoToken = f.photos.getSnapshot().token;
  f.act(tree, 'camera');
  assert.equal(f.photos.getSnapshot().visible, false);
  await f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [2]);
  assert.equal(f.resumed.length, 1);
  assert.notEqual(f.request.id, original);
  assert.equal(f.keyboard.ownsPicker(original), true);
  assert.equal(f.photos.getSnapshot().token, photoToken);
  assert.equal(f.photos.getSnapshot().visible, true);
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  const current = f.render();
  assert.equal(
    JSON.parse(current.props.model).caption.value,
    'Keep the selected photo and caption',
  );
  assert.equal(f.picked(), undefined);
  assert.deepEqual(f.calls, ['photo']);
  // Old native view callbacks cannot address this resumed presentation.
  f.act(tree, 'send', undefined, { text: 'old Send' });
  f.act(tree, 'select', 'ph://a');
  await f.close(tree);
  assert.equal(f.resumed.length, 1);
  assert.equal(f.submissions.length, 1);
  assert.equal(f.submission()?.send, false);
  assert.deepEqual(
    f.photos.getSnapshot().selected.map((value) => value.id),
    ['ph://b'],
  );
  f.act(current, 'send', undefined, { text: 'Current final caption' });
  await tick();
  assert.equal(f.submissions.length, 2);
  const submission = f.submission();
  assert.ok(submission);
  assert.equal(submission.caption, 'Current final caption');
  assert.equal(submission.send, true);
  assert.deepEqual(f.copies, [['ph://b']]);
  await f.close(current);
  await tick();
  assert.deepEqual(
    (f.picked() as AttachmentAsset[]).map((value) => value.uri),
    ['file:///owned/b'],
  );
  assert.equal(f.keyboard.ownsPicker(original), false);
});

test('duplicate and foreign native close cannot launch Camera twice or rotate a resumed Photos owner twice', async () => {
  const f = await fixture(true),
    cancelled = deferred<AttachmentAsset[]>();
  f.captureNext(() => cancelled.promise);
  const tree = f.render();
  f.act(tree, 'camera');
  await tree.props.onClosed({
    nativeEvent: { token: 'foreign', context: f.request.field.context, reason: 'closed' },
  });
  assert.deepEqual(f.captures, []);
  const firstClose = f.close(tree),
    duplicateClose = f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [3]);
  assert.equal(f.resumed.length, 0);
  cancelled.resolve([]);
  await Promise.all([firstClose, duplicateClose]);
  await tick();
  assert.equal(f.resumed.length, 1);
  await f.close(tree);
  assert.equal(f.resumed.length, 1);
  assert.deepEqual(f.calls, ['photo']);
  f.retire();
  await tick();
});

test('retirement during cancelled Camera capture cannot reopen Photos or revive its initial token', async () => {
  const f = await fixture(true),
    cancelled = deferred<AttachmentAsset[]>();
  f.photos.select(item('a'));
  f.captureNext(() => cancelled.promise);
  const tree = f.render(),
    original = f.request.id;
  f.act(tree, 'camera');
  const closed = f.close(tree);
  await tick();
  assert.deepEqual(f.captures, [2]);
  f.retire();
  cancelled.resolve([]);
  await closed;
  await tick();
  assert.deepEqual(f.resumed, []);
  assert.equal(f.keyboard.ownsPicker(original), false);
  assert.equal(f.keyboard.getSnapshot().presentation, null);
  assert.equal(f.submissions.filter((value) => value.send).length, 0);
  assert.deepEqual(f.picked(), []);
});

test('actual Photos hook subscription retains its initial task owner across a resumed native presentation', async () => {
  type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
  const slots: Slot[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const same = (a: readonly unknown[] | undefined, b: readonly unknown[]) =>
    !!a && a.length === b.length && a.every((value, index) => Object.is(value, b[index]));
  const effect = (read: () => void | (() => void), deps: readonly unknown[]) => {
    const current = slot();
    if (same(current.deps, deps)) return;
    current.deps = deps;
    effects.push(() => {
      current.cleanup?.();
      current.cleanup = read() ?? undefined;
    });
  };
  let token = 0;
  const libraryObserver = { changed: null as (() => void) | null, subscriptions: 0 };
  const keyboard = new AttachmentKeyboardSession('chat', true, () => `hook-${++token}`);
  keyboard.bind({ tag: 7, context: 'chat', label: 'Message', placeholder: 'Message' });
  const jsx = (type: unknown, props: Record<string, any>) => ({ type, props });
  const ports: Record<string, unknown> = {
    './attachment-categories': attachmentCategories,
    './attachment-recent-session': { AttachmentRecentSession, recentAttachmentId },
    '../../modules/zoen-core/src/ZoenCoreModule': { releaseLocalFile: async () => {} },
    react: {
      useState: (initial: unknown) => {
        const current = slot();
        if (!Object.hasOwn(current, 'value')) current.value = initial;
        return [
          current.value,
          (next: unknown) => {
            current.value = typeof next === 'function' ? next(current.value) : next;
          },
        ];
      },
      useRef: (initial: unknown) => {
        const current = slot();
        return (current.value ??= { current: initial });
      },
      useMemo: (read: () => unknown, deps: readonly unknown[]) => {
        const current = slot();
        if (!same(current.deps, deps)) {
          current.value = read();
          current.deps = deps;
        }
        return current.value;
      },
      useEffect: effect,
      useSyncExternalStore: (_subscribe: unknown, read: () => unknown) => read(),
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { AppState: { addEventListener: () => ({ remove() {} }) } },
    'expo-crypto': { randomUUID: () => `hook-render-${++token}` },
    '@expo/ui': {},
    '@expo/ui/swift-ui': {},
    '@expo/ui/swift-ui/modifiers': {},
    'expo-image': {},
    'expo-camera': {},
    'expo-haptics': {},
    '../components/primitives': {},
    '../theme': {},
    '../i18n': {},
    './attachment-photo-session': { AttachmentPhotoSession },
    './attachment-photo-library': {
      photoLibrary: {
        permission: async () => ({ granted: true, status: 'granted', accessPrivileges: 'all' }),
        requestPermission: async () => {
          throw new Error('permission already granted');
        },
        load: async () => [item('a')],
        refresh: async (items: readonly PhotoLibraryItem[]) => [...items],
        albums: async () => [],
        subscribe: (changed: () => void) => {
          libraryObserver.changed = changed;
          libraryObserver.subscriptions++;
          return () => {
            libraryObserver.changed = null;
          };
        },
        manage: async () => {},
        materialize: async () => [],
        discard: () => {},
      },
    },
    './attachment-picker': { pickAttachmentAssets: async () => [] },
    './attachment-image-surface': {},
    './attachment-keyboard-context': { useAttachmentKeyboardSession: () => keyboard },
    './attachment-photo-keyboard.ios': { AttachmentPhotoKeyboard: 'AttachmentPhotoKeyboard' },
  };
  type HookResult = {
    pick(remaining: number, current: () => boolean): Promise<AttachmentAsset[]>;
    view: ReturnType<typeof jsx>;
    submission(): { caption: string; send: boolean } | null;
  };
  const module = { exports: {} as { useAttachmentPhotoPicker(): HookResult } };
  runInNewContext(pickerCode, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  const render = () => {
    cursor = 0;
    effects = [];
    const result = module.exports.useAttachmentPhotoPicker();
    effects.forEach((run) => run());
    return result;
  };
  let hook = render();
  assert.equal(libraryObserver.subscriptions, 0, 'Chat mount must not enumerate/register Photos');
  const picked = deferred<AttachmentAsset[]>();
  keyboard.register(async () => picked.resolve(await hook.pick(3, () => true)), true);
  keyboard.openAttachments();
  await tick();
  hook = render();
  assert.equal(hook.view.type, 'AttachmentPhotoKeyboard');
  const initialView = hook.view,
    session = initialView.props.session as AttachmentPhotoSession,
    original = keyboard.getSnapshot().presentation!.id;
  session.select(item('a'));
  assert.equal(libraryObserver.subscriptions, 1);
  libraryObserver.changed?.();
  await tick();
  assert.deepEqual(
    session.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  initialView.props.onSubmission({ caption: 'Retain hook submission', send: false });
  session.camera();
  keyboard.closed(original);
  await session.dismissed(session.getSnapshot().token);
  assert.equal(session.getSnapshot().visible, true);
  const resumed = keyboard.resumePicker(original);
  assert.ok(resumed);
  // resumePicker publishes synchronously. The actual mounted hook's subscription
  // must consult ownsPicker(initialId), not owns(the retired native token).
  assert.equal(session.getSnapshot().visible, true);
  assert.deepEqual(
    session.getSnapshot().selected.map((value) => value.id),
    ['ph://a'],
  );
  initialView.props.onResume(resumed);
  hook = render();
  assert.equal(hook.view.props.request.id, resumed.id);
  assert.equal(hook.submission()?.caption, 'Retain hook submission');
  keyboard.retire();
  assert.equal(session.getSnapshot().visible, false);
  assert.deepEqual(await picked.promise, []);
  for (const current of slots) current.cleanup?.();
});
