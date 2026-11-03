import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';
import { test } from 'node:test';
import { build } from 'esbuild';
import * as scopes from './conversation-scope.ts';
import * as links from '../notifications/links.ts';
import { ClientStore } from './store.ts';
import { deferred, Driver, draftSession, tick } from './store.fixtures.ts';
import type { TimelineEntry } from './types';
import type { copyMessageLink } from '../components/message-link-actions';

const compiled = await build({
  stdin: {
    contents: readFileSync(
      new URL('../components/message-link-actions.ts', import.meta.url),
      'utf8',
    ),
    loader: 'ts',
  },
  external: ['*'],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
});
async function fixture() {
  const writes: string[] = [],
    alerts: string[] = [];
  let write = async (_value: string) => true;
  const module = { exports: {} as { copyMessageLink: typeof copyMessageLink } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name === 'expo-clipboard')
        return {
          setStringAsync: (text: string) => {
            writes.push(text);
            return write(text);
          },
        };
      if (name === '../core/conversation-scope') return scopes;
      if (name === '../notifications/links') return links;
      if (name === '../i18n/index.ts') return { t: (key: string) => key };
      throw new Error(name);
    },
  });
  const driver = new Driver(),
    snapshot = draftSession();
  driver.opening = async () => snapshot;
  const store = new ClientStore(driver);
  await store.open('account');
  const entry = {
    id: 'message',
    deleted: false,
    kind: { Message: { text: 'private text' } },
  } as TimelineEntry;
  const epoch = store.sessionEpoch,
    space = snapshot.conversations[0]!.id;
  let current = true;
  return {
    store,
    writes,
    alerts,
    entry,
    write: (next: typeof write) => {
      write = next;
    },
    retire: () => {
      current = false;
    },
    copy: () =>
      module.exports.copyMessageLink(
        store,
        space,
        entry,
        () => (current ? entry : undefined),
        epoch,
        async (title) => {
          alerts.push(title);
        },
      ),
  };
}

test('copy message link writes IDs only and reports rejected, thrown or refused clipboard writes', async () => {
  const f = await fixture();
  try {
    await f.copy();
    assert.deepEqual(f.writes, ['zoen://message/read-chat/message']);
    assert.deepEqual(f.alerts, []);
    f.write(async () => false);
    await f.copy();
    f.write(async () => {
      throw new Error('clipboard unavailable');
    });
    await f.copy();
    f.write(() => {
      throw new Error('native synchronous failure');
    });
    await f.copy();
    assert.deepEqual(f.alerts, [
      'chat.copyLinkFailed',
      'chat.copyLinkFailed',
      'chat.copyLinkFailed',
    ]);
  } finally {
    await f.store.close();
  }
});

test('held copy actions cannot borrow a new same-account session or a deleted message', async () => {
  const f = await fixture();
  try {
    f.entry.deleted = true;
    f.copy();
    f.entry.deleted = false;
    await f.store.close();
    await f.store.open('account');
    f.copy();
    assert.deepEqual(f.writes, []);
  } finally {
    await f.store.close();
  }
});

test('late clipboard errors do not reopen a retired message action', async () => {
  const f = await fixture();
  try {
    const result = deferred<boolean>();
    f.write(() => result.promise);
    f.copy();
    f.retire();
    result.reject(new Error('failed'));
    await tick();
    assert.deepEqual(f.alerts, []);
  } finally {
    await f.store.close();
  }
});
