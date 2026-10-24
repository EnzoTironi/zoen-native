import assert from 'node:assert/strict';
import test from 'node:test';
import { locationReactFixture } from './location-route-react.fixtures.ts';

test('actual React first mount settles the Location map and controls after route focus', async () => {
  const f = locationReactFixture();
  try {
    await f.warmDraft();
    await f.render();
    assert.deepEqual(f.errors, []);
    assert.ok(
      f.map,
      'The first navigation mount must activate the map, without another focus/background cycle',
    );
    assert.equal(f.map.editable, true);
    assert.equal(f.field('attachmentTabs.searchLocation').editable, true);
    assert.equal(f.button('attachmentTabs.chooseLocation').disabled, false);
    assert.equal(f.button('attachmentTabs.addToMessage').disabled, true);
    assert.deepEqual(f.searches, [], 'Mounting does not search, request GPS or stage/send a point');
    assert.deepEqual(f.counts(), { backs: 0, sends: 0 });
  } finally {
    await f.close();
  }
});
