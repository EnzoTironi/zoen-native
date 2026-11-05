import assert from 'node:assert/strict';
import { test } from 'node:test';
import { chatSearchReactFixture } from './chat-search-react.fixtures.ts';

test('real compiled chat consumes a scoped ticket into its existing composer search without changing its message draft', async () => {
  const f = await chatSearchReactFixture();
  try {
    f.draft.messageDraft.text = 'unsent original';
    f.handoff('#private');
    await f.render();
    assert.equal(f.current().searchState.active, true);
    assert.equal(f.current().searchState.query, '#private');
    assert.equal(f.current().composerText, 'unsent original');
    await f.act(() => new Promise((resolve) => setTimeout(resolve, 240)));
    const request = f.calls.find((call) => call.op === 'search_messages');
    assert.equal(request?.query, '#private');
    assert.equal(f.draft.messageDraft.text, 'unsent original');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
