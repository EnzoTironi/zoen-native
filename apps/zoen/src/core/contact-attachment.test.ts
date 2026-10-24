import assert from 'node:assert/strict';
import test from 'node:test';
import { fixture, tick, deferred } from './contact-attachment.fixtures.ts';
import type { ContactPreview } from '../media/types.ts';

test('actual contact card defers bytes, then shows native-parsed name/phone in the existing sheet; dismiss never sends', async () => {
  const f = fixture();
  let tree = f.render();
  assert.deepEqual(f.materialized, []);
  assert.equal(f.sheet(tree).props.visible, false);
  f.card(tree).props.onPress();
  await tick();
  tree = f.render();
  assert.equal(f.sheet(tree).props.visible, true);
  assert.ok(f.text(tree).includes('Ana Real Parser Port'));
  assert.ok(f.text(tree).includes('+55 31 1234 5678'));
  assert.equal(f.reads.length, 1);
  assert.deepEqual(f.shares, []);
  assert.deepEqual(f.copies, []);
  f.button(tree, 'media.copyContactPhone').props.onPress();
  await tick();
  assert.deepEqual(f.copies, ['+55 31 1234 5678']);
  f.button(f.render(), 'media.closeContact').props.onPress();
  await tick();
  assert.equal(f.sheet(f.render()).props.visible, false);
  assert.equal(f.released.length, 1);
});

test('actual close while native parsing is held drops the late response and releases one copy', async () => {
  const f = fixture(),
    gate = deferred<ContactPreview>();
  f.parse(gate.promise);
  f.card(f.render()).props.onPress();
  await tick();
  const loading = f.render();
  assert.equal(f.sheet(loading).props.visible, true);
  f.button(loading, 'media.closeContact').props.onPress();
  gate.resolve({ name: 'Late owner data', phones: [] });
  await tick();
  const tree = f.render();
  assert.equal(f.sheet(tree).props.visible, false);
  assert.equal(f.text(tree).includes('Late owner data'), false);
  assert.equal(f.released.length, 1);
});

test('foreign owner/device/epoch, membership loss and session retirement close the actual sheet and retire old actions', async () => {
  for (const loss of ['owner', 'device', 'epoch', 'member', 'retiring', 'driver'] as const) {
    const f = fixture();
    f.card(f.render()).props.onPress();
    await tick();
    const ready = f.render();
    const copy = f.button(ready, 'media.copyContactPhone').props.onPress;
    if (loss === 'owner') f.state.snapshot!.account!.identity_id = 'other';
    if (loss === 'device') f.state.snapshot!.account!.device_id = 'other';
    if (loss === 'epoch') f.store.sessionEpoch++;
    if (loss === 'member') f.state.snapshot!.conversations[0]!.members = [];
    if (loss === 'retiring') f.store.isSessionRetiring = true;
    if (loss === 'driver') f.store.driver.media = { ...f.store.driver.media };
    f.notify();
    copy();
    await tick();
    assert.deepEqual(f.copies, [], loss);
    assert.equal(f.sheet(f.render()).props.visible, false, loss);
    assert.equal(f.released.length, 1, loss);
  }
});

test('blur/refocus and background/foreground cannot revive captured Open/Copy callbacks', async () => {
  for (const lifecycle of ['focus', 'background'] as const) {
    const f = fixture();
    const oldOpen = f.card(f.render()).props.onPress;
    oldOpen();
    await tick();
    const oldCopy = f.button(f.render(), 'media.copyContactPhone').props.onPress;
    if (lifecycle === 'focus') {
      f.blur();
      f.focus();
    } else {
      f.background();
      f.foreground();
    }
    await tick();
    f.render();
    oldOpen();
    oldCopy();
    await tick();
    assert.equal(f.materialized.length, 1);
    assert.deepEqual(f.copies, []);
    f.card(f.render()).props.onPress();
    await tick();
    assert.equal(f.materialized.length, 2);
  }
});

test('deferred sharing readiness rechecks actual owner before any OS share is presented', async () => {
  const f = fixture();
  f.card(f.render()).props.onPress();
  await tick();
  const gate = deferred<boolean>();
  f.share(gate.promise);
  f.button(f.render(), 'media.shareContact').props.onPress();
  await tick();
  f.state.snapshot!.account!.identity_id = 'other';
  f.notify();
  gate.resolve(true);
  await tick();
  assert.deepEqual(f.shares, []);
  assert.equal(f.released.length, 1);
});

test('the explicit Share action uses exactly the authorized vCard copy and never dispatches twice', async () => {
  const f = fixture();
  f.card(f.render()).props.onPress();
  await tick();
  const gate = deferred<boolean>();
  f.share(gate.promise);
  const share = f.button(f.render(), 'media.shareContact').props.onPress;
  share();
  share();
  await tick();
  assert.deepEqual(f.shares, []);
  gate.resolve(true);
  await tick();
  assert.deepEqual(f.shares, [
    {
      uri: 'file:///synthetic/authorized.vcf',
      options: { mimeType: 'text/vcard', dialogTitle: 'Ana Real Parser Port' },
    },
  ]);
  assert.deepEqual(f.copies, []);
  assert.equal(f.released.length, 0, 'the sheet still owns the verified copy');
  f.sheet(f.render()).props.onDismiss();
  await tick();
  assert.equal(f.released.length, 1);
});

test('native parsing rejection is a localized contact error, never raw native paths or payload', async () => {
  const f = fixture();
  const failure = Promise.reject(new Error('NSError private/path.vcf private payload'));
  // The port is only awaited after explicit open; attach the native rejection then.
  failure.catch(() => {});
  f.parse(failure);
  f.card(f.render()).props.onPress();
  await tick();
  const tree = f.render();
  assert.ok(f.text(tree).includes('media.contactCouldNotBeOpened'));
  assert.equal(
    f.text(tree).some((text) => String(text).includes('private')),
    false,
  );
  assert.equal(f.released.length, 1);
  assert.deepEqual(f.copies, []);
  assert.deepEqual(f.shares, []);
});
