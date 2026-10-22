import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  DefinitionSession,
  type DefinitionPort,
  type DefinitionScope,
} from '../agent-definition/session.ts';
import type { AgentDefinitionDto } from './generated/AgentDefinitionDto.ts';
import type { AgentDefinitionPreviewDto } from './generated/AgentDefinitionPreviewDto.ts';
import type { AgentDefinitionUpdateDto } from './generated/AgentDefinitionUpdateDto.ts';

const scope: DefinitionScope = {
  owner: 'owner',
  device: 'phone',
  relay: 'https://relay.example',
  agent: 'agent',
};
const baseline: AgentDefinitionDto = {
  agent_id: scope.agent,
  owner_id: scope.owner,
  device_id: scope.device,
  relay_url: scope.relay,
  status: 'Active',
  version: 1,
  definition_sha256: 'base-hash',
  persona: 'Original persona',
  rules: ['Preserve order.', 'Ask first.'],
};
const proposed: AgentDefinitionDto = {
  ...baseline,
  version: 2,
  definition_sha256: 'proposed-hash',
  persona: 'Edited persona',
  rules: ['Ask first.', 'Preserve order.'],
};
const review: AgentDefinitionPreviewDto = {
  current: baseline,
  proposed,
  preview_digest: 'review-digest',
};
const held = (
  outcome: AgentDefinitionUpdateDto['outcome'] = 'Unknown',
  current: AgentDefinitionDto | null = baseline,
): AgentDefinitionUpdateDto => ({ receipt_id: 'receipt', review, outcome, current });
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
function fixture() {
  const calls: string[] = [];
  let receipt: AgentDefinitionUpdateDto | null = null;
  const port: DefinitionPort = {
    read: async () => {
      calls.push('read');
      return baseline;
    },
    preview: async (_scope, base, persona, rules) => {
      calls.push('preview');
      return {
        current: base,
        proposed: { ...proposed, persona, rules },
        preview_digest: 'review-digest',
      };
    },
    update: async (_scope, observed) => {
      calls.push('update');
      receipt = { ...held('Confirmed', observed.proposed), review: observed };
      return receipt;
    },
    reconcile: async () => {
      calls.push('reconcile');
      return receipt;
    },
    dismiss: async () => {
      calls.push('dismiss');
      receipt = null;
      return true;
    },
  };
  const session = new DefinitionSession(port);
  session.activate(scope);
  return {
    session,
    port,
    calls,
    setReceipt: (value: AgentDefinitionUpdateDto | null) => {
      receipt = value;
    },
  };
}
async function ready() {
  const f = fixture();
  await f.session.load();
  f.session.edit(proposed.persona, proposed.rules);
  await f.session.preview();
  return f;
}

test('cold entry reconciles first; only successful null permits editable authoritative read', async () => {
  const f = fixture();
  const pending = deferred<AgentDefinitionUpdateDto | null>();
  f.port.reconcile = async () => {
    f.calls.push('reconcile');
    return pending.promise;
  };
  const load = f.session.load();
  void f.session.load();
  f.session.setPersona('must not enter while loading');
  assert.deepEqual(f.calls, ['reconcile']);
  assert.equal(f.session.getSnapshot().persona, '');
  pending.resolve(null);
  await load;
  assert.deepEqual(f.calls, ['reconcile', 'read']);
  assert.equal(f.session.getSnapshot().phase, 'editing');
  assert.equal(f.session.getSnapshot().persona, baseline.persona);
  assert.equal(f.session.dirty(), false);
});
test('unsupported or failed reconcile cannot become empty editable content', async () => {
  const f = fixture();
  f.port.reconcile = async () => {
    throw Error('old relay/core');
  };
  await f.session.load();
  assert.equal(f.session.getSnapshot().phase, 'error');
  assert.equal(f.session.confirmation(), null);
  assert.deepEqual(f.calls, []);
});
for (const changed of [
  null,
  { ...scope, owner: 'other' },
  { ...scope, device: 'other' },
  { ...scope, relay: 'https://other.example' },
  { ...scope, agent: 'other' },
] as const) {
  test(`late initial read and preview cannot publish across retirement/scope ${JSON.stringify(changed)}`, async () => {
    const f = fixture(),
      read = deferred<AgentDefinitionDto>();
    f.port.read = () => read.promise;
    const pending = f.session.load();
    await Promise.resolve();
    f.session.activate(changed);
    read.resolve(baseline);
    await pending;
    assert.equal(f.session.getSnapshot().current, null);
    f.session.activate(scope);
    f.port.read = async () => baseline;
    await f.session.load();
    f.session.setPersona(proposed.persona);
    const preview = deferred<AgentDefinitionPreviewDto>();
    f.port.preview = () => preview.promise;
    const previewing = f.session.preview();
    f.session.activate(changed);
    preview.resolve(review);
    await previewing;
    assert.equal(f.session.getSnapshot().review, null);
  });
}
test('editing while preview is pending retires that review and keeps exact Unicode/whitespace/rule order', async () => {
  const f = fixture();
  await f.session.load();
  f.session.edit(proposed.persona, proposed.rules);
  const pending = deferred<AgentDefinitionPreviewDto>();
  f.port.preview = () => pending.promise;
  const previewing = f.session.preview();
  const text = '  e\u0301 🪷\nexact  ';
  f.session.edit(text, ['duplicate', 'duplicate', ' order ']);
  pending.resolve(review);
  await previewing;
  assert.equal(f.session.getSnapshot().phase, 'editing');
  assert.equal(f.session.getSnapshot().persona, text);
  assert.deepEqual(f.session.getSnapshot().rules, ['duplicate', 'duplicate', ' order ']);
  assert.equal(f.session.confirmation(), null);
});
test('removed-rule native callbacks cannot edit the next rule; current input callbacks preserve concurrent persona', async () => {
  const f = fixture();
  await f.session.load();
  const [first, second] = f.session.getSnapshot().ruleKeys;
  f.session.removeRule(first!);
  f.session.setRule(first!, 'stale removed input');
  f.session.setPersona('new persona');
  f.session.setRule(second!, 'new remaining rule');
  assert.equal(f.session.getSnapshot().persona, 'new persona');
  assert.deepEqual(f.session.getSnapshot().rules, ['new remaining rule']);
  f.session.removeRule(second!);
  assert.deepEqual(f.session.getSnapshot().rules, []);
});
test('only current displayed review applies once; edit revision invalidates old approval/discard tickets', async () => {
  const f = await ready(),
    old = f.session.confirmation()!,
    discard = f.session.ticket()!;
  f.session.setPersona('next text');
  await f.session.apply(old);
  await f.session.reload(discard);
  assert.equal(f.calls.includes('update'), false);
  assert.equal(f.session.getSnapshot().persona, 'next text');
  await f.session.preview();
  const approval = f.session.confirmation()!;
  const first = f.session.apply(approval),
    second = f.session.apply(approval);
  await Promise.all([first, second]);
  assert.equal(f.calls.filter((call) => call === 'update').length, 1);
  assert.equal(f.session.getSnapshot().receipt?.outcome, 'Confirmed');
  assert.equal(f.session.confirmation(), null);
});
for (const current of [baseline, proposed, null]) {
  test(`Unknown stays held when current is ${current?.definition_sha256 ?? 'null'}; cold and refresh never update`, async () => {
    const f = await ready();
    f.setReceipt(held('Unknown', current));
    f.port.update = async () => {
      f.calls.push('update');
      throw Error('lost ACK after receipt');
    };
    await f.session.apply(f.session.confirmation()!);
    assert.equal(f.session.getSnapshot().phase, 'held');
    assert.equal(f.session.getSnapshot().receiptKnown, false);
    await f.session.reconcile();
    assert.equal(f.session.getSnapshot().receipt?.outcome, 'Unknown');
    assert.equal(f.session.confirmation(), null);
    const cold = new DefinitionSession(f.port);
    cold.activate(scope);
    const before = f.calls.length;
    await cold.load();
    assert.deepEqual(f.calls.slice(before), ['reconcile']);
    assert.equal(cold.getSnapshot().phase, 'held');
    assert.equal(cold.getSnapshot().receipt?.outcome, 'Unknown');
    await cold.reconcile();
    assert.equal(f.calls.filter((call) => call === 'update').length, 1);
  });
}
test('null reconcile after unknown does not automatically unlock or resend; explicit forget reloads fresh only', async () => {
  const f = await ready();
  f.port.update = async () => {
    f.calls.push('update');
    throw Error('uncertain boundary');
  };
  await f.session.apply(f.session.confirmation()!);
  await f.session.reconcile();
  assert.equal(f.session.getSnapshot().phase, 'held');
  assert.equal(f.session.getSnapshot().receiptKnown, true);
  const before = f.calls.length;
  await f.session.dismiss(f.session.dismissal()!);
  assert.deepEqual(f.calls.slice(before), ['reconcile', 'reconcile', 'read']);
  assert.equal(f.session.getSnapshot().phase, 'editing');
  assert.equal(f.calls.filter((call) => call === 'update').length, 1);
});
for (const mutate of [
  (value: AgentDefinitionUpdateDto) => ({ ...value, current: null }),
  (value: AgentDefinitionUpdateDto) => ({ ...value, current: { ...proposed, device_id: 'other' } }),
  (value: AgentDefinitionUpdateDto) => ({
    ...value,
    current: { ...proposed, definition_sha256: 'wrong' },
  }),
  (value: AgentDefinitionUpdateDto) => ({
    ...value,
    current: { ...proposed, persona: 'wrong content' },
  }),
  (value: AgentDefinitionUpdateDto) => ({
    ...value,
    review: { ...review, proposed: { ...proposed, persona: 'different reviewed text' } },
    current: { ...proposed, persona: 'different reviewed text' },
  }),
]) {
  test('malformed Confirmed cannot display saved or restore Apply', async () => {
    const f = await ready();
    f.port.update = async () => mutate(held('Confirmed', proposed));
    await f.session.apply(f.session.confirmation()!);
    assert.equal(f.session.getSnapshot().phase, 'held');
    assert.equal(f.session.getSnapshot().receipt, null);
    assert.equal(f.session.confirmation(), null);
  });
}
for (const fail of ['false', 'throw']) {
  test(`CAS dismissal ${fail} re-reads newer receipt and preserves the edit lock`, async () => {
    const f = fixture();
    f.setReceipt(held());
    await f.session.load();
    const old = f.session.dismissal()!;
    const newer = {
      ...held('Conflict', { ...baseline, version: 3, definition_sha256: 'newer' }),
      receipt_id: 'newer',
      review: { ...review, preview_digest: 'new-review' },
    };
    f.port.dismiss = async () => {
      f.calls.push('dismiss');
      f.setReceipt(newer);
      if (fail === 'throw') throw Error('lost ACK');
      return false;
    };
    await f.session.dismiss(old);
    assert.equal(f.session.getSnapshot().receipt?.receipt_id, 'newer');
    assert.equal(f.session.getSnapshot().phase, 'held');
    const before = [...f.calls];
    await f.session.dismiss(old);
    assert.deepEqual(f.calls, before);
    assert.equal(f.calls.includes('read'), false);
    assert.equal(f.calls.includes('update'), false);
  });
}
test('dismissal true only permits fresh reconcile/read; lost ACK with no held receipt is read-only recovered', async () => {
  for (const lost of [false, true]) {
    const f = fixture();
    f.setReceipt(held('Confirmed', proposed));
    await f.session.load();
    f.port.dismiss = async () => {
      f.calls.push('dismiss');
      f.setReceipt(null);
      if (lost) throw Error('ACK lost');
      return true;
    };
    await f.session.dismiss(f.session.dismissal()!);
    assert.equal(f.session.getSnapshot().phase, 'editing');
    assert.equal(f.session.getSnapshot().current?.version, 1);
    assert.equal(f.calls.includes('update'), false);
  }
});
test('revocation allows reading but never preview/update, and unsupported/mismatched scope stays closed', async () => {
  const f = fixture();
  f.port.read = async () => ({ ...baseline, status: 'Revoked' });
  await f.session.load();
  f.session.setPersona('attempt');
  await f.session.preview();
  assert.equal(f.session.getSnapshot().persona, baseline.persona);
  assert.equal(f.session.confirmation(), null);
  assert.equal(f.calls.includes('preview'), false);
  f.session.activate(scope);
  f.port.read = async () => ({ ...baseline, owner_id: 'foreign' });
  await f.session.load();
  assert.equal(f.session.getSnapshot().phase, 'error');
});
test('closing during update retires visible callbacks without dismissing persisted receipt; reopening restores it', async () => {
  const f = await ready(),
    pending = deferred<AgentDefinitionUpdateDto>();
  f.port.update = async () => {
    f.calls.push('update');
    f.setReceipt(held());
    return pending.promise;
  };
  const applying = f.session.apply(f.session.confirmation()!);
  const old = f.session.ticket()!;
  f.session.retire();
  pending.resolve(held());
  await applying;
  assert.equal(f.session.acceptsSession(old), false);
  assert.equal(f.session.getSnapshot().review, null);
  assert.equal(f.calls.includes('dismiss'), false);
  f.session.activate(scope);
  await f.session.load();
  assert.equal(f.session.getSnapshot().phase, 'held');
  assert.equal(f.calls.filter((call) => call === 'update').length, 1);
});
