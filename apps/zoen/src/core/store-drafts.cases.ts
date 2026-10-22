import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  deferred,
  draftSession,
  Driver,
  empty,
  erasureAccount,
  erasureScope,
} from './store.fixtures.ts';
import { ClientStore } from './store.ts';
import type { Command } from './types.ts';

test('native draft writes avoid snapshot polling and survive controller retirement with exact scoped commands', async () => {
  const driver = new Driver();
  driver.durableMessageDrafts = true;
  driver.opening = async () => draftSession();
  let durable = { revision: '0', text: '', reply_to: null as string | null };
  const commands: Command[] = [];
  driver.operation = async (command) => {
    commands.push(command);
    assert.equal(
      'expected_identity_id' in command && command.expected_identity_id,
      erasureAccount.identity_id,
    );
    assert.equal(
      'expected_device_id' in command && command.expected_device_id,
      erasureAccount.device_id,
    );
    assert.equal('thread_root_id' in command && command.thread_root_id, null);
    if (command.op === 'message_draft') return durable;
    assert.equal(command.op, 'save_message_draft');
    if (command.op !== 'save_message_draft') throw Error('unexpected command');
    assert.equal(command.expected_revision, durable.revision);
    durable = { revision: 'opaque-next', text: command.text, reply_to: command.reply_to };
    return durable;
  };
  const store = new ClientStore(driver);
  await store.open();
  const channel = store.messageDrafts.channel('read-chat', null)!;
  await channel.controller.activate(channel.scope);
  assert.equal(channel.controller.setText(channel.scope, '  Café 🌤️\r\nAinda não enviado  '), true);
  await store.close();
  assert.equal(commands.filter((command) => command.op === 'save_message_draft').length, 1);
  assert.equal(
    commands.some((command) => command.op === 'snapshot'),
    false,
  );
  assert.equal(store.messageDrafts.isCurrent(channel), false);
  assert.equal(channel.controller.setText(channel.scope, 'stale callback'), false);
  await store.open();
  const restored = store.messageDrafts.channel('read-chat', null)!;
  assert.notEqual(restored.controller, channel.controller);
  await restored.controller.activate(restored.scope);
  assert.equal(
    restored.controller.getSnapshot(restored.scope).text,
    '  Café 🌤️\r\nAinda não enviado  ',
  );
  assert.notEqual(restored.scope.owner.sessionEpoch, channel.scope.owner.sessionEpoch);
  await store.close();
});

test('device erasure cancels queued draft text and drains an admitted native save before wiping', async () => {
  const driver = new Driver();
  driver.durableMessageDrafts = true;
  let erased = false;
  driver.opening = async () => (erased ? empty : draftSession());
  const save = deferred<unknown>();
  const admitted = deferred<void>();
  const writes: string[] = [];
  driver.operation = async (command) => {
    if (command.op === 'message_draft') return { revision: '0', text: '', reply_to: null };
    assert.equal(command.op, 'save_message_draft');
    if (command.op !== 'save_message_draft') throw Error('unexpected command');
    assert.equal(erased, false);
    writes.push(command.text);
    admitted.resolve();
    return save.promise;
  };
  driver.eraseDevice = async () => {
    erased = true;
    writes.push('wipe');
  };
  const store = new ClientStore(driver);
  await store.open();
  const channel = store.messageDrafts.channel('read-chat', null)!;
  await channel.controller.activate(channel.scope);
  channel.controller.setText(channel.scope, 'admitted');
  await admitted.promise;
  channel.controller.setText(channel.scope, 'queued after admitted');
  const erasing = store.eraseDevice(erasureScope);
  assert.equal(store.messageDrafts.isCurrent(channel), false);
  assert.equal(channel.controller.setText(channel.scope, 'late'), false);
  assert.equal(erased, false);
  save.resolve({ revision: 'saved-before-wipe', text: 'admitted', reply_to: null });
  await erasing;
  assert.deepEqual(writes, ['admitted', 'wipe']);
  assert.equal(channel.controller.getSnapshot(channel.scope).text, '');
  assert.equal(store.getSnapshot().phase, 'onboarding');
  await store.close();
});
