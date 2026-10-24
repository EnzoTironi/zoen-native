import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Handler } from './telegram-profile-ui.fixtures.ts';
import { deferred, fixture, nodes, settle, target } from './telegram-profile-ui.fixtures.ts';

test('profile header uses the pinned100pt avatar,28pt title and58pt real action tiles; self has no fake calls', () => {
  const f = fixture('header');
  let tree = f.render({ name: 'Ana', subtitle: '@ana', actions: [] });
  assert.equal(nodes(tree).find((node) => node.type === 'Avatar')!.props.size, 100);
  assert.equal(target(tree, 'peer-info-title').props.style.fontSize, 28);
  assert.equal(
    nodes(tree).some((node) => node.props.testID === 'peer-info-actions'),
    false,
  );
  let called = 0;
  tree = f.render({
    name: 'Ana',
    actions: [
      {
        id: 'message',
        label: 'Message',
        icon: 'bubble',
        onPress: () => {
          called++;
        },
      },
    ],
  });
  const action = target(tree, 'message');
  assert.equal(action.props.style({ pressed: false }).height, 58);
  action.props.onPress();
  assert.equal(called, 1);
});
test('You retains Appearance, all settings and the actual Edit Profile entry after header port', () => {
  const f = fixture('you');
  const outer = f.render();
  // You wraps its content in SkyScene. Invoke that actual component, with its hooks isolated here.
  const content = nodes(outer).find((node) => typeof node.type === 'function');
  assert.ok(content);
  const tree = (content.type as Handler)(content.props);
  const settings = target(tree, 'you-settings-rows').props.rows;
  assert.deepEqual(
    Array.from(settings, (row: any) => row.id),
    [
      'settings-account-budget',
      'settings-appearance',
      'settings-general',
      'settings-permissions',
      'settings-notifications',
      'settings-devices',
      'settings-data-controls',
    ],
  );
  settings.find((row: any) => row.id === 'settings-appearance').onPress();
  assert.deepEqual(f.routes[0], ['push', '/settings/appearance']);
  const header = nodes(tree).find((node) => node.type === 'ProfileHeader')!;
  header.props.onAvatarPress();
  assert.deepEqual(f.routes[1], ['push', '/edit-profile']);
});
test('Edit Profile blocks invalid handle and coalesces synchronous Save taps', async () => {
  const f = fixture('edit');
  let tree = f.render();
  nodes(tree)
    .find((node) => node.type === 'Field' && node.props.label === 'profile.handle')!
    .props.onChangeText('bad handle');
  tree = f.render();
  let save = nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight();
  assert.equal(save.props.disabled, true);
  save.props.onPress();
  await settle();
  assert.equal(f.saves.length, 0);
  nodes(tree)
    .find((node) => node.type === 'Field' && node.props.label === 'profile.handle')!
    .props.onChangeText('new_handle');
  tree = f.render();
  save = nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight();
  save.props.onPress();
  save.props.onPress();
  await settle();
  assert.equal(f.saves.length, 1);
  assert.equal(f.requests.filter((request) => request.op === 'update_handle').length, 1);
  assert.deepEqual(f.routes, [['back']]);
});
test('pending photo picker after departure/account change releases its draft without publishing', async () => {
  for (const depart of ['unmount', 'epoch'] as const) {
    const f = fixture('edit');
    const picked = deferred<any>();
    let disposed = 0;
    f.setPicker(() => picked.promise);
    const tree = f.render();
    target(tree, 'edit-profile-photo').props.onPress();
    if (depart === 'unmount') f.unmount();
    else f.store.sessionEpoch++;
    picked.resolve({
      uri: 'local:pick',
      dispose() {
        disposed++;
      },
    });
    await settle();
    assert.equal(disposed, 1);
    assert.equal(f.saves.length, 0);
    assert.equal(f.routes.length, 0);
  }
});
test('pending native save retains exact photo URI through unmount and never navigates late', async () => {
  const f = fixture('edit');
  let disposed = 0;
  f.setPicker(async () => ({
    uri: 'local:owned',
    dispose() {
      disposed++;
    },
  }));
  target(f.render(), 'edit-profile-photo').props.onPress();
  await settle();
  const pending = deferred<void>();
  f.store.driver.photos.save = async (request) => {
    f.saves.push(request);
    await pending.promise;
  };
  const tree = f.render();
  const save = nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight();
  save.props.onPress();
  await settle();
  assert.equal(f.saves[0].uri, 'local:owned');
  f.unmount();
  assert.equal(disposed, 0);
  pending.resolve();
  await settle();
  assert.equal(disposed, 1);
  assert.equal(f.routes.length, 0);
});
test('failed native photo save preserves displayed draft for retry and explicit Remove disposes once', async () => {
  const f = fixture('edit');
  let disposed = 0;
  f.setPicker(async () => ({
    uri: 'local:owned',
    dispose() {
      disposed++;
    },
  }));
  target(f.render(), 'edit-profile-photo').props.onPress();
  await settle();
  f.store.driver.photos.save = async () => {
    throw new Error('denied');
  };
  const tree = f.render();
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  const failed = f.render();
  assert.equal(disposed, 0);
  assert.ok(
    nodes(failed).some(
      (node) => node.props.accessibilityRole === 'alert' && node.props.children === 'denied',
    ),
  );
  target(failed, 'edit-profile-remove-photo').props.onPress();
  f.unmount();
  assert.equal(disposed, 1);
});
test('session epoch/device change rejects stale Save before mutation, including between handle and photo', async () => {
  const f = fixture('edit');
  let tree = f.render();
  f.store.sessionEpoch++;
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  assert.equal(f.saves.length, 0);
  assert.equal(f.requests.length, 0);
  const next = fixture('edit');
  tree = next.render();
  nodes(tree)
    .find((node) => node.type === 'Field' && node.props.label === 'profile.handle')!
    .props.onChangeText('new_handle');
  next.store.request = async (command) => {
    next.requests.push(command);
    next.state.snapshot.account = { ...next.state.snapshot.account, device_id: 'retired' };
    return '';
  };
  tree = next.render();
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  assert.equal(next.requests.length, 1);
  assert.equal(next.saves.length, 0);
  assert.equal(next.routes.length, 0);
});
test('Person real start-direct is coalesced and late success cannot navigate a retired owner', async () => {
  const f = fixture('person');
  const pending = deferred<string>();
  f.store.request = async (command) => {
    f.requests.push(command);
    return pending.promise;
  };
  const tree = f.render();
  const action = nodes(tree).find((node) => node.type === 'ProfileHeader')!.props.actions[0];
  action.onPress();
  action.onPress();
  assert.equal(f.requests.length, 1);
  f.unmount();
  pending.resolve('space');
  await settle();
  assert.equal(f.routes.length, 0);
});
