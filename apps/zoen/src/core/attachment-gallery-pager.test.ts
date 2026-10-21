import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';

type Element = { type: unknown; props: Record<string, unknown> };
type Item = { mutationId: string };
type Effect = () => void | (() => void);
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../media/attachment-gallery-pager.tsx', import.meta.url),
      'utf8',
    ),
    loader: 'tsx',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'actual-pager-native-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});

function fixture(ids = ['a', 'b', 'c', 'd']) {
  let cursor = 0;
  const slots: Slot[] = [];
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    Boolean(a && b && a.length === b.length && a.every((v, i) => Object.is(v, b[i])));
  const selections: string[] = [];
  const scrolls: { x: number; animated: boolean }[] = [];
  const previews: { id: string; active: boolean }[] = [];
  let drags = 0;
  let current = true;
  const options = {
    items: ids.map((mutationId) => ({ mutationId })),
    selectedId: ids[0]!,
    disabled: false,
    reducedMotion: false,
    isCurrent: () => current,
    onSelect: (id: string) => selections.push(id),
    onDragStart: () => drags++,
    renderItem: (item: Item, active: boolean) => {
      previews.push({ id: item.mutationId, active });
      return { type: 'Preview', props: { id: item.mutationId, active } };
    },
  };
  const jsx = (type: unknown, props: Record<string, unknown>): Element => ({ type, props });
  const ports: Record<string, unknown> = {
    react: {
      useRef: (initial: unknown) => {
        const s = slot();
        s.value ??= { current: initial };
        return s.value;
      },
      useState: (initial: unknown) => {
        const s = slot();
        if (!Object.hasOwn(s, 'value')) s.value = initial;
        return [
          s.value,
          (value: unknown) => {
            s.value = value;
          },
        ];
      },
      useMemo: (read: () => unknown, deps: readonly unknown[]) => {
        const s = slot();
        if (!same(s.deps, deps)) {
          s.deps = deps;
          s.value = read();
        }
        return s.value;
      },
      useLayoutEffect: (read: Effect, deps: readonly unknown[]) => {
        const s = slot();
        if (same(s.deps, deps)) return;
        s.deps = deps;
        effects.push(() => {
          s.cleanup?.();
          s.cleanup = read() ?? undefined;
        });
      },
    },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { ScrollView: 'ScrollView', View: 'View' },
  };
  const module = { exports: {} as { AttachmentGalleryPager(props: typeof options): Element } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let root: Element;
  const scan = (value: unknown): Element[] => {
    if (Array.isArray(value)) return value.flatMap(scan);
    if (!value || typeof value !== 'object' || !('props' in value)) return [];
    const e = value as Element;
    return [e, ...scan(e.props.children)];
  };
  const find = (id: string) => {
    const e = scan(root).find((e) => e.props.testID === id);
    assert.ok(e, id);
    return e;
  };
  const render = (patch: Partial<typeof options> = {}) => {
    Object.assign(options, patch);
    cursor = 0;
    effects = [];
    previews.length = 0;
    root = module.exports.AttachmentGalleryPager(options);
    const ref = find('attachment-gallery-pager').props.ref as { current: unknown };
    ref.current = {
      scrollTo: (value: { x: number; animated: boolean }) =>
        scrolls.push({ x: value.x, animated: value.animated }),
    };
    for (const run of effects) run();
    return root;
  };
  const measure = (width: number, height = 874) =>
    (find('attachment-gallery-viewport').props.onLayout as (e: unknown) => void)({
      nativeEvent: { layout: { width, height } },
    });
  const settled = () =>
    find('attachment-gallery-pager').props.onMomentumScrollEnd as (e: unknown) => void;
  const offset = (callback: (e: unknown) => void, x: number, begin = true) => {
    if (begin) (find('attachment-gallery-pager').props.onScrollBeginDrag as () => void)();
    callback({ nativeEvent: { contentOffset: { x } } });
  };
  render();
  measure(402);
  render();
  return {
    render,
    measure,
    find,
    settled,
    offset,
    selections,
    previews,
    scrolls,
    retire: () => {
      current = false;
    },
    drag: () => (find('attachment-gallery-pager').props.onScrollBeginDrag as () => void)(),
    drags: () => drags,
    unmount: () => {
      for (const s of slots) s.cleanup?.();
    },
  };
}

test('actual native pager selects the exact settled asset and keeps only nearby previews with one active owner', () => {
  const f = fixture();
  assert.deepEqual(f.previews, [
    { id: 'a', active: true },
    { id: 'b', active: false },
  ]);
  f.offset(f.settled(), 402);
  assert.deepEqual(f.selections, ['b']);
  f.render({ selectedId: 'b' });
  assert.deepEqual(f.previews, [
    { id: 'a', active: false },
    { id: 'b', active: true },
    { id: 'c', active: false },
  ]);
  assert.equal(f.find('attachment-gallery-page-0').props.accessibilityElementsHidden, true);
  assert.equal(f.find('attachment-gallery-page-1').props.accessibilityElementsHidden, false);
  f.drag();
  assert.equal(f.drags(), 2);
});

test('queued settled events cannot select after reordering, removal or replacement of the rendered draft assets', () => {
  for (const ids of [
    ['b', 'a', 'c'],
    ['a', 'c'],
    ['replacement', 'b', 'c'],
  ]) {
    const f = fixture();
    const stale = f.settled();
    f.render({ items: ids.map((mutationId) => ({ mutationId })), selectedId: ids[0]! });
    f.offset(stale, 402);
    assert.deepEqual(f.selections, []);
    f.offset(f.settled(), 402);
    assert.deepEqual(f.selections, [ids[1]]);
  }
});

test('rotation A to B to A fences old native events before any React commit, then converges to the current exact page width', () => {
  const f = fixture();
  const stale = f.settled();
  f.measure(874, 402);
  f.measure(402, 874);
  f.offset(stale, 402);
  assert.deepEqual(f.selections, []);
  f.render();
  assert.equal((f.find('attachment-gallery-page-0').props.style as { width: number }).width, 402);
  assert.deepEqual(f.scrolls.at(-1), { x: 0, animated: false });
  f.offset(stale, 402);
  assert.deepEqual(f.selections, []);
  f.offset(f.settled(), 402);
  assert.deepEqual(f.selections, ['b']);
});

test('native callbacks cannot select or reclaim preview after immediate privacy retirement, busy state or unmount', () => {
  for (const boundary of ['retire', 'busy', 'unmount']) {
    const f = fixture();
    const stale = f.settled();
    if (boundary === 'retire') f.retire();
    if (boundary === 'busy') f.render({ disabled: true });
    if (boundary === 'unmount') f.unmount();
    f.offset(stale, 402);
    f.drag();
    assert.deepEqual(f.selections, []);
    assert.equal(f.drags(), 0);
  }
});

test('programmatic thumbnail navigation animates only within stable geometry and respects reduced motion', () => {
  const f = fixture();
  assert.deepEqual(f.scrolls[0], { x: 0, animated: false });
  f.render({ selectedId: 'b' });
  assert.deepEqual(f.scrolls.at(-1), { x: 402, animated: true });
  f.render({ selectedId: 'c', reducedMotion: true });
  assert.deepEqual(f.scrolls.at(-1), { x: 804, animated: false });
  f.measure(874, 402);
  f.render();
  assert.deepEqual(f.scrolls.at(-1), { x: 1748, animated: false });
});

test('invalid offsets and duplicate current-page events never change the draft selection', () => {
  const f = fixture();
  for (const x of [NaN, Infinity, -402, 402 * 5, 0]) f.offset(f.settled(), x);
  assert.deepEqual(f.selections, []);
});

test('held page completion cannot override a newer explicit thumbnail selection in the same ordered draft', () => {
  const f = fixture();
  const held = f.settled();
  f.render({ selectedId: 'b' });
  f.offset(held, 804);
  assert.deepEqual(f.selections, []);
  f.offset(f.settled(), 804);
  assert.deepEqual(f.selections, ['c']);
});

test('programmatic completion cannot select, while exact nonmomentum user release selects once', () => {
  const f = fixture();
  f.offset(f.settled(), 402, false);
  assert.deepEqual(f.selections, []);
  f.drag();
  const end = f.find('attachment-gallery-pager').props.onScrollEndDrag as (event: unknown) => void;
  end({
    nativeEvent: { contentOffset: { x: 402 }, targetContentOffset: { x: 402 }, velocity: { x: 0 } },
  });
  assert.deepEqual(f.selections, ['b']);
  f.offset(f.settled(), 402, false);
  assert.deepEqual(f.selections, ['b']);
});

test('fractional moving release waits for momentum settlement and never selects twice', () => {
  const f = fixture();
  f.drag();
  const end = f.find('attachment-gallery-pager').props.onScrollEndDrag as (event: unknown) => void;
  end({
    nativeEvent: {
      contentOffset: { x: 270 },
      targetContentOffset: { x: 402 },
      velocity: { x: 0.4 },
    },
  });
  assert.deepEqual(f.selections, []);
  f.offset(f.settled(), 402, false);
  f.offset(f.settled(), 402, false);
  assert.deepEqual(f.selections, ['b']);
});
