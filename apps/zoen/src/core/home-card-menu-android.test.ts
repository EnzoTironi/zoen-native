import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import type { HomeCardMenuProps } from '../home/menus.types';

const require = createRequire(import.meta.url);
const source = fileURLToPath(new URL('../home/menus.android.tsx', import.meta.url));
const compiled = await build({
  entryPoints: [source],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  jsx: 'automatic',
  write: false,
  plugins: [
    {
      name: 'native-menu-ports',
      setup(builder) {
        builder.onResolve(
          {
            filter:
              /^(react(?:\/jsx-runtime)?|react-native|expo-router|@expo\/ui(?:\/jetpack-compose(?:\/modifiers)?)?|\.\.\/theme|\.\.\/components\/primitives|\.\.\/i18n\/index\.ts)$/,
          },
          ({ path }) => ({ path, external: true }),
        );
        builder.onResolve({ filter: /more-horizontal\.xml$/ }, ({ path }) => ({
          path,
          external: true,
        }));
        builder.onResolve({ filter: /^\.\/card-actions$/ }, () => ({
          path: fileURLToPath(new URL('../home/card-actions.ts', import.meta.url)),
        }));
      },
    },
  ],
});
type Element = { type: unknown; props: Record<string, any> };
type Slot = { value?: any; deps?: readonly unknown[]; cleanup?: () => void };
const Menu = Object.assign(() => {}, { Trigger: 'MenuTrigger', Items: 'MenuItems' });
const Item = Object.assign(() => {}, { Text: 'ItemText' });
function fixture() {
  const slots: Slot[] = [];
  let cursor = 0;
  let effects: (() => void)[] = [];
  const slot = () => (slots[cursor++] ??= {});
  const react = {
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
    useRef: (initial: unknown) => {
      const s = slot();
      if (!Object.hasOwn(s, 'value')) s.value = { current: initial };
      return s.value;
    },
    useLayoutEffect: (effect: () => void | (() => void), deps: readonly unknown[]) => {
      const s = slot();
      if (s.deps?.length === deps.length && s.deps.every((v, i) => Object.is(v, deps[i]))) return;
      s.deps = deps;
      effects.push(() => {
        s.cleanup?.();
        s.cleanup = effect() ?? undefined;
      });
    },
  };
  const jsx = (type: unknown, props: Record<string, unknown>) => ({ type, props });
  const ports: Record<string, unknown> = {
    react,
    'react/jsx-runtime': { jsx, jsxs: jsx },
    'react-native': { View: 'View' },
    '@expo/ui': { Host: 'NativeHost' },
    '@expo/ui/jetpack-compose': {
      DropdownMenu: Menu,
      DropdownMenuItem: Item,
      FilledTonalIconButton: 'NativeIconButton',
      Icon: 'NativeIcon',
      Text: 'NativeText',
    },
    '@expo/ui/jetpack-compose/modifiers': {
      size: (width: number, height: number) => ({ size: [width, height] }),
      semantics: (value: unknown) => ({ semantics: value }),
      testID: (value: unknown) => ({ testID: value }),
    },
    '../theme': {
      useTheme: () => ({ scheme: 'dark', colors: { accent: '#9ACB87', raised: '#17212DF5' } }),
    },
    '../i18n/index.ts': {
      t: (key: string, args?: { name: string }) => (args ? `${key}: ${args.name}` : key),
    },
    '../components/primitives': { NativeButton: 'NativeButton' },
    'expo-router': { router: { push: () => assert.fail('card actions cannot invent navigation') } },
  };
  const module = { exports: {} as { HomeCardMenu: (props: HomeCardMenuProps) => Element | null } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => (name.endsWith('.xml') ? 1 : (ports[name] ?? require(name))),
  });
  function render(props: Partial<HomeCardMenuProps> = {}) {
    cursor = 0;
    effects = [];
    const tree = module.exports.HomeCardMenu({
      name: 'An extremely long app title must not become the button',
      pinned: false,
      testID: 'menu-item-one',
      ...props,
    });
    for (const effect of effects) effect();
    return tree;
  }
  return {
    render,
    unmount() {
      for (const s of slots) s.cleanup?.();
    },
  };
}
function nodes(tree: unknown, type: unknown): Element[] {
  if (Array.isArray(tree)) return tree.flatMap((child) => nodes(child, type));
  if (!tree || typeof tree !== 'object') return [];
  const node = tree as Element;
  return [...(node.type === type ? [node] : []), ...nodes(node.props?.children, type)];
}
function only(tree: unknown, type: unknown) {
  const found = nodes(tree, type);
  assert.equal(found.length, 1);
  return found[0]!;
}

test('native options have a fixed Material tap area and complete name independent of title length', () => {
  const f = fixture();
  const tree = f.render({ onToggle() {} });
  const button = only(tree, 'NativeIconButton');
  assert.deepEqual(JSON.parse(JSON.stringify(button.props.modifiers[0])), { size: [48, 48] });
  assert.match(
    button.props.modifiers[1].semantics.contentDescription,
    /home.options: An extremely long/,
  );
  assert.equal(button.props.modifiers[2].testID, 'menu-item-one');
  assert.equal(only(tree, 'NativeIcon').props.size, 24);
  assert.equal(nodes(tree, 'NativeButton').length, 0);
  assert.deepEqual(JSON.parse(JSON.stringify(only(tree, Menu).props.properties)), {
    focusable: true,
    dismissOnBackPress: true,
    dismissOnClickOutside: true,
    clippingEnabled: false,
  });
});

test('every supplied action is independently reachable once and native dismiss leaves no action active', () => {
  const callbacks = [
    'onToggle',
    'onToggleConversation',
    'onOrganizeConversation',
    'onMoveEarlier',
    'onMoveLater',
    'onHistory',
  ] as const;
  for (let index = 0; index < callbacks.length; index++) {
    const f = fixture();
    const calls: string[] = [];
    const props = Object.fromEntries(callbacks.map((key) => [key, () => calls.push(key)]));
    let tree = f.render(props);
    assert.equal(nodes(tree, Item).length, 6);
    only(tree, 'NativeIconButton').props.onClick();
    tree = f.render(props);
    assert.equal(only(tree, Menu).props.expanded, true);
    const action = nodes(tree, Item)[index]!;
    action.props.onClick();
    action.props.onClick();
    assert.deepEqual(calls, [callbacks[index]]);
    tree = f.render(props);
    assert.equal(only(tree, Menu).props.expanded, false);
    only(tree, 'NativeIconButton').props.onClick();
    tree = f.render(props);
    only(tree, Menu).props.onDismissRequest();
    nodes(tree, Item)[index]!.props.onClick();
    assert.deepEqual(calls, [callbacks[index]]);
  }
});

test('history and conversation-only menus survive; no callbacks has no trigger', () => {
  const f = fixture();
  assert.equal(f.render(), null);
  assert.equal(nodes(f.render({ onHistory() {} }), Item).length, 1);
  assert.equal(nodes(f.render({ onOrganizeConversation() {} }), Item).length, 1);
  assert.equal(nodes(f.render({ onToggle() {}, onHistory() {} }), Item).length, 2);
});

test('late callbacks from a replaced card or unmounted popup cannot execute an action', () => {
  const f = fixture();
  const calls: string[] = [];
  const props = { onToggle: () => calls.push('original') };
  let tree = f.render(props);
  only(tree, 'NativeIconButton').props.onClick();
  tree = f.render(props);
  const oldAction = only(tree, Item).props.onClick;
  tree = f.render({ testID: 'replacement-card', onToggle: () => calls.push('replacement') });
  oldAction();
  assert.equal(calls.length, 0);
  tree = f.render({ testID: 'replacement-card', onToggle: () => calls.push('replacement') });
  only(tree, 'NativeIconButton').props.onClick();
  tree = f.render({ testID: 'replacement-card', onToggle: () => calls.push('replacement') });
  const replacementAction = only(tree, Item).props.onClick;
  f.unmount();
  replacementAction();
  assert.equal(calls.length, 0);
});
