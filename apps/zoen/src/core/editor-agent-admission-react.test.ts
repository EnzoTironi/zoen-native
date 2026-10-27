import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorAgentReactFixture } from './editor-agent-react.fixtures.ts';
import { t } from '../i18n/index.ts';

const press = async (f: Awaited<ReturnType<typeof editorAgentReactFixture>>, id: string) => {
  const control = f.controls.get(id);
  assert.ok(control, id);
  await f.act(() => control.onPress());
};
const openDraft = async (f: Awaited<ReturnType<typeof editorAgentReactFixture>>) => {
  await f.render();
  await press(f, 'editor-agent-open');
  await f.act(() => f.controls.get('editor-agent-prompt')!.onChangeText('Keep this exact request'));
};
const adds = (f: Awaited<ReturnType<typeof editorAgentReactFixture>>) =>
  f.calls.filter((c) => c.op === 'set_member_role_reviewed');
const sends = (f: Awaited<ReturnType<typeof editorAgentReactFixture>>) =>
  f.calls.filter((c) => c.op === 'request_app_edit');

test('one retained sheet: explicit owned inclusion, pending MLS, Back, disclosed current share and one Send', async () => {
  const f = await editorAgentReactFixture();
  try {
    f.members('missing');
    await openDraft(f);
    assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
    await press(f, 'editor-agent-add');
    const search = `label:${t('chat.searchPeople')}`;
    await f.act(() => f.controls.get(search)!.onChangeText('my'));
    // One explicit current refresh; the actual debounced search remains input-owned.
    await f.act(() => new Promise((resolve) => setTimeout(resolve, 270)));
    await press(f, `member-add-${f.agentId}`);
    assert.equal(adds(f).length, 0, 'a row does not silently add its agent');
    await press(f, 'member-add-cancel');
    assert.equal(f.controls.get(search)!.value, 'my');
    await press(f, `member-add-${f.agentId}`);
    await press(f, 'member-add-confirm');
    assert.equal(adds(f).length, 1);
    assert.equal(f.sheets(), 1);
    await f.act(() => f.members('waiting'));
    assert.equal(f.controls.has(`text:${t('community.connecting')}`), true);
    await press(f, `label:${t('common.back')}`);
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Keep this exact request');
    assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
    await f.act(() => f.members('ready'));
    assert.equal(f.controls.get('editor-agent-send')!.disabled, false);
    assert.equal(f.controls.has(`text:${t('miniapps.shareCurrentDisclosure')}`), true);
    assert.equal(sends(f).length, 0);
    await press(f, 'editor-agent-send');
    assert.equal(sends(f).length, 1);
    assert.equal(sends(f)[0]!.share_current, true);
    assert.equal(adds(f).length, 1);
    assert.deepEqual(f.counters(), { mounts: 1, unmounts: 0, flushes: 1, presented: true });
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const change of [undefined, 'device'] as const)
  test(`stale sheet dismissal cannot close a reopened owned picker (${change ?? 'same owner'})`, async () => {
    const f = await editorAgentReactFixture();
    try {
      f.members('missing');
      await openDraft(f);
      const staleDismiss = f.controls.get('sheet')!.onDismiss;
      await f.act(staleDismiss);
      if (change) await f.act(() => f.reopen(change));
      await press(f, 'editor-agent-open');
      assert.ok(
        f.controls.has('editor-agent-add'),
        JSON.stringify({
          controls: [...f.controls.keys()],
          counters: f.counters(),
          errors: f.errors.map(String),
          calls: f.calls.slice(-8),
        }),
      );
      await press(f, 'editor-agent-add');
      await f.act(staleDismiss);
      assert.equal(f.counters().presented, true);
      await press(f, `member-add-${f.agentId}`);
      await press(f, 'member-add-confirm');
      assert.equal(adds(f).length, 1);
      assert.equal(adds(f)[0]!.expected_device_id, f.snapshot.account!.device_id);
      assert.equal(sends(f).length, 0);
      assert.equal(f.sheets(), 1);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });

test('zero owned profiles reuse enrollment inside the same sheet and keep code/draft on Back', async () => {
  const f = await editorAgentReactFixture();
  try {
    f.members('none');
    await openDraft(f);
    await press(f, 'editor-agent-add');
    await press(f, 'agent-enrollment-paste');
    await f.act(() =>
      f.controls.get('agent-enrollment-code-input')!.onChangeText('explicit-pairing-code'),
    );
    await press(f, `label:${t('common.back')}`);
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Keep this exact request');
    await press(f, 'editor-agent-add');
    assert.equal(f.controls.get('agent-enrollment-code-input')!.value, 'explicit-pairing-code');
    await press(f, 'agent-enrollment-check');
    await press(f, 'agent-enrollment-confirm');
    assert.equal(f.calls.filter((c) => c.op === 'agent_enrollment_approve').length, 1);
    assert.equal(adds(f).length, 0, 'enrollment does not silently include its agent');
    await press(f, 'agent-enrollment-status');
    await press(f, 'agent-enrollment-profile');
    await press(f, `member-add-${f.agentId}`);
    await press(f, 'member-add-confirm');
    assert.equal(adds(f).length, 1);
    await press(f, `label:${t('common.back')}`);
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Keep this exact request');
    assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
    assert.equal(
      f.calls.filter((c) => c.op === 'find_people').length,
      0,
      'embedded enrollment uses scoped owned lookup',
    );
    assert.equal(adds(f).length, 1);
    assert.equal(sends(f).length, 0);
    assert.equal(f.sheets(), 1);
    assert.deepEqual(f.routes, []);
    assert.equal(f.counters().mounts, 1);
    assert.equal(f.counters().unmounts, 0);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

test('Direct with ready owned Agent can send; human Direct cannot silently gain an Agent or share elsewhere', async () => {
  const f = await editorAgentReactFixture();
  try {
    await f.act(() => f.direct());
    await openDraft(f);
    assert.equal(f.controls.get('editor-agent-send')!.disabled, false);
    await press(f, 'editor-agent-send');
    assert.equal(sends(f).length, 1);
    await f.act(() => f.members('missing'));
    await f.act(() =>
      f.controls.get('editor-agent-prompt')!.onChangeText('Preserve unsupported Direct draft'),
    );
    assert.equal(f.controls.get('editor-agent-send')!.disabled, true);
    assert.equal(f.controls.has('editor-agent-add'), false);
    assert.equal(f.controls.has(`text:${t('miniapps.agentUnavailable')}`), true);
    await press(f, 'editor-agent-send');
    assert.equal(sends(f).length, 1);
    assert.equal(adds(f).length, 0);
    assert.equal(f.controls.get('editor-agent-prompt')!.value, 'Preserve unsupported Direct draft');
    assert.deepEqual(f.routes, []);
    assert.deepEqual(f.errors, []);
  } finally {
    await f.close();
  }
});

for (const fault of ['role', 'proof'] as const)
  test(`inline inclusion cannot outlive ${fault} loss and never submits a document`, async () => {
    const f = await editorAgentReactFixture();
    try {
      f.members('missing');
      await openDraft(f);
      await press(f, 'editor-agent-add');
      await press(f, `member-add-${f.agentId}`);
      const staleConfirm = f.controls.get('member-add-confirm')!.onPress;
      await f.act(() => (fault === 'role' ? f.roleLost() : f.members('foreign')));
      await f.act(staleConfirm);
      assert.equal(adds(f).length, 0);
      assert.equal(sends(f).length, 0);
      assert.equal(f.counters().mounts, 1);
      assert.equal(f.counters().unmounts, 0);
      assert.deepEqual(f.errors, []);
    } finally {
      await f.close();
    }
  });
