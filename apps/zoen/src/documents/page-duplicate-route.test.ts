import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fixture, tick, originalBlock } from './page-duplicate-route.fixtures.ts';
import { canDuplicateBlock, duplicateBlockChange } from './page-duplicate.ts';

// Real Page component + exact generated command shape; native receipt/storage is
// an explicit port. These regressions do not claim a new Core execution proof.
test('manual Duplicate emits one exact private draft edit and only accepted projection changes the Page', async () => {
  const f = fixture();
  let tree = f.render();
  assert.deepEqual(f.counts(), { ids: 0, effects: 0, refreshes: 0 });
  const source = f.read().blocks[1]!;
  let accept!: () => void;
  f.applyGate(
    new Promise<void>((resolve) => {
      accept = resolve;
    }),
  );
  const duplicate = f.blocks(tree).props.onDuplicate;
  duplicate(source);
  duplicate(source);
  await tick();
  assert.equal(f.counts().ids, 1);
  assert.equal(f.commands.length, 1);
  assert.equal(f.read().blocks.length, 3, 'no optimistic document is invented by the component');
  const command = f.commands[0]!;
  assert.equal(command.op, 'page_apply');
  if (command.op !== 'page_apply') throw Error('wrong contract');
  assert.equal(command.edit_context, 'observed-context');
  assert.equal(command.item_id, 'page');
  assert.deepEqual(command.order, ['before', 'original', command.mutation_id, 'after']);
  assert.deepEqual(command.changed, [{ ...source, id: command.mutation_id }]);
  accept();
  await tick();
  tree = f.render();
  assert.equal(f.read().blocks.length, 4);
  assert.equal(f.read().version, 3, 'duplication never shares a version');
  assert.equal(f.read().unsaved, true);
  assert.equal(f.counts().effects, 1);
  assert.ok(f.button(tree, 'documents.undoLastChange'));
  assert.equal(f.commands.filter((value) => value.op === 'page_commit').length, 0);
});

test('lost response retries the original mutation without another ID/copy, and Undo uses its accepted own context', async () => {
  const f = fixture();
  let tree = f.render();
  f.lostAck();
  f.blocks(tree).props.onDuplicate(f.read().blocks[1]);
  await tick();
  tree = f.render();
  const original = f.commands[0];
  assert.equal(f.counts().effects, 1);
  assert.equal(f.read().blocks.length, 4);
  assert.equal(f.read().version, 3);
  f.button(tree, 'documents.retryTheSameChange').props.onPress();
  await tick();
  tree = f.render();
  assert.deepEqual(f.commands[1], original);
  assert.equal(f.counts().ids, 1);
  assert.equal(f.counts().effects, 1);
  f.button(tree, 'documents.undoLastChange').props.onPress();
  await tick();
  const undo = f.commands[2]!;
  assert.equal(undo.op, 'page_apply');
  if (undo.op !== 'page_apply') throw Error('wrong undo');
  assert.equal(
    undo.edit_context,
    'accepted-own-context',
    'never revert against the merged/remote head',
  );
  assert.deepEqual(undo.order, ['before', 'original', 'after']);
  assert.deepEqual(undo.changed, []);
  assert.deepEqual(
    f.read().blocks.map((block) => block.id),
    ['before', 'original', 'after'],
  );
  assert.equal(f.commands.filter((value) => value.op === 'page_commit').length, 0);
});

test('owner/revocation/background/refocus/unmount while Core allocates ID cannot dispatch a stale copy', async () => {
  for (const retire of ['owner', 'revoke', 'background', 'blur', 'retire'] as const) {
    const f = fixture();
    const tree = f.render();
    let resolve!: (value: string) => void;
    f.idGate(
      new Promise<string>((done) => {
        resolve = done;
      }),
    );
    f.blocks(tree).props.onDuplicate(f.read().blocks[1]);
    f[retire]();
    if (retire === 'background') f.foreground();
    if (retire === 'blur') f.focus();
    resolve('client_edit_late');
    await tick();
    assert.equal(f.commands.length, 0, retire);
    assert.equal(f.counts().effects, 0, retire);
    assert.equal(f.read().blocks.length, 3);
    assert.equal(f.read().unsaved, false);
    if (retire === 'owner') {
      f.render();
      assert.notEqual(f.keys.at(-1), f.keys[0]);
    }
  }
});

test('late accepted reply stays recoverable after background and cannot silently create another mutation on return', async () => {
  const f = fixture();
  let tree = f.render();
  let resolve!: () => void;
  f.applyGate(
    new Promise<void>((done) => {
      resolve = done;
    }),
  );
  f.blocks(tree).props.onDuplicate(f.read().blocks[1]);
  await tick();
  f.background();
  f.foreground();
  resolve();
  await tick();
  tree = f.render();
  assert.equal(f.counts().effects, 1);
  assert.equal(f.commands.length, 1);
  f.applyGate(null);
  f.button(tree, 'documents.retryTheSameChange').props.onPress();
  await tick();
  assert.deepEqual(f.commands[1], f.commands[0]);
  assert.equal(f.counts().ids, 1);
  assert.equal(f.counts().effects, 1);
});

test('read-only and retained native input refuse duplicate; read-only current Markdown export stays available', async () => {
  for (const lock of ['readonly', 'retainedInput'] as const) {
    const f = fixture();
    f[lock]();
    const tree = f.render();
    assert.equal(f.blocks(tree).props.editable, false);
    f.blocks(tree).props.onDuplicate(f.read().blocks[1]);
    await tick();
    assert.equal(f.counts().ids, 0);
    assert.deepEqual(f.commands, []);
    if (lock === 'readonly') {
      const exportButton = f.button(tree, 'documents.exportMarkdown');
      assert.equal(exportButton.props.disabled, false);
      exportButton.props.onPress();
      await tick();
      assert.deepEqual(f.exports, ['actual read-port Markdown']);
    }
  }
});

test('duplicate intent detaches observed text/order/custom marks before allocation and refuses identity collisions', () => {
  const source = {
    ...originalBlock(),
    kind: 'task',
    checked: true,
    indent: 2,
    number: 7,
    lang: 'rust',
  };
  const order = ['before', source.id, 'after'];
  const before = structuredClone(source),
    build = duplicateBlockChange(order, source);
  order.reverse();
  source.text = 'changed during allocation';
  source.spans[0]!.value = 'changed mark';
  const result = build('client_edit_copy');
  assert.deepEqual(result.order, ['before', 'original', 'client_edit_copy', 'after']);
  assert.deepEqual(result.changed, [{ ...before, id: 'client_edit_copy' }]);
  for (const id of ['', 'original', 'not an id', 'a'.repeat(129)]) assert.throws(() => build(id));
  for (const kind of ['image', 'heading', 'future']) {
    const unsupported = { ...before, kind };
    assert.equal(canDuplicateBlock(unsupported), false);
    assert.throws(() => duplicateBlockChange(['original'], unsupported));
  }
});
