import assert from 'node:assert/strict';
import { test } from 'node:test';
import { appReopenFixture } from './app-reopen.fixtures.ts';

test('compiled same-card drawing opens, closes cleanly and reopens with a fresh official SDK session', async () => {
  const f = await appReopenFixture();
  try {
    await f.render();
    await f.pump();
    assert.equal(f.wires.length, 1);
    assert.equal(f.wires[0]!.error(), null);
    assert.equal(f.wires[0]!.loaded(), f.scene());
    const closing = f.controller().flush({ retire: true });
    await f.pump();
    await closing;
    await f.render(false);
    await f.render(true);
    await f.pump();
    assert.equal(f.wires.length, 2);
    assert.equal(f.wires[1]!.error(), null);
    assert.equal(f.wires[1]!.loaded(), f.scene());
    assert.equal(
      f.commands.filter((command) => command.op === 'app_document_save_pinned').length,
      0,
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('compiled close refuses a changed release and preserves the dirty frame and its draft', async () => {
  const f = await appReopenFixture();
  try {
    await f.render();
    await f.pump();
    const frame = f.wires[0]!,
      original = f.scene();
    frame.edit('{"elements":[{"id":"draft"}],"appState":{},"files":{}}');
    f.changePin();
    const closing = f
      .controller()
      .flush({ retire: true })
      .catch((error: unknown) => error);
    await f.pump();
    assert.match(String(await closing), /release changed|scene was refused/);
    assert.equal(f.wires.length, 1);
    assert.equal(frame.draft()?.includes('draft'), true);
    assert.equal(frame.loaded(), original);
    assert.equal(f.scene(), original);
    assert.equal(
      f.commands.filter((command) => command.op === 'app_document_save_pinned').length,
      1,
      'Core receives the old pin and refuses the checkpoint without changing the document',
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('closing during initialization retires the old frame before a fresh same-card opening', async () => {
  const f = await appReopenFixture();
  try {
    await f.render();
    assert.equal(f.wires[0]!.loaded(), null);
    await f.act(() => f.controller().flush({ retire: true }));
    await f.render(false);
    await f.render(true);
    await f.pump();
    assert.equal(f.wires.length, 2);
    assert.equal(f.wires[1]!.error(), null);
    assert.equal(f.wires[1]!.loaded(), f.scene());
    assert.equal(
      f.commands.filter((command) => command.op === 'app_document_save_pinned').length,
      0,
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('compiled close checkpoints one dirty scene with the displayed pin and CAS before reopening it', async () => {
  const f = await appReopenFixture();
  try {
    await f.render();
    await f.pump();
    const next = '{"elements":[{"id":"changed"}],"appState":{},"files":{}}';
    f.wires[0]!.edit(next);
    const closing = f.controller().flush({ retire: true });
    await f.pump();
    await closing;
    assert.equal(f.scene(), next);
    assert.equal(f.wires[0]!.draft(), null);
    assert.equal(
      f.commands.filter((command) => command.op === 'app_document_save_pinned').length,
      1,
    );
    await f.render(false);
    await f.render(true);
    await f.pump();
    assert.equal(f.wires[1]!.loaded(), next);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('an old owner controller and late guest packets cannot cross a new session lifetime', async () => {
  const f = await appReopenFixture();
  try {
    await f.render();
    await f.pump();
    const old = f.controller(),
      frame = f.wires[0]!;
    f.retireOwner();
    await assert.rejects(old.flush(), /accountChanged|closed/);
    frame.queue.push({
      jsonrpc: '2.0',
      id: 40,
      method: 'tools/call',
      params: { name: 'app_scene_begin', arguments: {} },
    });
    await f.pump();
    assert.equal(
      f.commands.filter((command) => command.op === 'app_document_save_pinned').length,
      0,
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
