import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { messageInfoNavigation } from './message-info-navigation.fixtures.ts';
import type { TimelineEntry } from './types';
test('actual menu navigation captures owner epoch and reauthorizes source before opening Info', async () => {
  const driver = new Driver();
  const snapshot = draftSession();
  driver.opening = async () => snapshot;
  const store = new ClientStore(driver);
  await store.open('account');
  const opened: unknown[] = [];
  const { messageInfoAction } = messageInfoNavigation({
    push: (route: unknown) => opened.push(route),
  });
  const entry = { id: 'message' } as TimelineEntry;
  const space = snapshot.conversations[0]!.id;
  const action = messageInfoAction(store, space, entry, (value) => value);
  action();
  assert.equal(opened.length, 1);
  assert.equal((opened[0] as { pathname: string }).pathname, '/message-info/[space]/[message]');
  await store.close();
  await store.open('account');
  action();
  assert.equal(opened.length, 1, 'same IDs in a new session do not borrow held action');
  messageInfoAction(store, space, entry, () => undefined)();
  assert.equal(opened.length, 1);
  await store.close();
});
