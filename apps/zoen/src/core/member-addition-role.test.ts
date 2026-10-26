import assert from 'node:assert/strict';
import { test } from 'node:test';
import { additionFixture, additionContext, peer } from './member-addition.fixtures.ts';
import { reader, deferred, settle } from './member-role.fixtures.ts';
import { memberAdditionReactFixture } from './member-addition-react.fixtures.ts';
import { fixture } from './community-members.fixtures.ts';

test('first Reader assignment uses its exact choice and has no intermediate Member canonical projection', async () => {
  const f = await additionFixture();
  await f.open();
  await f.select(f.session.ticket(), reader);
  assert.equal(f.writes.length, 1);
  assert.deepEqual(f.choices, [reader]);
  assert.equal(f.writes[0]?.state, 'Removed');
  assert.equal(f.writes[0]?.role, null);
  assert.equal(f.session.getSnapshot().selected?.state, 'Updating');
  assert.equal(f.session.getSnapshot().selected?.role, null);
  assert.equal(f.session.getSnapshot().selected?.pending_role, 'reader');
  f.value({
    ...additionContext(),
    role_options: [],
    pending_role: null,
    can_add: false,
    role: 'reader',
    state: 'Active',
    member: additionContext().candidate,
    delivery_ready: true,
  });
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().selected?.role, 'reader');
  assert.equal(f.choices.length, 1);
  f.session.retire();
});

test('first-role choice is rechecked after confirmation and cannot be swapped or rebound', async () => {
  for (const stale of [false, true]) {
    const f = await additionFixture();
    await f.open();
    let confirmations = 0;
    f.confirm(async () => {
      ++confirmations;
      f.value({ ...additionContext(), role_options: [{ ...reader, revision: 'e'.repeat(64) }] });
      return true;
    });
    await f.select(f.session.ticket(), stale ? { ...reader, role: 'admin' } : reader);
    assert.equal(f.writes.length, 0);
    assert.equal(confirmations, stale ? 0 : 1);
    f.session.retire();
  }
});

test('existing-member actions accept an addable Removed context only to close, never to offer role changes', async () => {
  const f = fixture();
  f.value(additionContext());
  await f.open();
  assert.equal(f.session.getSnapshot().selected, null);
  assert.equal(f.session.getSnapshot().phase, 'inactive');
  assert.equal(f.roleWrites.length, 0);
});

for (const inline of [false, true])
  for (const locale of ['en', 'pt-BR'] as const)
    test(`${locale} actual Compiler ${inline ? 'inline' : 'dialog'} inclusion selects Reader before review`, async () => {
      const f = await memberAdditionReactFixture(inline, locale);
      try {
        await f.render();
        await f.open();
        const menu = f.roleMenu(peer);
        assert.equal(menu.actions[0].id, reader.revision);
        assert.match(
          menu.actions[0].title,
          locale === 'en' ? /Add as Reader/ : /Adicionar como Leitor/,
        );
        await f.act(() => menu.onPressAction({ nativeEvent: { event: reader.revision } }));
        if (inline) {
          assert.match(
            f.text(),
            locale === 'en' ? /Add Selected as Reader/ : /Adicionar Selected como Leitor/,
          );
          assert.match(f.text(), /Community/);
          assert.equal(f.writes.length, 0);
          await f.press('member-add-confirm');
        } else {
          const review = f.confirmations[0] as { title: string; message: string };
          assert.match(
            review.title,
            locale === 'en' ? /Add Selected as Reader/ : /Adicionar Selected como Leitor/,
          );
          assert.match(review.message, /Community/);
        }
        assert.deepEqual(f.choices, [reader]);
        assert.equal(f.writes.length, 1);
        assert.equal(f.session.getSnapshot().selected?.role, null);
        assert.equal(f.session.getSnapshot().selected?.pending_role, 'reader');
        assert.deepEqual(f.errors, []);
      } finally {
        await f.act(() => f.session.retire());
        await f.close();
      }
    });

test('queued native Reader choices share one review and stale Reader callbacks cannot survive reopen', async () => {
  const f = await memberAdditionReactFixture(),
    answer = deferred<boolean>();
  try {
    await f.render();
    await f.open();
    f.confirm(() => answer.promise);
    const menu = f.roleMenu(peer);
    await f.act(() => {
      menu.onPressAction({ nativeEvent: { event: reader.revision } });
      menu.onPressAction({ nativeEvent: { event: reader.revision } });
    });
    assert.equal(f.confirmations.length, 1);
    assert.equal(f.writes.length, 0);
    await f.press('common.close');
    await f.open();
    await f.act(async () => {
      answer.resolve(true);
      await settle();
      menu.onPressAction({ nativeEvent: { event: reader.revision } });
    });
    assert.equal(f.writes.length, 0);
    assert.equal(f.confirmations.length, 1);
    assert.deepEqual(f.errors, []);
  } finally {
    answer.resolve(false);
    await f.act(() => f.session.retire());
    await f.close();
  }
});
