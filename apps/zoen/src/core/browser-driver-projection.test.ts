import assert from 'node:assert/strict';
import test from 'node:test';
import { parseConversation } from '../../../../packages/zoen-ui/web/messages.ts';
import { browserSnapshot } from './browser-driver-projection.ts';
import { draftSession, erasureAccount } from './store.fixtures.ts';

test('browser snapshots retain canonical write admission for Readers, blocked writers and corrupt histories', () => {
  const person = draftSession().conversations[0]!.members[0]!;
  const conversations = [
    { id: 'writer', role: 'Member', can_send: true, integrity_error: null, expected: true },
    { id: 'reader', role: 'Reader', can_send: false, integrity_error: null, expected: false },
    { id: 'waiting-owner', role: 'Owner', can_send: false, integrity_error: null, expected: false },
    {
      id: 'invalid-projection',
      role: 'Owner',
      can_send: true,
      integrity_error: 'Invalid signed history',
      expected: false,
    },
  ];
  const snapshot = browserSnapshot({
    account: {
      identity: erasureAccount.identity_id,
      device: erasureAccount.device_id,
      name: person.name,
      handle: person.handle,
      relay: erasureAccount.relay_url,
    },
    registered: true,
    setupPending: false,
    ownName: person.name,
    connection: null,
    error: null,
    sourceConversations: conversations.map((conversation) =>
      parseConversation({
        ...conversation,
        title: conversation.id,
        kind: 'group',
        privacy: 'EndToEnd',
        roster: { [person.id]: conversation.role },
        profiles: [],
        prefs: { pinned: false, archived: false, muted_until_ms: null },
        event_count: '1',
        waiting: conversation.can_send ? null : 'Write admission unavailable',
        unreadable: 0,
        messages: [],
      }),
    ),
    persona: () => person,
    entry: () => {
      throw Error('No messages in this permission fixture');
    },
  });
  assert.deepEqual(
    snapshot.conversations.map((conversation) => [conversation.id, conversation.can_write]),
    conversations.map((conversation) => [conversation.id, conversation.expected]),
  );
  assert.deepEqual(snapshot.integrity_warnings, {
    'invalid-projection': 'Invalid signed history',
  });
});
