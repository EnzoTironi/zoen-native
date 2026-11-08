import assert from 'node:assert/strict';
import { test } from 'node:test';
import { backupReactFixture } from './backup-react.fixtures.ts';
import { deferred } from './store.fixtures.ts';

test('optimized backup screen keeps saved-key consent, exact scoped commands and one operation per press', async () => {
  const f = await backupReactFixture();
  try {
    await f.renderBackup();
    const oldGenerate = f.controls.get('backup-generate').onPress;
    await f.act(() => {
      oldGenerate();
      oldGenerate();
    });
    assert.equal(f.requests.filter((value) => value.op === 'backup_create_recovery_key').length, 1);
    assert.ok(f.text().includes(f.key));
    await f.act(() => oldGenerate());
    assert.equal(
      f.requests.filter((value) => value.op === 'backup_create_recovery_key').length,
      1,
      'old generate must not replace an unsaved key',
    );
    await f.press('backup-copy');
    assert.deepEqual(f.copies, [f.key]);
    assert.equal(f.controls.get('backup-enable').disabled, true);
    await f.press('backup-key-saved');
    await f.press('backup-enable');
    const command = f.requests.find((value) => value.op === 'backup_enable_recovery_key');
    assert.deepEqual(command, {
      op: 'backup_enable_recovery_key',
      recovery_key: f.key,
      scope: {
        expected_identity_id: f.snapshot.account!.identity_id,
        expected_device_id: f.snapshot.account!.device_id,
        expected_relay_url: f.snapshot.account!.relay_url,
      },
    });
    assert.equal(f.text().includes(f.key), false);
    assert.ok(f.text().includes('access.backupEnabledSent'));
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const interruption of ['background', 'blur', 'account'] as const)
  test(`backup discards private form and ignores generation replies after ${interruption}`, async () => {
    const f = await backupReactFixture();
    const generated = deferred<string>();
    f.operation(async (command) =>
      command.op === 'backup_status'
        ? { enabled: false, mode: '', last_backup_ms: '0', bytes: '0' }
        : generated.promise,
    );
    try {
      await f.renderBackup();
      await f.press('backup-generate');
      if (interruption === 'background') await f.app('background');
      else if (interruption === 'blur') await f.focusBackup(false);
      else await f.reopen();
      await f.act(() => generated.resolve(f.key));
      assert.equal(f.text().includes(f.key), false);
      assert.equal(f.current().state.key, '');
      assert.deepEqual(f.copies, []);
      assert.deepEqual(f.errors, []);
    } finally {
      generated.resolve(f.key);
      await f.close();
    }
  });

test('old delete confirmation cannot delete a new session backup or clear a newer pending operation', async () => {
  const f = await backupReactFixture();
  const confirmation = deferred<boolean>(),
    upload = deferred<unknown>();
  f.status({ enabled: true });
  f.confirm(() => confirmation.promise);
  try {
    await f.renderBackup();
    await f.press('backup-disable');
    await f.app('background');
    await f.app('active');
    f.operation(async (command) =>
      command.op === 'backup_status'
        ? { enabled: true, mode: 'recovery_key', last_backup_ms: '0', bytes: '0' }
        : upload.promise,
    );
    await f.press('backup-now');
    await f.act(() => confirmation.resolve(true));
    assert.equal(
      f.requests.some((value) => value.op === 'backup_turn_off'),
      false,
    );
    assert.equal(f.current().state.pending, true);
    await f.act(() => upload.resolve(null));
    assert.equal(f.current().state.pending, false);
    assert.deepEqual(f.errors, []);
  } finally {
    confirmation.resolve(false);
    upload.resolve(null);
    await f.close();
  }
});

test('backup password and key controls retire on background and old clipboard errors stay silent', async () => {
  const f = await backupReactFixture();
  const copy = deferred<boolean>();
  f.copy(() => copy.promise);
  try {
    await f.renderBackup();
    await f.press('backup-method-password');
    await f.input('access.newBackupPassword', 'a private synthetic password');
    await f.input('access.repeatPassword', 'a private synthetic password');
    const submit = f.controls.get('backup-password-enable').onPress;
    await f.app('background');
    await f.app('active');
    assert.equal(f.current().state.password, '');
    assert.equal(f.current().state.repeat, '');
    await f.act(() => submit());
    assert.equal(
      f.requests.some((value) => value.op === 'backup_password'),
      false,
    );
    await f.press('backup-generate');
    await f.press('backup-copy');
    await f.app('background');
    await f.app('active');
    await f.act(() => copy.resolve(false));
    assert.equal(f.current().state.failure, null);
    assert.equal(f.text().includes(f.key), false);
    assert.deepEqual(f.errors, []);
  } finally {
    copy.resolve(false);
    await f.close();
  }
});

test('returning to an admitted native backup waits for its real status and cancels background polling', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = await backupReactFixture();
  try {
    f.status({ enabled: true, in_progress: true });
    await f.renderBackup();
    assert.equal(f.controls.get('backup-now').disabled, true);
    assert.ok(f.text().includes('access.backupInProgress'));
    const count = () => f.requests.filter((value) => value.op === 'backup_status').length;
    const initial = count();
    await f.app('background');
    await f.act(() => t.mock.timers.tick(2000));
    assert.equal(count(), initial, 'no status polling while retired');
    await f.app('active');
    assert.equal(count(), initial + 1);
    f.status({ in_progress: false, last_backup_ms: '2000', bytes: '100' });
    await f.act(() => t.mock.timers.tick(1000));
    assert.equal(count(), initial + 2);
    assert.equal(f.controls.get('backup-now').disabled, false);
    await f.act(() => t.mock.timers.tick(2000));
    assert.equal(count(), initial + 2, 'completed work stops polling');
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
    t.mock.timers.reset();
  }
});
