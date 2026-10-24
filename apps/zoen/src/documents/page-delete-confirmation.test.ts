import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, tick } from './page-duplicate-route.fixtures.ts';

function confirmation(f: ReturnType<typeof fixture>) {
  let respond!: (accepted: boolean) => void;
  f.confirmation(
    new Promise<boolean>((resolve) => {
      respond = resolve;
    }),
  );
  const before = f.confirmationCount();
  const tree = f.render();
  f.blocks(tree).props.onDelete(f.read().blocks[1]);
  assert.equal(f.confirmationCount(), before + 1);
  assert.equal(f.counts().ids, 0, 'a dialog is not a mutation');
  return respond;
}

test('actual Page delete confirmation cannot borrow a new activation after blur/refocus', async () => {
  const f = fixture();
  const respond = confirmation(f);
  f.blur();
  f.focus();
  f.render();
  respond(true);
  await tick();
  assert.equal(f.counts().ids, 0, 'stale confirmation must not allocate a new mutation ID');
  assert.deepEqual(f.commands, []);
  assert.deepEqual(
    f.read().blocks.map((block) => block.id),
    ['before', 'original', 'after'],
  );
  assert.equal(f.read().unsaved, false);
  // A new current user confirmation remains usable after the old one retires.
  const current = confirmation(f);
  current(true);
  await tick();
  assert.equal(f.counts().ids, 1);
  assert.equal(f.counts().effects, 1);
});

test('actual Page delete confirmation cannot borrow foreground activation after background', async () => {
  const f = fixture();
  const respond = confirmation(f);
  f.background();
  f.foreground();
  f.render();
  respond(true);
  await tick();
  assert.equal(f.counts().ids, 0, 'foreground does not reauthorize the old destructive dialog');
  assert.deepEqual(f.commands, []);
  assert.equal(f.counts().effects, 0);
});

test('owner, membership, edit permission and unmount changes retire an outstanding Page delete confirmation', async () => {
  for (const retire of ['owner', 'revoke', 'readonly', 'retire'] as const) {
    const f = fixture();
    const respond = confirmation(f);
    f[retire]();
    if (retire !== 'retire') f.render();
    respond(true);
    await tick();
    assert.equal(f.counts().ids, 0, retire);
    assert.deepEqual(f.commands, [], retire);
    assert.equal(f.counts().effects, 0, retire);
  }
});

test('current delete acceptance uses canonical draft edit and its accepted receipt remains undoable', async () => {
  const f = fixture();
  const original = structuredClone(f.read().blocks[1]);
  const respond = confirmation(f);
  respond(true);
  await tick();
  assert.equal(f.counts().ids, 1);
  assert.equal(f.counts().effects, 1);
  const deletion = f.commands[0]!;
  assert.equal(deletion.op, 'page_apply');
  if (deletion.op !== 'page_apply') throw Error('expected canonical draft edit');
  assert.equal(deletion.edit_context, 'observed-context');
  assert.deepEqual(deletion.order, ['before', 'after']);
  assert.deepEqual(deletion.changed, []);
  assert.equal(f.read().version, 3);
  f.button(f.render(), 'documents.undoLastChange').props.onPress();
  await tick();
  const undo = f.commands[1]!;
  assert.equal(undo.op, 'page_apply');
  if (undo.op !== 'page_apply') throw Error('expected canonical undo');
  assert.equal(undo.edit_context, 'accepted-own-context');
  assert.deepEqual(undo.order, ['before', 'original', 'after']);
  assert.deepEqual(undo.changed, [original]);
  assert.equal(f.read().version, 3);
  assert.equal(
    f.commands.some((command) => command.op === 'page_commit'),
    false,
  );
});

test('cancelling a Page delete dialog never allocates or submits a mutation', async () => {
  for (const resume of [false, true]) {
    const f = fixture();
    const respond = confirmation(f);
    if (resume) {
      f.background();
      f.foreground();
      f.render();
    }
    respond(false);
    await tick();
    assert.equal(f.counts().ids, 0);
    assert.deepEqual(f.commands, []);
    assert.equal(f.read().unsaved, false);
  }
});

test('delete accepted in the current activation still retires if ID allocation finishes after refocus', async () => {
  const f = fixture();
  let allocate!: (id: string) => void;
  f.idGate(
    new Promise<string>((resolve) => {
      allocate = resolve;
    }),
  );
  const respond = confirmation(f);
  respond(true);
  await tick();
  assert.equal(f.counts().ids, 1);
  f.blur();
  f.focus();
  f.render();
  allocate('client_edit_delete_late');
  await tick();
  assert.deepEqual(f.commands, []);
  assert.equal(f.counts().effects, 0);
});
