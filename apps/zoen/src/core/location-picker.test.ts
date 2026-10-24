import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, nodes } from './attachment-route-ui.fixtures.ts';
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test('actual Location route searches only on explicit Search, then selects the returned point without committing', async () => {
  const f = fixture('location');
  f.render();
  await f.tick();
  let tree = f.render();
  const search = nodes(tree).find(
    (node) => node.type === 'Field' && node.props.label === 'attachmentTabs.searchLocation',
  )!;
  search.props.onChangeText('A place');
  tree = f.render();
  assert.deepEqual(f.searches, []);
  f.button(tree, 'attachmentTabs.searchLocation').props.onPress();
  await f.tick();
  tree = f.render();
  f.button(tree, 'Chosen place').props.onPress();
  tree = f.render();
  const map = nodes(tree).find((node) => node.type === 'LocationMap')!;
  assert.deepEqual(map.props.point, { longitude: 4, latitude: 3, label: 'Chosen place' });
  assert.equal(f.prepared.length, 0);
  assert.equal(f.readMedia().content_json, 'null');
  f.button(tree, 'attachmentTabs.close').props.onPress();
  await f.tick();
  assert.equal(f.readMedia().content_json, 'null');
  assert.equal(f.read().text, '**old draft**');
  assert.equal(f.counts().sends, 0);
});

test('actual Location route refuses old map/Add callbacks and pending GPS across background/refocus', async () => {
  const f = fixture('location');
  f.render();
  await f.tick();
  let tree = f.render();
  const oldMap = nodes(tree).find((node) => node.type === 'LocationMap')!;
  const gate = deferred<{ longitude: number; latitude: number }>();
  f.gpsGate(gate.promise);
  f.button(tree, 'attachmentTabs.chooseLocation').props.onPress();
  await f.tick();
  f.background();
  f.foreground();
  gate.resolve({ longitude: 9, latitude: 8 });
  await f.tick();
  oldMap.props.onPointChange({ longitude: 11, latitude: 12 });
  tree = f.render();
  assert.equal(nodes(tree).find((node) => node.type === 'LocationMap')!.props.point, undefined);
  assert.equal(f.prepared.length, 0);
  assert.equal(f.readMedia().content_json, 'null');
  const map = nodes(tree).find((node) => node.type === 'LocationMap')!;
  map.props.onPointChange({ longitude: 2, latitude: 1 });
  tree = f.render();
  const add = f.button(tree, 'attachmentTabs.addToMessage');
  f.background();
  f.foreground();
  add.props.onPress();
  await f.tick();
  assert.equal(f.prepared.length, 0, 'retained Add cannot adopt the new activation');
});

test('actual Location route drops late search after a map choice and refuses owner/membership loss', async () => {
  for (const retire of ['replaceOwner', 'revoke', 'blur', 'replaceMedia'] as const) {
    const f = fixture('location');
    f.render();
    await f.tick();
    let tree = f.render();
    const gate = deferred<readonly { longitude: number; latitude: number }[]>();
    f.searchGate(gate.promise);
    nodes(tree)
      .find((node) => node.type === 'Field')!
      .props.onChangeText('Place');
    tree = f.render();
    f.button(tree, 'attachmentTabs.searchLocation').props.onPress();
    await f.tick();
    const map = nodes(tree).find((node) => node.type === 'LocationMap')!;
    map.props.onPointChange({ longitude: 5, latitude: 6 });
    tree = f.render();
    gate.resolve([{ longitude: 100, latitude: 80 }]);
    await f.tick();
    tree = f.render();
    assert.equal(nodes(tree).find((node) => node.type === 'LocationMap')!.props.point.longitude, 5);
    const add = f.button(tree, 'attachmentTabs.addToMessage');
    f[retire]();
    add.props.onPress();
    await f.tick();
    assert.equal(f.prepared.length, 0, retire);
    assert.equal(f.counts().sends, 0, retire);
  }
});
