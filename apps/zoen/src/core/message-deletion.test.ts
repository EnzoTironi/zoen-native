import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  confirmMessageDeletion,
  messageDeletionTargets,
  type MessageDeletionChoice,
} from './message-deletion.ts';
import { plainMessage } from './message-caption.fixtures.ts';

const entry = (id: string) => {
  const source = plainMessage(id);
  return {
    ...source,
    id,
    text_revision: `revision-${id}`,
    author: { ...source.author, id: 'owner', is_me: true },
  };
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test('the entire selected set must be owned, current, unique and bounded', () => {
  const own = entry('a');
  assert.deepEqual(messageDeletionTargets([own], 'owner'), [
    { message_id: 'a', expected_revision: 'revision-a' },
  ]);
  for (const rows of [
    [],
    [own, own],
    [own, { ...entry('b'), deleted: true }],
    [own, { ...entry('b'), text_revision: undefined }],
    [own, { ...entry('b'), author: { ...own.author, id: 'other', is_me: false } }],
    Array.from({ length: 101 }, (_, i) => entry(String(i))),
  ])
    assert.equal(messageDeletionTargets(rows, 'owner'), null);
  assert.equal(messageDeletionTargets([own], 'different-account'), null);
});

test('confirmation holds exact revisions and sends nothing until explicitly accepted', async () => {
  const answer = deferred<MessageDeletionChoice | null>();
  const commands: unknown[] = [];
  let detached = false;
  const pending = confirmMessageDeletion({
    identity: 'owner',
    device: 'device',
    space: 'chat',
    canDeleteEveryone: () => true,
    resolve: () => [entry('a'), entry('b')],
    subscribe: () => () => {
      detached = true;
    },
    confirm: async (count) => {
      assert.equal(count, 2);
      return answer.promise;
    },
    send: async (command) => {
      commands.push(command);
    },
  });
  assert.equal(commands.length, 0);
  answer.resolve('everyone');
  assert.equal(await pending, true);
  assert.deepEqual(commands, [
    {
      expected_identity_id: 'owner',
      expected_device_id: 'device',
      space_id: 'chat',
      for_me: false,
      targets: [
        { message_id: 'a', expected_revision: 'revision-a' },
        { message_id: 'b', expected_revision: 'revision-b' },
      ],
    },
  ]);
  assert.ok(detached);
});

test('cancel, edited sources and a revoked then restored scope cannot authorize deletion', async () => {
  for (const change of ['cancel', 'edit', 'selection', 'revocation']) {
    const answer = deferred<MessageDeletionChoice | null>();
    let rows = [entry('a'), entry('b')];
    let changed: () => void = () => {};
    let sent = 0,
      detached = 0;
    const pending = confirmMessageDeletion({
      identity: 'owner',
      device: 'device',
      space: 'chat',
      canDeleteEveryone: () => true,
      resolve: () => rows,
      subscribe: (listener) => {
        changed = listener;
        return () => {
          detached++;
        };
      },
      confirm: () => answer.promise,
      send: async () => {
        sent++;
      },
    });
    if (change === 'edit') rows = [entry('a'), { ...entry('b'), text_revision: 'new-revision' }];
    if (change === 'selection') rows = [entry('a')];
    if (change === 'revocation') {
      rows = [];
      changed();
      rows = [entry('a'), entry('b')];
      changed();
    }
    answer.resolve(change === 'cancel' ? null : 'everyone');
    assert.equal(await pending, false, change);
    assert.equal(sent, 0, change);
    assert.equal(detached, 1);
  }
});

test('a failed command remains retryable and always detaches its observation', async () => {
  let detached = 0;
  const input = {
    identity: 'owner',
    device: 'device',
    space: 'chat',
    canDeleteEveryone: () => true,
    resolve: () => [entry('a')],
    subscribe: () => () => {
      detached++;
    },
    confirm: async () => 'everyone' as const,
    send: async () => {
      throw new Error('lost acknowledgement');
    },
  };
  await assert.rejects(confirmMessageDeletion(input), /lost acknowledgement/);
  assert.equal(detached, 1);
  assert.equal(await confirmMessageDeletion({ ...input, send: async () => ['a'] }), true);
  assert.equal(detached, 2);
});

test('mixed authors and readers can choose local deletion but cannot escalate to everyone', async () => {
  const rows = [
    entry('mine'),
    { ...entry('theirs'), author: { ...entry('theirs').author, id: 'peer', is_me: false } },
  ];
  for (const choice of ['for_me', 'everyone'] as const) {
    const sent: unknown[] = [];
    const accepted = await confirmMessageDeletion({
      identity: 'owner',
      device: 'device',
      space: 'chat',
      resolve: () => rows,
      canDeleteEveryone: () => false,
      subscribe: () => () => {},
      confirm: async (count, everyone) => {
        assert.equal(count, 2);
        assert.equal(everyone, false);
        return choice;
      },
      send: async (input) => {
        sent.push(input);
      },
    });
    assert.equal(accepted, choice === 'for_me');
    assert.equal(sent.length, choice === 'for_me' ? 1 : 0);
    if (sent.length) assert.equal((sent[0] as { for_me: boolean }).for_me, true);
  }
});
