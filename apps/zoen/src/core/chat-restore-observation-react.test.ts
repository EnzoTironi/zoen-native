import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chatSearchReactFixture } from './chat-search-react.fixtures.ts';

test('actual Expo chat observes normal→search→restored modes under the same current conversation owner', async () => {
  const f = await chatSearchReactFixture(true);
  try {
    await f.render();
    const packet = (sequence: string) => ({
      binding: f.current().navigationMetrics.binding,
      sequence,
      geometry: {
        contentOffset: { y: 120 },
        contentSize: { height: 900 },
        layoutMeasurement: { height: 600 },
        contentInset: { top: 80, bottom: 240 },
      },
    });
    assert.ok(packet('1').binding);
    await f.act(() => f.current().navigationMetrics.changed(packet('1')));
    assert.equal(
      f.restoreRecords.length,
      1,
      'normal owned conversation must be observed before search',
    );
    await f.act(() => f.current().search.open());
    await f.act(() => f.current().navigationMetrics.changed(packet('2')));
    await f.act(() => f.current().search.close());
    await f.act(() => f.current().navigationMetrics.changed(packet('3')));
    assert.deepEqual(
      f.restoreRecords.map((r) => JSON.parse(r.slice('[ZoenRestore] '.length)).phase),
      ['before-search', 'search', 'restored'],
    );
    const old = f.current().navigationMetrics.changed;
    f.snapshot.conversations[0]!.members = [];
    await f.act(() => f.store.refresh());
    await f.act(() => old(packet('4')));
    assert.equal(f.restoreRecords.length, 3, 'membership loss refuses late observations');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
