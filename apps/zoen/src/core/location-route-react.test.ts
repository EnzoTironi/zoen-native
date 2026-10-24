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
    assert.deepEqual(f.effectCounts(), { stages: 0, gps: 0 });
  } finally {
    await f.close();
  }
});

test('actual React also admits Location after a cold draft finishes loading', async () => {
  const f = locationReactFixture();
  try {
    await f.render();
    assert.deepEqual(f.errors, []);
    assert.ok(f.map?.editable);
    assert.equal(f.button('attachmentTabs.chooseLocation').disabled, false);
    assert.deepEqual(f.effectCounts(), { stages: 0, gps: 0 });
  } finally {
    await f.close();
  }
});

for (const transition of ['background', 'blur'] as const) {
  test(`actual React batched ${transition}/return replaces the binding and fences old map/Add callbacks`, async () => {
    const f = locationReactFixture();
    try {
      await f.warmDraft();
      await f.render();
      const original = f.map!;
      await f.act(() => original.onPointChange({ longitude: 5, latitude: 6 }));
      const add = f.button('attachmentTabs.addToMessage');
      assert.equal(add.disabled, false);
      await f.act(() => {
        if (transition === 'background') {
          f.appState('background');
          f.appState('active');
        } else {
          f.focus(false);
          f.focus(true);
        }
        // Invoked before React commits its replacement child: the route's
        // synchronous epoch fence must already reject the captured Add.
        add.onPress();
      });
      assert.ok(f.map?.editable);
      assert.notEqual(f.map.binding, original.binding);
      assert.equal(f.map.point, undefined);
      await f.act(() => original.onPointChange({ longitude: 99, latitude: 80 }));
      assert.equal(f.map.point, undefined);
      assert.ok(f.cancels.includes(original.binding));
      assert.deepEqual(f.errors, []);
      assert.deepEqual(f.effectCounts(), { stages: 0, gps: 0 });
    } finally {
      await f.close();
    }
  });
}

test('actual React drops late search results across a batched background activation', async () => {
  const f = locationReactFixture();
  try {
    await f.warmDraft();
    await f.render();
    let resolve!: (
      value: readonly { longitude: number; latitude: number; label: string }[],
    ) => void;
    const response = new Promise<readonly { longitude: number; latitude: number; label: string }[]>(
      (done) => {
        resolve = done;
      },
    );
    f.searchGate(response);
    await f.act(() => f.field('attachmentTabs.searchLocation').onChangeText('Synthetic'));
    await f.act(() => f.button('attachmentTabs.searchLocation').onPress());
    assert.deepEqual(f.searches, ['Synthetic']);
    await f.act(() => {
      f.appState('background');
      f.appState('active');
    });
    const current = f.map!.binding;
    await f.act(() => resolve([{ longitude: 1, latitude: 2, label: 'Late result' }]));
    assert.equal(f.hasButton('Late result'), false);
    assert.equal(f.map?.binding, current);
    assert.deepEqual(f.effectCounts(), { stages: 0, gps: 0 });
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const retire of ['revoke', 'replaceOwner'] as const) {
  test(`actual React retained Location controls refuse ${retire} before any rerender`, async () => {
    const f = locationReactFixture();
    try {
      await f.warmDraft();
      await f.render();
      const original = f.map!;
      await f.act(() => original.onPointChange({ longitude: 5, latitude: 6 }));
      const add = f.button('attachmentTabs.addToMessage');
      await f.act(() => {
        f[retire]();
        add.onPress();
        original.onPointChange({ longitude: 8, latitude: 9 });
      });
      assert.deepEqual(f.effectCounts(), { stages: 0, gps: 0 });
      await f.render();
      assert.equal(f.map, undefined);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
}
