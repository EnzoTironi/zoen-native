import assert from 'node:assert/strict';
import { test } from 'node:test';
import { all, fixture, review, text, type Review } from './forward-selection-react.fixtures.ts';
import type { ItemCard } from './generated/ItemCard';

const send = (tree: unknown) =>
  all(tree, (element) => element.props?.testID === 'forward-selection-send')[0]!;

test('real batch route destination tap only prepares; one Send passes the same complete frozen review', () => {
  const f = fixture();
  const initial = f.render();
  f.choose(initial);
  assert.deepEqual(
    f.calls.filter((call) => ['prepare', 'confirm'].includes(call.action)),
    [{ action: 'prepare', value: 'target' }],
  );
  const frozen = review();
  f.publish(frozen);
  const tree = f.render();
  assert.equal(all(tree, (element) => element.type === 'Preview').length, 2);
  assert.equal(send(tree).props.label, 'chat.send');
  (send(tree).props.onPress as () => void)();
  assert.equal(f.calls.at(-1)!.action, 'confirm');
  assert.equal(f.calls.at(-1)!.value, frozen);
});

test('real batch route displays accepted/pending rows and requires re-review before resuming partial progress', () => {
  const f = fixture();
  f.render();
  const partial = review(1, false);
  f.publish(partial);
  const tree = f.render();
  assert.equal(
    all(
      tree,
      (element) =>
        element.type === 'Text' && text(element.props.children) === 'chat.alreadyForwarded',
    ).length,
    1,
  );
  assert.equal(
    all(
      tree,
      (element) =>
        element.type === 'Text' &&
        text(element.props.children) === 'chat.forwardSelectionProgress:1/2',
    ).length,
    1,
  );
  assert.equal(send(tree).props.label, 'chat.forwardSelectionReview');
  (send(tree).props.onPress as () => void)();
  assert.deepEqual(f.calls.at(-1), { action: 'prepare', value: 'target' });
  assert.equal(f.calls.filter((call) => call.action === 'confirm').length, 0);
});

test('real accepted unfinished batch offers Close and never confirms an accepted set again', () => {
  const f = fixture();
  f.render();
  const accepted = review(2);
  f.publish(accepted);
  const tree = f.render();
  assert.equal(send(tree).props.label, 'common.close');
  (send(tree).props.onPress as () => void)();
  assert.equal(f.calls.at(-1)!.action, 'finish');
  assert.equal(f.calls.at(-1)!.value, accepted);
});

for (const reason of ['blur', 'access', 'epoch', 'retiring', 'replacement'] as const)
  test(`real batch route rejects retained preview and Send after ${reason}`, () => {
    const f = fixture();
    f.render();
    f.publish(review());
    const retained = f.render();
    if (reason === 'blur') f.blur();
    if (reason === 'access') f.revoke();
    if (reason === 'epoch') f.store.sessionEpoch++;
    if (reason === 'retiring') f.store.isSessionRetiring = true;
    if (reason === 'replacement') {
      f.publish(review());
      f.render();
    }
    const preview = all(retained, (element) => element.type === 'Preview')[0]!;
    assert.equal((preview.props.isCurrent as () => boolean)(), false);
    (send(retained).props.onPress as () => void)();
    assert.equal(f.calls.filter((call) => ['confirm', 'finish'].includes(call.action)).length, 0);
  });

test('real route uses durable Core batch status, review, confirm and finish ports', async () => {
  const f = fixture();
  f.render();
  const source = { source_space: 'source', message_ids: ['a', 'b'] };
  const scope = { ...source, target_space: 'target' };
  await f.ports().status!(source);
  await f.ports().review!(scope);
  await f.ports().confirm!(source, 'batch-id', 'review-revision');
  await f.ports().abandon!(source, 'batch-id', 'review-revision');
  await f.ports().finish!(source, 'batch-id');
  assert.deepEqual(
    JSON.parse(
      JSON.stringify(f.calls.filter((call) => call.action === 'driver' || call.action === 'store')),
    ),
    [
      { action: 'driver', value: { op: 'forward_batch_status', source } },
      { action: 'driver', value: { op: 'forward_batch_review', scope } },
      {
        action: 'store',
        value: {
          op: 'confirm_forward_batch',
          source,
          batch_id: 'batch-id',
          review_revision: 'review-revision',
        },
      },
      {
        action: 'driver',
        value: {
          op: 'abandon_forward_batch',
          source,
          batch_id: 'batch-id',
          review_revision: 'review-revision',
        },
      },
      { action: 'driver', value: { op: 'finish_forward_batch', source, batch_id: 'batch-id' } },
    ],
  );
});

test('batch options update the exact reviewed set through the private driver without sending', async () => {
  const f = fixture();
  f.render();
  const partial = review(1, false);
  partial.messages[0]!.message.author.name = 'Accepted sender';
  partial.messages[1]!.message.author.name = 'Pending sender';
  partial.messages[1]!.options = { hide_author: true, hide_caption: true };
  partial.messages[1]!.message.kind = {
    Message: { text: 'Only this caption', card: { kind_id: 'file' } as ItemCard },
  };
  f.publish(partial);
  const tree = f.render();
  const menu = all(tree, (element) => element.type === 'Options')[0]!;
  assert.equal(menu.props.remaining, true);
  assert.equal(menu.props.canHideCaption, true);
  assert.equal(menu.props.disabled, false);
  assert.strictEqual(menu.props.options, partial.messages[1]!.options);
  const labels = all(tree, (element) => element.type === 'Text').map((n) => text(n.props.children));
  assert.ok(labels.includes('Accepted sender'));
  assert.ok(!labels.includes('Pending sender'));
  const options = { hide_author: true, hide_caption: false };
  (menu.props.onChange as (options: unknown) => void)(options);
  assert.deepEqual(f.calls.at(-1), { action: 'configure', value: { review: partial, options } });
  assert.equal(f.calls.filter((c) => c.action === 'confirm').length, 0);
  const previews = all(tree, (e) => e.type === 'Preview');
  assert.strictEqual(previews[0]!.props.options, partial.messages[0]!.options);
  assert.strictEqual(previews[1]!.props.options, partial.messages[1]!.options);
  const source = { source_space: 'source', message_ids: ['a', 'b'] };
  await f.ports().configure!(source, 'batch', 'revision', options);
  assert.deepEqual(JSON.parse(JSON.stringify(f.calls.at(-1))), {
    action: 'driver',
    value: {
      op: 'set_forward_batch_options',
      source,
      batch_id: 'batch',
      review_revision: 'revision',
      options,
    },
  });
  f.publish(review(2));
  assert.equal(all(f.render(), (e) => e.type === 'Options').length, 0);
});

for (const reason of ['blur', 'epoch', 'access', 'retiring', 'replacement'] as const)
  test(`batch option menu cannot operate a retained ${reason} review`, () => {
    const f = fixture();
    f.render();
    f.publish(review());
    const tree = f.render();
    const menu = all(tree, (element) => element.type === 'Options')[0]!;
    if (reason === 'blur') f.blur();
    if (reason === 'epoch') f.store.sessionEpoch++;
    if (reason === 'access') f.revoke();
    if (reason === 'retiring') f.store.isSessionRetiring = true;
    if (reason === 'replacement') {
      f.publish(review());
      f.render();
    }
    (menu.props.onChange as (options: unknown) => void)({ hide_author: true, hide_caption: true });
    assert.equal(f.calls.filter((c) => c.action === 'configure').length, 0);
  });

test('batch review renders original forwarding attribution and keeps legacy sources generic', () => {
  const f = fixture();
  f.render();
  const value = review();
  for (const row of value.messages) {
    row.message.forwarded = true;
    row.message.author.name = 'Intermediate sender';
  }
  value.messages[0]!.message.forwarded_author = 'Original author';
  f.publish(value);
  const labels = all(f.render(), (e) => e.type === 'Text').map((n) => text(n.props.children));
  assert.ok(labels.includes('Forwarded from Original author'));
  assert.ok(labels.includes('Forwarded'));
  assert.ok(!labels.some((label) => label.includes('Intermediate sender')));
});

test('completed durable batch requires Forward Again and another destination choice before a new review', () => {
  const f = fixture();
  f.render();
  f.publish(review(2, true, true));
  const completed = f.render();
  assert.equal(send(completed).props.label, 'chat.forwardAgain');
  (send(completed).props.onPress as () => void)();
  const picker = f.render();
  assert.equal(all(picker, (element) => element.type === 'Preview').length, 0);
  assert.equal(all(picker, (element) => element.type === 'FlashList').length, 1);
  assert.equal(
    f.calls.filter((call) => ['prepare', 'confirm', 'finish'].includes(call.action)).length,
    0,
  );
  assert.deepEqual(
    f.calls.filter((call) => call.action === 'clear'),
    [{ action: 'clear' }],
  );
  f.choose(picker);
  assert.deepEqual(
    f.calls.filter((call) => ['clear', 'prepare'].includes(call.action)),
    [{ action: 'clear' }, { action: 'prepare', value: 'target' }],
  );
});

test('Cancel leaves interrupted batch with Core and only returns to the conversation', () => {
  const f = fixture();
  f.render();
  f.publish(review(1, false));
  const tree = f.render();
  const cancel = all(
    tree,
    (element) => element.type === 'Button' && element.props.label === 'chat.cancel',
  )[0]!;
  (cancel.props.onPress as () => void)();
  assert.deepEqual(f.calls.at(-1), { action: 'back' });
  assert.equal(
    f.calls.filter((call) => ['cancel', 'confirm', 'finish'].includes(call.action)).length,
    0,
  );
});

test('pending retarget preserves accepted destination and requires the exact newly reviewed set for Send', () => {
  const f = fixture();
  f.render();
  f.publish(review(1, false));
  const previous = f.render();
  const changeRecipient = all(
    previous,
    (element) =>
      element.type === 'Pressable' &&
      element.props.accessibilityLabel === 'chat.forwardTo: Recipient',
  )[0]!;
  assert.equal(changeRecipient.props.disabled, false);
  (changeRecipient.props.onPress as () => void)();
  const picker = f.render();
  (send(previous).props.onPress as () => void)();
  assert.equal(f.calls.filter((call) => ['confirm', 'prepare'].includes(call.action)).length, 0);
  assert.deepEqual(
    f.calls.filter((call) => call.action === 'clear'),
    [{ action: 'clear' }],
  );
  f.choose(picker, 'other');
  assert.deepEqual(
    f.calls.filter((call) => ['clear', 'prepare'].includes(call.action)),
    [{ action: 'clear' }, { action: 'prepare', value: 'other' }],
  );
  const retargeted: Review = { ...review(1), targetId: 'other' };
  retargeted.messages[1]!.targetId = 'other';
  f.publish(retargeted);
  const current = f.render();
  assert.equal(
    all(
      current,
      (element) =>
        element.type === 'Text' &&
        element.props.accessibilityRole === 'header' &&
        text(element.props.children) === 'Other Recipient',
    ).length,
    1,
  );
  assert.equal(
    all(
      current,
      (element) =>
        element.type === 'Text' &&
        text(element.props.children) === 'chat.alreadyForwarded · Recipient',
    ).length,
    1,
  );
  (send(current).props.onPress as () => void)();
  assert.equal(f.calls.at(-1)!.action, 'confirm');
  assert.equal(f.calls.at(-1)!.value, retargeted);
});

test('recipient retarget control is disabled while a batch operation is busy', () => {
  const f = fixture();
  f.render();
  f.publish(review(1, false));
  f.busy();
  const tree = f.render();
  const changeRecipient = all(
    tree,
    (element) =>
      element.type === 'Pressable' &&
      element.props.accessibilityLabel === 'chat.forwardTo: Recipient',
  )[0]!;
  assert.equal(changeRecipient.props.disabled, true);
});

test('batch owner port revokes presentation authority while the session is retiring', () => {
  const f = fixture();
  f.render();
  assert.ok(f.ports().owner!());
  f.store.isSessionRetiring = true;
  assert.equal(f.ports().owner!(), null);
});

test('explicit Stop remaining forwarding passes the exact frozen interrupted review and cannot use a replaced preview', () => {
  const f = fixture();
  f.render();
  const interrupted = review(1, false);
  f.publish(interrupted);
  const tree = f.render();
  const stop = all(tree, (element) => element.props.testID === 'forward-selection-stop')[0]!;
  assert.equal(stop.props.disabled, false);
  (stop.props.onPress as () => void)();
  assert.equal(f.calls.at(-1)!.action, 'abandon');
  assert.equal(f.calls.at(-1)!.value, interrupted);
  f.publish(review(1, false));
  f.render();
  (stop.props.onPress as () => void)();
  assert.equal(f.calls.filter((call) => call.action === 'abandon').length, 1);
});

test('Stop remaining is disabled while busy and absent for ready, terminal or completed sets', () => {
  const f = fixture();
  f.render();
  f.publish(review(1, false));
  f.busy();
  assert.equal(
    all(f.render(), (element) => element.props.testID === 'forward-selection-stop')[0]!.props
      .disabled,
    true,
  );
  for (const terminal of [review(1, true), review(2, false), review(1, false, true)]) {
    const g = fixture();
    g.render();
    g.publish(terminal);
    assert.equal(
      all(g.render(), (element) => element.props.testID === 'forward-selection-stop').length,
      0,
    );
  }
});

test('skipped source is Not forwarded and has a separate count from accepted delivery', () => {
  const f = fixture();
  f.render();
  const stopped = { ...review(1, false), skippedIds: ['b'] };
  f.publish(stopped);
  const tree = f.render();
  assert.equal(
    all(
      tree,
      (element) =>
        element.type === 'Text' && text(element.props.children) === 'chat.alreadyForwarded',
    ).length,
    1,
  );
  assert.equal(
    all(
      tree,
      (element) => element.type === 'Text' && text(element.props.children) === 'chat.notForwarded',
    ).length,
    1,
  );
  assert.equal(
    all(
      tree,
      (element) =>
        element.type === 'Text' &&
        text(element.props.children) ===
          'chat.forwardSelectionProgress:1/2 · chat.forwardSelectionSkipped:1',
    ).length,
    1,
  );
  assert.equal(send(tree).props.label, 'common.close');
  (send(tree).props.onPress as () => void)();
  assert.equal(f.calls.at(-1)!.action, 'finish');
  assert.equal(f.calls.at(-1)!.value, stopped);
  assert.equal(f.calls.filter((call) => call.action === 'confirm').length, 0);
});

test('completed skipped summary offers Close and Forward Again without repeating Send', () => {
  const f = fixture();
  f.render();
  f.publish({ ...review(1, false, true), skippedIds: ['b'] });
  const tree = f.render();
  assert.equal(
    all(tree, (element) => element.type === 'Button' && element.props.label === 'common.close')
      .length,
    1,
  );
  assert.equal(send(tree).props.label, 'chat.forwardAgain');
  (send(tree).props.onPress as () => void)();
  const picker = f.render();
  assert.equal(all(picker, (element) => element.type === 'FlashList').length, 1);
  assert.equal(
    f.calls.filter((call) => ['confirm', 'abandon', 'finish', 'prepare'].includes(call.action))
      .length,
    0,
  );
  assert.deepEqual(
    f.calls.filter((call) => call.action === 'clear'),
    [{ action: 'clear' }],
  );
  f.choose(picker);
  assert.deepEqual(
    f.calls.filter((call) => ['clear', 'prepare'].includes(call.action)),
    [{ action: 'clear' }, { action: 'prepare', value: 'target' }],
  );
});
