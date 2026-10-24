import assert from 'node:assert/strict';
import { test } from 'node:test';
import { communityUi, nodes, strings, settle } from './community-members-ui.fixtures.ts';
import { peer, deferred } from './community-members.fixtures.ts';

for (const locale of ['en', 'pt-BR'] as const)
  test(`${locale}: real member row opens scoped native sheet and the exact named removal confirmation`, async () => {
    const f = await communityUi(locale);
    let screen = f.render();
    const row = nodes(screen.tree).find((node) => node.props.testID === `details-member-${peer}`)!;
    assert.equal(row.props.accessibilityActions[0].label, f.t('community.actions'));
    row.props.onAccessibilityAction({ nativeEvent: { actionName: 'showMenu' } });
    await settle();
    screen = f.render();
    assert.equal(
      nodes(screen.sheet).find((node) => node.type === 'Host')!.props.pointerEvents,
      'none',
    );
    assert.equal(
      nodes(screen.sheet).find((node) => node.type === 'BottomSheet')!.props.isPresented,
      true,
    );
    assert.equal(nodes(screen.sheet).filter((node) => node.type === 'ScrollView').length, 1);
    assert.equal(
      nodes(screen.sheet).find((node) => node.type === 'ScrollView')!.props
        .contentInsetAdjustmentBehavior,
      'automatic',
    );
    assert.equal(
      strings(nodes(screen.sheet).find((node) => node.props.accessibilityRole === 'header')),
      'Selected',
    );
    f.confirm(async () => true);
    nodes(screen.sheet)
      .find((node) => node.props.testID === 'member-remove')!
      .props.onPress();
    await settle();
    assert.equal(f.dialogs[0].title, f.t('community.removeNamed', { name: 'Selected' }));
    assert.equal(
      f.dialogs[0].message,
      f.t('community.removeFrom', { title: 'Selected community' }),
    );
    assert.ok(!JSON.stringify(f.dialogs).includes('%{'));
    const writes = f.calls.filter((call) => call.op === 'remove_member_reviewed');
    assert.equal(writes.length, 1);
    assert.equal(writes[0].identity_id, peer);
    assert.equal(writes[0].expected_identity_id, f.snapshot.account!.identity_id);
    assert.equal(writes[0].expected_device_id, f.snapshot.account!.device_id);
    screen = f.render();
    assert.equal(
      nodes(screen.sheet).find((node) => node.props.testID === 'member-remove')!.props.disabled,
      true,
    );
    assert.equal(
      nodes(screen.sheet).find((node) => node.props.testID === 'member-remove')!.props.label,
      f.t('community.removing'),
    );
    assert.ok(
      !f.store
        .getSnapshot()
        .snapshot!.conversations[0]!.members.some((member) => member.id === peer),
    );
    assert.equal(
      screen.props!.state.context.state,
      'Removing',
      'optimistic list omission is not confirmed removal',
    );
    await f.close();
  });

test('real sheet hides removal after current permission loss and keeps ordinary profile navigation', async () => {
  const f = await communityUi();
  const row = nodes(f.render().tree).find(
    (node) => node.props.testID === `details-member-${peer}`,
  )!;
  row.props.onPress();
  assert.equal((f.routes[0] as any).pathname, '/person/[id]');
  f.value({ ...f.context(), can_remove: false, role: 'owner' });
  row.props.onLongPress();
  await settle();
  const screen = f.render();
  assert.equal(
    nodes(screen.sheet).filter((node) => node.props.testID === 'member-remove').length,
    0,
  );
  const rows = nodes(screen.sheet).find((node) => node.type === 'SettingsRows')!.props.rows;
  rows[0].onPress();
  assert.equal((f.routes.at(-1) as any).params.id, peer);
  assert.equal(f.calls.filter((call) => call.op === 'remove_member_reviewed').length, 0);
  await f.close();
});

test('actual foreground hook retires late confirmation and does not reopen it automatically', async () => {
  const f = await communityUi();
  const row = nodes(f.render().tree).find(
    (node) => node.props.testID === `details-member-${peer}`,
  )!;
  row.props.onLongPress();
  await settle();
  const screen = f.render();
  const answer = deferred<boolean>();
  f.confirm(() => answer.promise);
  nodes(screen.sheet)
    .find((node) => node.props.testID === 'member-remove')!
    .props.onPress();
  f.foreground('background');
  answer.resolve(true);
  await settle();
  assert.equal(f.calls.filter((call) => call.op === 'remove_member_reviewed').length, 0);
  assert.equal(f.render().props!.state.selected, null);
  f.foreground('active');
  await settle();
  assert.equal(f.render().props!.state.selected, null);
  await f.close();
});

test('Direct and self rows retain their separate intended controls', async () => {
  const f = await communityUi();
  f.room.kind = 'Direct';
  // Snapshot is the real store projection, not the fixture source object.
  const snapshot = f.store.getSnapshot().snapshot!;
  snapshot.conversations[0]!.kind = 'Direct';
  let tree = f.render().tree;
  let row = nodes(tree).find((node) => node.props.testID === `details-member-${peer}`)!;
  assert.equal(row.props.onLongPress, undefined);
  assert.equal(row.props.accessibilityActions, undefined);
  snapshot.conversations[0]!.kind = 'Community';
  tree = f.render().tree;
  row = nodes(tree).find(
    (node) => node.props.testID === `details-member-${snapshot.account!.identity_id}`,
  )!;
  assert.equal(row.props.onLongPress, undefined);
  await f.close();
});

test('web keeps profile navigation without advertising unsupported native member controls', async () => {
  const f = await communityUi('en', 'web');
  const screen = f.render();
  const row = nodes(screen.tree).find((node) => node.props.testID === `details-member-${peer}`)!;
  assert.equal(row.props.onLongPress, undefined);
  assert.equal(row.props.accessibilityActions, undefined);
  assert.equal(screen.sheet, null);
  row.props.onPress();
  assert.equal((f.routes[0] as any).params.id, peer);
  assert.equal(f.calls.length, 0);
  await f.close();
});
