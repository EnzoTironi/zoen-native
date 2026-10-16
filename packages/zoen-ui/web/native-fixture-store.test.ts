import test from 'node:test';
import assert from 'node:assert/strict';
import { nativeFixture, nativeFixtureConversations, nativeFixtureRootData } from './native-fixture-store.ts';
import { configureDisplayClock } from './display-clock.ts';
import { PARITY_FIXTURE } from './parity-config.ts';
test('native fixture adapter preserves actual core content, IDs, cards and timestamps without normalization', () => {
  const result = nativeFixtureConversations();
  assert.equal(result.length, nativeFixture.spaces.length);
  for (const [key, expected] of Object.entries(PARITY_FIXTURE)) assert.equal(nativeFixture[key as keyof typeof nativeFixture], expected);
  for (const [index, conversation] of result.entries()) {
    const source = nativeFixture.spaces[index]!;
    assert.equal(conversation.id, source.id);
    assert.equal(conversation.messages.length, source.entries.length);
    for (const [position, entry] of source.entries.entries()) {
      const message = conversation.messages[position]!;
      assert.equal(message.id, entry.id); assert.equal(message.author, entry.author.id);
      assert.equal(message.at_ms, entry.atMs); assert.equal(message.text, entry.text);
      assert.deepEqual(message.card ?? null, entry.card);
    }
  }
});

test('root screens receive actual request, ownership, task and account projections', (t) => {
  configureDisplayClock({ now: Date.parse(PARITY_FIXTURE.fixed_time), timeZone: 'UTC' });
  t.after(() => configureDisplayClock());
  const data = nativeFixtureRootData();
  assert.deepEqual(data.requests?.map(request => request.id), nativeFixture.requests.map(request => request.id));
  assert.equal(data.requests?.filter(request => request.status === 'pending').length, 3);
  assert.deepEqual(data.agents?.filter(agent => agent.persona.isMine).map(agent => agent.id),
    nativeFixture.agents.filter(agent => agent.persona.isMine).map(agent => agent.persona.id));
  assert.equal(data.agents?.filter(agent => agent.persona.isMine).length, 2);
  assert.equal(data.mentions?.length, 1);
  assert.equal(data.mentions?.[0]?.id, nativeFixture.mentions[0]?.entry.id);
  assert.equal(data.mentions?.[0]?.relativeTime, '18 min');
  assert.equal(data.items?.filter(item => item.kindId === 'task' || item.plan?.sections.some(section => section.lines.some(line => !line.done))).length, 2);
  assert.equal(data.stats?.items, 8);
  assert.equal(data.account, null);
  assert.deepEqual(data.grants, []);
  assert.deepEqual(data.connection, nativeFixture.connection);
});

test('fixture conversation mentions come from the actual Engine mention projection', () => {
  const conversations = nativeFixtureConversations();
  for (const conversation of conversations) {
    assert.equal(conversation.mentions, nativeFixture.mentions.filter(mention => mention.spaceId === conversation.id).length);
  }
  assert.equal(conversations.reduce((count, conversation) => count + conversation.mentions, 0), nativeFixture.mentions.length);
});
