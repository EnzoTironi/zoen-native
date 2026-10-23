import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  act,
  actions,
  elements,
  fixture,
  message,
  quote,
} from './message-accessibility.fixtures.ts';
import type { TimelineEntry } from './types';

test('VoiceOver standard activation opens the real row menu with a localized author action', async () => {
  const entry = message();
  const f = fixture(entry);
  const { group } = await f.enable();
  assert.equal(group.props.accessible, true);
  assert.equal(group.props.accessibilityRole, 'button');
  assert.equal(group.props.accessibilityHint, 'Message actions for Nina Author');
  assert.equal(
    actions(group)?.find((action) => action.name === 'actions')?.label,
    'Message actions for Nina Author',
  );
  assert.equal(typeof group.props.onAccessibilityTap, 'function');
  (group.props.onAccessibilityTap as () => void)();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0]?.action, 'menu');
  assert.equal(f.calls[0]?.mode, 'actions');
  assert.equal(f.calls[0]?.entry, entry);
  f.unmount();
});

test('custom quote, reply and thread actions use the current real row after recycling', async () => {
  const first = message({ reply_to: quote('original-target') });
  const f = fixture(first);
  let { group } = await f.enable();
  assert.ok(actions(group)?.some((action) => action.name === 'quote'));
  act(group, 'quote');
  assert.equal(f.calls[0]?.target, 'original-target');
  const next = message({ id: 'recycled-row', reply_to: quote('current-target') });
  ({ group } = f.recycle(next));
  act(group, 'quote');
  act(group, 'reply');
  act(group, 'thread');
  act(group, 'actions');
  assert.equal(f.calls[1]?.target, 'current-target');
  assert.equal(f.calls[2]?.entry, next);
  assert.equal(f.calls[3]?.entry, next);
  assert.equal(f.calls[4]?.entry, next);
  assert.equal(f.calls[4]?.mode, 'actions');
  ({ group } = f.recycle(message({ id: 'unquoted-row' })));
  assert.ok(!actions(group)?.some((action) => action.name === 'quote'));
  const count = f.calls.length;
  act(group, 'quote');
  act(group, 'unknown-native-action');
  assert.equal(f.calls.length, count);
  assert.equal(f.registrations(), 1, 'Recycling does not remount the screen-reader subscription');
  assert.equal(f.queries(), 1);
  f.unmount();
});

test('the grouped label retains actual author, body, quoted author/body, edit time and own receipt', async () => {
  const base = message();
  const mine = message({
    author: { ...base.author, name: 'Own Author', is_me: true },
    reply_to: quote('quoted-seed', 'Exact quoted text 🪴'),
    edited: true,
    forwarded: true,
    delivery: 'Read',
  });
  const f = fixture(mine);
  const { group } = await f.enable();
  const label = group.props.accessibilityLabel as string;
  for (const expected of [
    'Own Author: Canonical message body 🌤️',
    'Replying to Quoted quoted-seed: Exact quoted text 🪴',
    'Forwarded',
    `Edited at ${new Date(Number(mine.at_ms)).toLocaleTimeString('en-US', { hour: '2-digit', minute: '2-digit' })}`,
    'Read by everyone',
  ])
    assert.ok(label.includes(expected), `Missing spoken metadata: ${expected}`);
  const received = f.recycle({ ...mine, author: { ...mine.author, is_me: false } });
  assert.ok(!(received.group.props.accessibilityLabel as string).includes('Read by everyone'));
  f.unmount();
});

test('disabled and deleted rows remain readable static text with no advertised or executable actions', async () => {
  for (const deleted of [false, true]) {
    const entry = message({
      deleted,
      reply_to: quote('private-target', 'Private quoted body'),
    });
    const f = fixture(entry, !deleted);
    const { group, bubble } = await f.enable();
    assert.equal(group.props.accessible, true);
    assert.equal(group.props.accessibilityRole, 'text');
    assert.equal((group.props.accessibilityState as { disabled: boolean }).disabled, true);
    assert.equal(group.props.accessibilityHint, undefined);
    assert.equal(group.props.onAccessibilityTap, undefined);
    assert.equal(actions(group), undefined);
    assert.equal(bubble.props.accessibilityHint, undefined);
    for (const action of ['reply', 'thread', 'quote', 'actions']) act(group, action);
    (bubble.props.onAccessibilityTap as () => void)();
    assert.equal(f.calls.length, 0);
    if (deleted) {
      assert.equal(bubble.props.accessibilityRole, 'text');
      const label = group.props.accessibilityLabel as string;
      assert.ok(label.includes('Message deleted'));
      assert.ok(!label.includes('Canonical message body'));
      assert.ok(!label.includes('Private quoted body'));
    }
    f.unmount();
  }
});

test('native media and miniapp boundaries remain ungrouped and never hide independent child controls', async () => {
  const file = message({
    kind: {
      Message: {
        text: 'File caption',
        card: { item_id: 'file', kind_id: 'file', title: 'photo.jpg', version: 1 },
      },
    } as TimelineEntry['kind'],
  });
  const miniapp = message({
    kind: {
      Message: {
        text: 'Shared miniapp',
        card: {
          item_id: 'app',
          kind_id: 'app',
          title: 'Schedule',
          version: 1,
          app: { app_id: 'list' },
        },
      },
    } as TimelineEntry['kind'],
  });
  const voice = message({ voice: { item_id: 'voice' } as TimelineEntry['voice'] });
  for (const entry of [file, miniapp, voice]) {
    const f = fixture(entry);
    const { group, bubble, wrapper } = await f.enable();
    assert.equal(group.props.accessible, false);
    assert.equal(group.props.accessibilityRole, undefined);
    assert.equal(group.props.onAccessibilityTap, undefined);
    assert.equal(actions(group), undefined);
    assert.equal(group.props.accessibilityElementsHidden, undefined);
    assert.notEqual(group.props.importantForAccessibility, 'no-hide-descendants');
    assert.equal(bubble.type, 'View');
    assert.equal(bubble.props.accessible, false);
    assert.ok(elements(wrapper).some((node) => node.type === 'MessageMedia'));
    if (entry === miniapp) {
      assert.ok(elements(wrapper).some((node) => node.type === 'MiniappMessageCard'));
      assert.equal((bubble.props.style as Record<string, unknown>).width, 160);
      assert.equal(
        elements(wrapper).find((node) => node.type === 'MiniappMessageCard')?.props.width,
        160,
        'Human shares have no outer message rim or caption wrapper',
      );
    }
    act(group, 'actions');
    assert.equal(f.calls.length, 0);
    f.unmount();
  }
});

test('Web App shares retain a visible author menu while iOS keeps the native hold presentation', () => {
  const entry = message({
    kind: {
      Message: {
        text: 'Shared canvas',
        card: {
          item_id: 'shared-canvas',
          kind_id: 'app',
          title: 'Drawing',
          version: 1,
          app: { app_id: 'excalidraw' },
        },
      },
    } as TimelineEntry['kind'],
  });
  const label = 'Message actions for Nina Author';
  for (const platform of ['ios', 'web'] as const) {
    const f = fixture(entry, false, platform);
    const controls = () =>
      elements(f.render().tree).filter((node) => node.props.accessibilityLabel === label);
    const menu = controls();
    assert.equal(menu.length, platform === 'web' ? 1 : 0);
    if (platform === 'web') {
      assert.equal(menu[0]!.props.accessibilityRole, 'button');
      (menu[0]!.props.onPress as () => void)();
      assert.deepEqual(
        f.calls.map(({ action, mode }) => ({ action, mode })),
        [{ action: 'menu', mode: 'actions' }],
      );
      assert.equal(f.calls[0]!.entry, entry);
      f.recycle(entry, true);
      assert.equal(controls().length, 0, 'Disabled shares expose no executable action');
      f.recycle({ ...entry, deleted: true });
      assert.equal(controls().length, 0, 'Deleted shares cannot retain an App menu');
    }
    f.unmount();
  }
});
