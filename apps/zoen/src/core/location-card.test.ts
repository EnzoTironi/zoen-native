import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, tick, deferred } from './contact-attachment.fixtures.ts';
import { nodes } from './attachment-route-ui.fixtures.ts';
import {
  elements,
  fixture as fileFixture,
  tick as fileTick,
} from './inline-image-frame.fixtures.ts';

test('actual GeoJSON recipient routes to a location card without materializing bytes or filename inference', async () => {
  for (const [mime, capability, expected] of [
    ['application/geo+json', true, true],
    ['application/json', true, false],
    ['application/geo+json', false, false],
  ] as const) {
    const f = fileFixture(false, {
      info: { mime, name: 'location.geojson' },
      locationCapability: capability,
    });
    f.render();
    await fileTick();
    assert.equal(
      elements(f.render()).some((node) => node.type === 'LocationAttachment'),
      expected,
    );
    assert.deepEqual(f.materialized, []);
  }
});

test('actual location card reads only after Open and keeps the authorized copy until its read-only sheet closes', async () => {
  const f = fixture('location');
  let tree = f.render();
  assert.deepEqual(f.materialized, []);
  f.card(tree).props.onPress();
  f.card(tree).props.onPress();
  await tick();
  tree = f.render();
  assert.equal(f.materialized.length, 1);
  assert.equal(f.reads.length, 1);
  const map = nodes(tree).find((node) => node.type === 'LocationMap')!;
  assert.equal(map.props.editable, false);
  assert.equal(map.props.point.label, 'Praça Real');
  assert.deepEqual(f.released, []);
  f.button(tree, 'attachmentTabs.close').props.onPress();
  await tick();
  assert.equal(f.sheet(f.render()).props.visible, false);
  assert.equal(f.released.length, 1);
  assert.deepEqual(f.shares, []);
  assert.deepEqual(f.copies, []);
});

test('actual location preview drops late parsing on Close, source-context change and owner retirement', async () => {
  for (const retire of ['close', 'context', 'background', 'blur', 'owner'] as const) {
    const f = fixture('location');
    const gate = deferred<{ longitude: number; latitude: number; label: string }>();
    f.locationParse(gate.promise);
    f.card(f.render()).props.onPress();
    await tick();
    const tree = f.render();
    if (retire === 'close') f.button(tree, 'attachmentTabs.close').props.onPress();
    if (retire === 'context') {
      f.context('other-space', 'different-message');
      f.render();
    }
    if (retire === 'background') f.background();
    if (retire === 'blur') f.blur();
    if (retire === 'owner') {
      f.store.sessionEpoch++;
      f.notify();
    }
    gate.resolve({ longitude: 3, latitude: 4, label: 'Late point' });
    await tick();
    const next = f.render();
    assert.equal(
      nodes(next).some((node) => node.type === 'LocationMap'),
      false,
      retire,
    );
    assert.equal(f.released.length, 1, retire);
  }
});

test('native location authorization failure shows only a localized error and releases the leased file', async () => {
  const f = fixture('location');
  f.refuse();
  f.card(f.render()).props.onPress();
  await tick();
  const tree = f.render();
  assert.ok(f.text(tree).includes('attachmentTabs.locationOpenFailed'));
  assert.equal(
    nodes(tree).some((node) => node.type === 'LocationMap'),
    false,
  );
  assert.equal(f.released.length, 1);
});
