import assert from 'node:assert/strict';
import test from 'node:test';
import { loadChatPreferences, saveChatPreferences } from './chat-preferences.ts';

class Storage {
  data = new Map<string, string>();
  getItem(key: string) { return this.data.get(key) ?? null; }
  setItem(key: string, value: string) { this.data.set(key, value); }
}

test('chat preferences are isolated between accounts', () => {
  const storage = new Storage();
  saveChatPreferences(storage, 'alice', { chat: { archived: true, pinned: true } });
  assert.deepEqual(loadChatPreferences(storage, 'alice'), { chat: { archived: true, pinned: true } });
  assert.deepEqual(loadChatPreferences(storage, 'bob'), {});
});

test('unscoped legacy preferences are never attributed to another account', () => {
  const storage = new Storage();
  storage.setItem('zoen.chat-prefs', JSON.stringify({ chat: { muted: true } }));
  assert.deepEqual(loadChatPreferences(storage, 'alice'), {});
});

test('malformed and unexpected preferences fall back safely', () => {
  const storage = new Storage();
  const key = 'zoen.chat-prefs:alice';
  for (const value of ['null', '[]', '42', 'broken']) {
    storage.setItem(key, value);
    assert.deepEqual(loadChatPreferences(storage, 'alice'), {});
  }
  storage.setItem(key, JSON.stringify({ good: { pinned: true, muted: false, archived: 'yes' }, bad: null, other: [] }));
  assert.deepEqual(loadChatPreferences(storage, 'alice'), { good: { pinned: true, muted: false } });
});

test('blocked browser storage does not prevent chatting', () => {
  const storage = { getItem() { throw new Error('denied'); }, setItem() { throw new Error('full'); } };
  assert.deepEqual(loadChatPreferences(storage, 'alice'), {});
  assert.doesNotThrow(() => saveChatPreferences(storage, 'alice', { chat: { pinned: true } }));
});
