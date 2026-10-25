import assert from 'node:assert/strict';
import { test } from 'node:test';
import { editorAgentAdmission } from '../miniapps/editor-agent-admission.ts';
import { emptyMemberAddition } from '../communities/member-addition.ts';
import { editorAgentMembers } from './editor-agent-members.fixtures.ts';
import { draftSession } from './store.fixtures.ts';

test('admission uses ANY ready owned writer from the unfiltered facts, not the picker search', () => {
  const snapshot = draftSession(),
    model = editorAgentMembers(snapshot, snapshot.conversations[0]!.id);
  model.set('waiting');
  const pending = model.current();
  model.set('ready');
  const ready = model.current();
  assert.equal(
    editorAgentAdmission({
      ...emptyMemberAddition,
      phase: 'ready',
      canManage: true,
      results: [pending],
      allResults: [pending, ready],
    }),
    'ready',
  );
});

test('a reader remains an associated reader, without becoming an editing candidate or a perpetual join', () => {
  const snapshot = draftSession(),
    model = editorAgentMembers(snapshot, snapshot.conversations[0]!.id);
  model.set('reader');
  const reader = model.current();
  assert.equal(reader.delivery_ready, true);
  assert.equal(
    editorAgentAdmission({
      ...emptyMemberAddition,
      phase: 'ready',
      canManage: true,
      results: [reader],
      allResults: [reader],
    }),
    'unavailable',
  );
});

test('failed or loading observations cannot revive a stale ready fact', () => {
  const snapshot = draftSession(),
    model = editorAgentMembers(snapshot, snapshot.conversations[0]!.id);
  for (const phase of ['failed', 'loading'] as const)
    assert.equal(
      editorAgentAdmission({ ...emptyMemberAddition, phase, allResults: [model.current()] }),
      phase,
    );
});
