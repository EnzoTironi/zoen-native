import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import type { KeyboardEvent } from 'react';
import type { useAnchoredMenu } from '../components/use-anchored-menu.web';

type Options = Parameters<typeof useAnchoredMenu>[0];
type Slot = { value?: unknown; deps?: readonly unknown[]; cleanup?: () => void };
const require = createRequire(import.meta.url);
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../components/use-anchored-menu.web.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  external: ['react'],
});
function fixture(labels = ['Pin to Home', 'Move earlier', 'Move later', 'History']) {
  const slots: Slot[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  const same = (a?: readonly unknown[], b?: readonly unknown[]) =>
    !!a && !!b && a.length === b.length && a.every((value, i) => Object.is(value, b[i]));
  const slot = () => (slots[cursor++] ??= {});
  const events: string[] = [];
  const listeners = new Map<string, Set<unknown>>();
  const subscribe = (scope: string) => ({
    addEventListener(name: string, listener: unknown) {
      const key = `${scope}:${name}`;
      if (!listeners.has(key)) listeners.set(key, new Set());
      listeners.get(key)!.add(listener);
    },
    removeEventListener(name: string, listener: unknown) {
      listeners.get(`${scope}:${name}`)?.delete(listener);
    },
  });
  const doc: { activeElement: unknown } = { activeElement: null };
  const buttons = labels.map((label) => ({
    focus() {
      doc.activeElement = this;
      events.push(label);
    },
    getAttribute: () => label,
  }));
  const trigger = {
    getBoundingClientRect: () => ({ left: 470, right: 514, top: 360, bottom: 404 }),
    focus() {
      doc.activeElement = this;
      events.push('trigger');
    },
  };
  let nativeOpen = false;
  const panel = {
    matches(selector: string) {
      assert.equal(selector, ':popover-open');
      return nativeOpen;
    },
    style: {} as Record<string, string>,
    querySelectorAll: () => buttons,
    showPopover(input: { source: unknown }) {
      assert.equal(input.source, trigger);
      nativeOpen = true;
      events.push('show');
    },
    hidePopover() {
      nativeOpen = false;
      events.push('hide');
    },
  };
  const viewport = {
    ...subscribe('viewport'),
    offsetLeft: 10,
    offsetTop: 30,
    width: 500,
    height: 400,
  };
  const module = { exports: {} as { useAnchoredMenu: typeof useAnchoredMenu } };
  const ports = {
    useId: () => 'real-hook-menu-id',
    useRef: (initial: unknown) => {
      const s = slot();
      if (!Object.hasOwn(s, 'value')) s.value = { current: initial };
      return s.value;
    },
    useState: (initial: unknown) => {
      const s = slot();
      if (!Object.hasOwn(s, 'value')) s.value = initial;
      return [
        s.value,
        (next: unknown) => {
          s.value = next;
        },
      ];
    },
    useCallback: (value: unknown, deps: readonly unknown[]) => {
      const s = slot();
      if (!same(s.deps, deps)) {
        s.deps = deps;
        s.value = value;
      }
      return s.value;
    },
    useEffect: (read: () => (() => void) | void, deps: readonly unknown[]) => {
      const s = slot();
      if (!same(s.deps, deps)) {
        s.deps = deps;
        effects.push(() => {
          s.cleanup?.();
          s.cleanup = read() ?? undefined;
        });
      }
    },
  };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => (name === 'react' ? ports : require(name)),
    window: { ...subscribe('window'), visualViewport: viewport, innerWidth: 800, innerHeight: 600 },
    document: doc,
    getComputedStyle: () => ({ direction: 'ltr' }),
  });
  function render(options: Options = {}) {
    cursor = 0;
    effects = [];
    const result = module.exports.useAnchoredMenu(options);
    result.trigger.current = trigger as unknown as HTMLButtonElement;
    result.menu.current = panel as unknown as HTMLDivElement;
    for (const effect of effects) effect();
    return result;
  }
  function key(value: string, extras: Record<string, unknown> = {}) {
    let prevented = false;
    return {
      event: {
        key: value,
        currentTarget: trigger,
        preventDefault() {
          prevented = true;
        },
        ...extras,
      } as unknown as KeyboardEvent<HTMLButtonElement & HTMLDivElement>,
      prevented: () => prevented,
    };
  }
  const active = () =>
    doc.activeElement === trigger
      ? 'trigger'
      : labels[buttons.indexOf(doc.activeElement as (typeof buttons)[number])];
  const listenerCount = () => [...listeners.values()].reduce((sum, set) => sum + set.size, 0);
  const dispose = () => {
    for (const s of slots) s.cleanup?.();
  };
  return {
    render,
    key,
    active,
    events,
    panel,
    trigger,
    buttons,
    viewport,
    listenerCount,
    dispose,
    listeners,
  };
}

test('the actual shared hook opens at first/last, wraps and handles Home/End/typeahead without executing actions', () => {
  const f = fixture();
  const menu = f.render();
  const up = f.key('ArrowUp');
  menu.triggerProps.onKeyDown(up.event);
  assert.equal(up.prevented(), true);
  assert.equal(f.active(), 'History');
  menu.menuProps.onKeyDown(f.key('ArrowDown').event);
  assert.equal(f.active(), 'Pin to Home');
  menu.menuProps.onKeyDown(f.key('End').event);
  assert.equal(f.active(), 'History');
  menu.menuProps.onKeyDown(f.key('Home').event);
  assert.equal(f.active(), 'Pin to Home');
  menu.menuProps.onKeyDown(f.key('m').event);
  assert.equal(f.active(), 'Move earlier');
  menu.menuProps.onKeyDown(f.key('m').event);
  assert.equal(f.active(), 'Move later');
  const modified = f.key('h', { ctrlKey: true });
  menu.menuProps.onKeyDown(modified.event);
  assert.equal(modified.prevented(), false);
  assert.equal(f.active(), 'Move later');
});

test('selection dismisses and focuses before the original callback in the same synchronous activation', () => {
  const f = fixture();
  const menu = f.render();
  let active = true;
  menu.triggerProps.onKeyDown(f.key('ArrowDown').event);
  f.events.length = 0;
  menu.select(() => {
    assert.equal(active, true, 'native file picker activation must not be deferred');
    assert.equal(f.active(), 'trigger');
    f.events.push('actual callback');
  });
  active = false;
  assert.deepEqual(f.events, ['hide', 'trigger', 'actual callback']);
});

test('Tab closes then yields browser focus movement instead of trapping or preventing Tab', () => {
  const f = fixture();
  const menu = f.render();
  const tab = f.key('Tab');
  menu.menuProps.onKeyDown(tab.event);
  assert.equal(tab.prevented(), false);
  assert.deepEqual(f.events, ['hide', 'trigger']);
});

test('visual viewport positioning stays bounded and end aligned, including above/below changes', () => {
  const f = fixture();
  const menu = f.render({ align: 'end' });
  menu.menuProps.onBeforeToggle({ newState: 'open' });
  assert.equal(f.panel.style.left, '250px');
  assert.equal(f.panel.style.width, '248px');
  assert.equal(f.panel.style.bottom, '248px');
  assert.equal(f.panel.style.maxHeight, '310px');
  assert.equal(f.panel.style.transformOrigin, 'right bottom');
  f.viewport.height = 900;
  menu.menuProps.onBeforeToggle({ newState: 'open' });
  assert.equal(f.panel.style.top, '412px');
  assert.equal(f.panel.style.bottom, 'auto');
});

test('listeners exist only while open, do not accumulate and are removed on close or disposal', () => {
  const f = fixture();
  let menu = f.render();
  assert.equal(f.listenerCount(), 0);
  menu.menuProps.onToggle({ newState: 'open' });
  menu = f.render();
  assert.equal(f.listenerCount(), 4);
  menu = f.render();
  assert.equal(f.listenerCount(), 4);
  menu.menuProps.onToggle({ newState: 'closed' });
  menu = f.render();
  assert.equal(f.listenerCount(), 0);
  menu.menuProps.onToggle({ newState: 'open' });
  f.render();
  assert.equal(f.listenerCount(), 4);
  f.dispose();
  assert.equal(f.listenerCount(), 0);
});

test('disabled menus dismiss and reject mouse/keyboard callback activation, including lost Home callbacks', () => {
  const f = fixture();
  let menu = f.render();
  menu.menuProps.onToggle({ newState: 'open' });
  f.render();
  assert.equal(f.listenerCount(), 4);
  menu = f.render({ disabled: true });
  menu.select(() => assert.fail('disabled action must not fire'));
  menu.triggerProps.onKeyDown(f.key('ArrowDown').event);
  assert.equal(f.events.filter((event) => event === 'show').length, 0);
  menu = f.render({ disabled: true });
  assert.equal(menu.triggerProps['aria-expanded'], false);
  assert.equal(f.listenerCount(), 0);
});

test('an empty or removed item list tolerates keyboard navigation', () => {
  const f = fixture([]);
  const menu = f.render();
  for (const key of ['ArrowUp', 'ArrowDown', 'Home', 'End', 'x'])
    menu.menuProps.onKeyDown(f.key(key).event);
  assert.equal(f.active(), undefined);
  assert.deepEqual(f.events, []);
});

test('native closed or retired menu callbacks cannot act again before the queued toggle event arrives', () => {
  const f = fixture();
  const menu = f.render();
  let calls = 0;
  menu.select(() => calls++);
  assert.equal(calls, 0, 'a never-open native popover cannot select');
  menu.triggerProps.onKeyDown(f.key('ArrowDown').event);
  menu.select(() => calls++);
  menu.select(() => calls++);
  assert.equal(calls, 1, 'native closure fences a second callback synchronously');
  menu.menu.current = null;
  menu.select(() => calls++);
  assert.equal(calls, 1, 'retired DOM cannot reactivate a callback');
});
