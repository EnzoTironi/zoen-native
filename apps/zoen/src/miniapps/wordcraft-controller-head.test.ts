import test from 'node:test';
import assert from 'node:assert/strict';
import { ownWriteFixture, initialWord, firstWord } from './wordcraft-own-write-refresh.fixtures.ts';
import { hash } from './canonical-scene.ts';
import { deferred, tick } from '../core/store.fixtures.ts';
import { createDeckcraftSession } from './deckcraft-session.ts';
import { binaryEditorFormat, type BinaryEditorId } from './binary-formats.ts';
import type { VerifiedMiniappResource } from './sandbox.ts';

const accepted = { itemId: 'it_deck', head: { version: '1', sha256: hash(initialWord) } };

test('controller reads the installed Word head over the real SDK, independently of a newer Core head', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  const observe = f.session.controller.observedHead;
  assert.ok(observe);
  f.stopNotifications();
  await f.peer();
  assert.equal(f.canonical().version, 2);
  f.events.length = 0;
  const observed = await observe();
  assert.deepEqual(observed, { itemId: 'it_deck', version: 1, sha256: hash(initialWord) });
  assert.equal(Object.isFrozen(observed), true);
  assert.deepEqual(f.events, ['status'], 'no Core read, freeze, save or reopen');
  assert.equal(f.states.length, 0, 'observation never changes frame blocking');
  assert.equal(f.writes(), 0);
});

test('dirty and pending installed Sessions refuse observation without pausing or covering input', async (t) => {
  for (const mode of ['dirty', 'pending']) {
    const f = await ownWriteFixture();
    t.after(f.close);
    const observe = f.session.controller.observedHead;
    assert.ok(observe);
    if (mode === 'dirty') f.port.type(firstWord);
    else
      await f.port.zoenHostRequest(
        JSON.stringify({
          op: 'snapshot',
          input: {
            mutationId: 'unacknowledged',
            base: f.port.status().head,
          },
        }),
        new Uint8Array(),
      );
    // Let the dirty notification already emitted by typing reach the host.
    await tick();
    const before = f.states.length;
    f.events.length = 0;
    await assert.rejects(observe());
    assert.deepEqual(f.events, ['status']);
    assert.equal(f.states.length, before);
    assert.equal(
      f.states.some((state) => state.blocked),
      false,
    );
    assert.equal(f.port.status().frozen, false);
    assert.equal(f.writes(), 0);
  }
});

test('controller rejects foreign or malformed guest heads and keeps the read-only queue usable', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  const observe = f.session.controller.observedHead;
  assert.ok(observe);
  for (const value of [
    { ...accepted, itemId: 'foreign' },
    { ...accepted, extra: true },
    ...['0', '01', '1.0', '4294967296', 1].map((version) => ({
      ...accepted,
      head: { ...accepted.head, version },
    })),
    { ...accepted, head: { version: '1', sha256: 'A'.repeat(64) } },
    { ...accepted, head: { ...accepted.head, extra: true } },
  ]) {
    f.app.oncalltool = async () => ({ content: [], structuredContent: value });
    await assert.rejects(observe());
  }
  f.app.oncalltool = async () => ({
    content: [],
    structuredContent: {
      itemId: 'it_deck',
      head: { version: '4294967295', sha256: hash(initialWord) },
    },
  });
  assert.equal((await observe()).version, 4294967295);
  assert.equal(f.states.length, 0);
  assert.equal(f.port.status().frozen, false);
  assert.equal(f.writes(), 0);
});

test('queued and in-flight head reads cannot return after their owner retires', async (t) => {
  const f = await ownWriteFixture();
  t.after(f.close);
  const observe = f.session.controller.observedHead;
  assert.ok(observe);
  const entered = deferred<void>(),
    response = deferred<void>();
  let calls = 0;
  f.app.oncalltool = async ({ name }) => {
    assert.equal(name, 'document_head');
    calls++;
    entered.resolve();
    await response.promise;
    return { content: [], structuredContent: accepted };
  };
  const first = observe(),
    second = observe();
  void first.catch(() => {});
  void second.catch(() => {});
  await entered.promise;
  await tick();
  assert.equal(calls, 1);
  assert.equal(f.states.length, 0);
  await f.close();
  response.resolve();
  await assert.rejects(first);
  await assert.rejects(second);
  assert.equal(calls, 1);
  await assert.rejects(observe());
});

test('observed head is absent outside capable editable formats and unopened editors never calls the guest', async () => {
  for (const editor of [
    'deckcraft',
    'gridcraft',
    'photocraft',
    'wordcraft',
    'pdfcraft',
  ] as BinaryEditorId[]) {
    for (const historical of [false, true]) {
      let calls = 0;
      const format = binaryEditorFormat(editor);
      const unavailable = async () => {
        calls++;
        throw Error('Unexpected Core call');
      };
      const session = createDeckcraftSession({
        editor,
        itemId: 'unopened',
        ...(historical ? { selectedHead: { version: '1', sha256: hash(initialWord) } } : {}),
        title: () => 'Unopened',
        verified: { resource: { uri: format.resourceUri, text: '' } } as VerifiedMiniappResource,
        driver: { request: unavailable },
        mutate: unavailable,
        check: () => {},
        id: () => 'unopened',
        context: () => ({}),
        send: () => {
          calls++;
        },
        disposeFrame: () => {},
        close: () => {},
        share: async () => {},
        onState: () => {},
      });
      if (format.liveReview && !historical) {
        assert.ok(session.controller.observedHead);
        await assert.rejects(session.controller.observedHead());
      } else assert.equal(session.controller.observedHead, undefined);
      assert.equal(calls, 0);
      session.retire();
    }
  }
});
