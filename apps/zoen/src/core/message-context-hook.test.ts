import assert from 'node:assert/strict';
import { test } from 'node:test';
import { liveMessageActions } from './message-actions-live.fixtures.ts';

type Fixture = Awaited<ReturnType<typeof liveMessageActions>>;
type Boundary = 'row' | 'blur' | 'owner' | 'access';
async function retire(f: Fixture, boundary: Boundary) {
  if (boundary === 'row') f.revokeRow();
  else await f.change(boundary);
}

test('the compiled hook waits for the existing keyboard clock before attaching native context', async () => {
  const f = await liveMessageActions();
  try {
    const hide = f.holdKeyboard();
    const request = f.context();
    const opening = await f.startContext(request);
    assert.deepEqual(f.keyboardCalls, ['keyboard']);
    assert.equal(f.panel().entry, null);
    await f.root.act(async () => {
      hide();
      await opening.done;
    });
    assert.equal(f.panel().nativeSource, request);
    assert.equal(f.keyboardCalls.length, 1);
    assert.deepEqual(f.commands, []);
  } finally {
    await f.close();
  }
});

for (const boundary of ['row', 'blur', 'owner', 'access'] as const) {
  test(`keyboard-held native context cannot cross ${boundary}`, async () => {
    const f = await liveMessageActions();
    try {
      const hide = f.holdKeyboard();
      const opening = await f.startContext(f.context());
      await retire(f, boundary);
      await f.root.act(async () => {
        hide();
        await opening.done;
      });
      assert.equal(f.panel().entry, null);
      assert.deepEqual(f.commands, []);
    } finally {
      await f.close();
    }
  });
}

test('double tap awaits exact native admission and toggles the latest canonical reaction', async () => {
  const f = await liveMessageActions();
  try {
    const gate = f.holdAdmission();
    const opening = await f.startContext(f.context('double'), 'reactions');
    assert.deepEqual(f.keyboardCalls, []);
    assert.deepEqual(f.admissions, [['row', 'gesture']]);
    await f.replaceReaction();
    await f.root.act(async () => {
      gate.resolve(true);
      await opening.done;
    });
    assert.deepEqual(structuredClone(f.commands), [
      { op: 'react', space_id: 'read-chat', message_id: f.entry.id, emoji: null },
    ]);
    assert.deepEqual(f.keyboardCalls, []);
  } finally {
    await f.close();
  }
});

for (const boundary of ['row', 'blur', 'owner', 'access'] as const) {
  test(`admission-held quick reaction cannot cross ${boundary}`, async () => {
    const f = await liveMessageActions();
    try {
      const gate = f.holdAdmission();
      const opening = await f.startContext(f.context('double'), 'reactions');
      await retire(f, boundary);
      await f.root.act(async () => {
        gate.resolve(true);
        await opening.done;
      });
      assert.deepEqual(f.commands, []);
      assert.deepEqual(f.keyboardCalls, []);
    } finally {
      await f.close();
    }
  });
}

test('native refusal performs no canonical mutation or keyboard movement', async () => {
  const f = await liveMessageActions();
  try {
    const gate = f.holdAdmission();
    const opening = await f.startContext(f.context('double'), 'reactions');
    await f.root.act(async () => {
      gate.resolve(false);
      await opening.done;
    });
    assert.deepEqual(f.commands, []);
    assert.deepEqual(f.keyboardCalls, []);
  } finally {
    await f.close();
  }
});
