import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import test from 'node:test';
import { build } from 'esbuild';
import { homeCardActions } from '../home/card-actions.ts';
import type { HomeCardMenuProps } from '../home/menus.types';
import { createLocalization } from '../i18n/runtime.ts';
import { home } from '../i18n/catalogs/home.ts';
import { conversationCards } from '../i18n/catalogs/conversation-cards.ts';
import { miniapps } from '../i18n/catalogs/miniapps.ts';

const callbacks = [
  ['home-pin', 'onToggle'],
  ['conversation-pin', 'onToggleConversation'],
  ['conversation-organize', 'onOrganizeConversation'],
  ['move-earlier', 'onMoveEarlier'],
  ['move-later', 'onMoveLater'],
  ['history', 'onHistory'],
] as const;

test('every optional Home action stays independently reachable, including history with pin', () => {
  for (let mask = 0; mask < 64; mask++) {
    const calls: string[] = [];
    const props: HomeCardMenuProps = { name: 'A very long shared app title', pinned: false };
    const expected: string[] = [];
    callbacks.forEach(([id, key], index) => {
      if (mask & (1 << index)) {
        expected.push(id);
        props[key] = () => calls.push(id);
      }
    });
    const before = { ...props };
    const actions = homeCardActions(props);
    assert.deepEqual(
      actions.map((action) => action.id),
      expected,
      `callback set ${mask}`,
    );
    assert.deepEqual(calls, [], 'building a menu must not execute any action');
    for (const action of actions) {
      const key = callbacks.find(([id]) => id === action.id)![1];
      assert.equal(action.onPress, props[key], 'the renderer receives the original callback');
      action.onPress();
    }
    assert.deepEqual(calls, expected, 'each action invokes only its own callback once');
    assert.deepEqual(props, before, 'building actions must not mutate the authoritative props');
  }
});

test('an updated pin state changes labels without preserving stale callbacks or local pin state', () => {
  const calls: string[] = [];
  const before = homeCardActions({
    name: 'Drawing',
    pinned: false,
    onToggle: () => calls.push('old'),
    onToggleConversation: () => calls.push('old conversation'),
  });
  const after = homeCardActions({
    name: 'Drawing',
    pinned: true,
    conversationPinned: true,
    onToggle: () => calls.push('current'),
    onToggleConversation: () => calls.push('current conversation'),
  });
  assert.deepEqual(
    before.map((action) => action.label),
    ['Pin to Home', 'Show in this chat'],
  );
  assert.deepEqual(
    after.map((action) => action.label),
    ['Unpin from Home', 'Hide from this chat'],
  );
  after.forEach((action) => action.onPress());
  assert.deepEqual(calls, ['current', 'current conversation']);
});

const require = createRequire(import.meta.url);
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../home/card-actions.ts', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'home-actions-localization',
      setup(builder) {
        builder.onResolve({ filter: /^\.\.\/i18n\/index\.ts$/ }, ({ path }) => ({
          path,
          external: true,
        }));
      },
    },
  ],
});

test('the same action contract uses real EN/PT catalogs and keeps titles out of action labels', () => {
  for (const [locale, expected] of [
    [
      'en',
      [
        'Unpin from Home',
        'Hide from this chat',
        'Manage cards',
        'Move earlier',
        'Move later',
        'History',
      ],
    ],
    [
      'pt-BR',
      [
        'Desafixar do início',
        'Ocultar nesta conversa',
        'Organizar cards',
        'Mover para antes',
        'Mover para depois',
        'Histórico',
      ],
    ],
  ] as const) {
    const module = { exports: {} as { homeCardActions: typeof homeCardActions } };
    const localization = createLocalization({ home, conversationCards, miniapps }, [locale]);
    runInNewContext(compiled.outputFiles[0]!.text, {
      module,
      exports: module.exports,
      require: (name: string) => (name === '../i18n/index.ts' ? localization : require(name)),
    });
    const props: HomeCardMenuProps = {
      name: 'A title that must never widen the trigger',
      pinned: true,
      conversationPinned: true,
    };
    for (const [, key] of callbacks) props[key] = () => assert.fail('rendering cannot act');
    const labels = module.exports.homeCardActions(props).map((action) => action.label);
    assert.deepEqual(Array.from(labels), expected, locale);
    assert.ok(labels.every((label) => !label.includes(props.name)));
  }
});
