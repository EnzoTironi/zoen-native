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
      assert.equal(message.forwarded, entry.forwarded);
      assert.equal(message.thread, entry.inThread);
      assert.deepEqual(message.quote, entry.replyRef ? { id: entry.replyRef.id, name: entry.replyRef.author.name, text: entry.replyRef.text } : null);
      assert.deepEqual(message.reactions, entry.reactions.map(reaction => ({ emoji: reaction.emoji, mine: reaction.mine, count: reaction.people.length, participants: reaction.people.map(person => ({ identity: person.id, name: person.name })) })));
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

test('native timeline relations are exercised by actual reactions and thread replies', () => {
  const conversations = nativeFixtureConversations();
  assert.ok(conversations.flatMap(space => space.messages).some(message => message.reactions.length > 0));
  const threadReply = conversations.flatMap(space => space.messages).find(message => message.thread !== null);
  assert.ok(threadReply, 'the real seed includes a thread reply, which must not leak into the main timeline');
  const root = conversations.flatMap(space => space.messages).find(message => message.id === threadReply.thread);
  assert.ok(root);
  assert.ok(root.repliers.includes(threadReply.name));
  assert.equal(root.last_reply_ms, threadReply.at_ms);
});

test('fixture conversation mentions come from the actual Engine mention projection', () => {
  const conversations = nativeFixtureConversations();
  for (const conversation of conversations) {
    assert.equal(conversation.mentions, nativeFixture.mentions.filter(mention => mention.spaceId === conversation.id).length);
  }
  assert.equal(conversations.reduce((count, conversation) => count + conversation.mentions, 0), nativeFixture.mentions.length);
});
