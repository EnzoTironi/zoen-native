import assert from 'node:assert/strict';
import { test } from 'node:test';
import { contactSaveReactFixture } from './contact-save-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';

test('optimized contact card opens the native create form with exactly the reviewed name and every phone', async () => {
  const f = await contactSaveReactFixture();
  try {
    await f.renderContact();
    assert.equal(f.reads.length, 0);
    assert.equal(f.saved.length, 0);
    await f.openContact();
    await f.pressSave();
    assert.deepEqual(f.saved, [
      {
        contact: {
          givenName: f.parsed.name,
          phones: f.parsed.phones.map((phone) => ({ label: phone.label, number: phone.value })),
        },
        options: { showsCancelButton: true, cancelButtonTitle: 'common.cancel' },
      },
    ]);
    assert.equal(
      f.reads.length,
      2,
      'native authorization repeats immediately before OS disclosure',
    );
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('duplicate taps coalesce until the OS form completes; cancellation leaves the original preview usable', async () => {
  const f = await contactSaveReactFixture();
  const form = deferred<boolean>();
  let calls = 0;
  f.save(async () => {
    calls++;
    return form.promise;
  });
  try {
    await f.renderContact();
    await f.openContact();
    const save = f.buttons.get('media.saveContact').onPress;
    await f.act(() => {
      save();
      save();
    });
    assert.equal(calls, 1);
    assert.equal(f.buttons.get('media.saveContact').disabled, true);
    await f.act(() => form.resolve(false));
    assert.equal(f.buttons.get('media.saveContact').disabled, false);
    assert.equal(f.text().includes('media.contactActionFailed'), false);
    assert.deepEqual(f.errors, []);
  } finally {
    form.resolve(false);
    await f.close();
  }
});

test('failed native form remains retryable and checks the authorized file again', async () => {
  const f = await contactSaveReactFixture();
  let calls = 0;
  f.save(async () => {
    if (++calls === 1) throw Error('system presentation failed');
    return true;
  });
  try {
    await f.renderContact();
    await f.openContact();
    await f.pressSave();
    assert.ok(f.text().includes('media.contactActionFailed'));
    assert.equal(f.buttons.get('media.saveContact').disabled, false);
    await f.pressSave();
    assert.equal(calls, 2);
    assert.equal(f.reads.length, 3);
    assert.equal(f.text().includes('media.contactActionFailed'), false);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const loss of ['background', 'blur'] as const)
  test(`native contact reread finishing after ${loss} cannot present an OS form`, async () => {
    const f = await contactSaveReactFixture();
    const reread = deferred<typeof f.parsed>();
    try {
      await f.renderContact();
      await f.openContact();
      f.read(() => reread.promise);
      await f.pressSave();
      await f.act(() => (loss === 'background' ? f.foreground('background') : f.focus(false)));
      await f.act(() => reread.resolve(f.parsed));
      assert.deepEqual(f.saved, []);
      assert.equal(f.released.length, 1);
      assert.equal(f.buttons.has('media.saveContact'), false);
      assert.deepEqual(f.errors, []);
    } finally {
      reread.resolve(f.parsed);
      await f.close();
    }
  });

test('a retained Save callback cannot act after account reopening, and current native refusal prevents disclosure', async () => {
  const f = await contactSaveReactFixture();
  try {
    await f.renderContact();
    await f.openContact();
    const save = f.buttons.get('media.saveContact').onPress;
    await f.reopen();
    await f.act(() => save());
    assert.deepEqual(f.saved, []);
    await f.openContact();
    f.read(async () => {
      throw Error('native file permission revoked');
    });
    await f.pressSave();
    assert.deepEqual(f.saved, []);
    assert.ok(f.text().includes('media.contactActionFailed'));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('replacing a parsed contact cannot send a different name or phone to the OS form', async () => {
  const f = await contactSaveReactFixture();
  try {
    await f.renderContact();
    await f.openContact();
    f.read(async () => ({ ...f.parsed, name: 'Substituted contact' }));
    await f.pressSave();
    assert.deepEqual(f.saved, []);
    assert.ok(f.text().includes('media.contactActionFailed'));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('optimized location card also remounts its file session after account reopening without reviving old Open', async () => {
  const f = await contactSaveReactFixture('location');
  try {
    await f.renderContact();
    const oldOpen = [...f.buttons.values()].find(
      (value) => value.accessibilityRole === 'button',
    ).onPress;
    await f.openContact();
    assert.ok(f.text().includes('Praça Nova'));
    await f.reopen();
    await f.act(() => oldOpen());
    assert.equal(f.reads.length, 1);
    assert.equal(f.text().includes('Praça Nova'), false);
    await f.openContact();
    assert.equal(f.reads.length, 2);
    assert.ok(f.text().includes('Praça Nova'));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});
