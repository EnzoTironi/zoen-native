import assert from 'node:assert/strict';
import { test } from 'node:test';
import { SearchLaunches } from './search-launch.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';

test('scoped search handoff is bounded, private, released and retired by account/epoch changes', async () => {
  const launches = new SearchLaunches();
  const driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => structuredClone(snapshot);
  const store = new ClientStore(driver);
  await store.open('account');
  const space = snapshot.conversations[0]!.id;
  try {
    assert.equal(launches.prepare('ticket', space, '#private', store), true);
    assert.equal(launches.read('ticket', store)?.query, '#private');
    assert.equal(launches.prepare('ticket', space, '#replace', store), false);
    launches.release('ticket');
    assert.equal(launches.read('ticket', store), null);
    for (let i = 0; i < 5; i++)
      assert.equal(launches.prepare(`ticket-${i}`, space, `#private_${i}`, store), true);
    assert.equal(launches.read('ticket-0', store), null);
    assert.equal(launches.read('ticket-4', store)?.query, '#private_4');
    await store.close();
    await store.open('account');
    assert.equal(launches.read('ticket-4', store), null);
    assert.equal(launches.prepare('missing', 'foreign', '#private', store), false);
    assert.equal(launches.prepare('oversized', space, '#'.repeat(513), store), false);
  } finally {
    await store.close();
  }
});

test('membership loss followed by restoration cannot revive a captured search ticket', async () => {
  const launches = new SearchLaunches();
  const driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async () => structuredClone(snapshot);
  const store = new ClientStore(driver);
  await store.open('account');
  const chat = snapshot.conversations[0]!;
  try {
    assert.equal(launches.prepare('membership', chat.id, '#secret', store), true);
    const members = chat.members;
    chat.members = [];
    await store.refresh();
    chat.members = members;
    await store.refresh();
    assert.equal(launches.read('membership', store), null);
  } finally {
    await store.close();
  }
});
