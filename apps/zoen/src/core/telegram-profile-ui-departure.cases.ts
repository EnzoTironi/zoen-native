import assert from 'node:assert/strict';
import { test } from 'node:test';
import { deferred, fixture, nodes, settle, target } from './telegram-profile-ui.fixtures.ts';

test('admitted profile save may settle after blur without popping a later route or disposing its owned photo early', async () => {
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
  nodes(tree)
    .find((node) => node.type === 'Screen')!
    .props.options.headerRight()
    .props.onPress();
  await settle();
  assert.equal(f.saves.length, 1);
  f.blur();
  assert.equal(disposed, 0);
  pending.resolve();
  await settle();
  assert.equal(f.routes.length, 0);
  assert.equal(disposed, 0);
  f.unmount();
  assert.equal(disposed, 1);
});
test('photo picker completion after route blur releases without replacing the retained profile draft', async () => {
  const f = fixture('edit');
  const selected = deferred<any>();
  let disposed = 0;
  f.setPicker(() => selected.promise);
  target(f.render(), 'edit-profile-photo').props.onPress();
  f.blur();
  selected.resolve({
    uri: 'local:late',
    dispose() {
      disposed++;
    },
  });
  await settle();
  assert.equal(disposed, 1);
  assert.equal(f.saves.length, 0);
  assert.equal(f.routes.length, 0);
});
