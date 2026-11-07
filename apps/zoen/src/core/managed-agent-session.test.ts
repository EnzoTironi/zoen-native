import assert from 'node:assert/strict';
import { test } from 'node:test';
import { ManagedAgentSession, type ManagedAgentPort } from '../managed-agent/session.ts';
import type { ManagedEnrollmentDto } from './generated/ManagedEnrollmentDto.ts';
const owner = { identity: 'owner', device: 'phone', relay: 'relay', epoch: 1 };
function view(state = 'queued'): ManagedEnrollmentDto {
  return {
    operation_id: 'operation',
    state,
    name: 'Assistant',
    handle: 'agent_handle',
    agent_id: state === 'queued' ? null : 'agent',
    preview:
      state === 'review'
        ? {
            agent_id: 'agent',
            offer_digest: 'digest',
            name: 'Assistant',
            handle: 'agent_handle',
            persona: 'Help me',
            rules: ['Ask before effects'],
            expires_ms: '9999999999999',
            pairing_check: '',
          }
        : null,
  };
}
function deferred<T>() {
  let resolve: (value: T) => void = () => {
    throw Error('not initialized');
  };
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function fixture(initial: ManagedEnrollmentDto | null = null) {
  const calls: string[] = [];
  const port: ManagedAgentPort = {
    discover: async () => ({ requests: [], next_operation: null }),
    recover: async () => view(),
    status: async () => {
      calls.push('status');
      return initial;
    },
    start: async (name) => {
      calls.push(`start:${name}`);
      return view();
    },
    approve: async () => {
      calls.push('approve');
      return view('installing');
    },
    verify: async () => {
      calls.push('verify');
    },
    chat: async () => {
      calls.push('chat');
      return 'space';
    },
    cancel: async () => {
      calls.push('cancel');
      return view('cancelling');
    },
    discard: async () => {
      calls.push('discard');
    },
    complete: async () => {
      calls.push('complete');
    },
  };
  const session = new ManagedAgentSession(port);
  session.activate(owner);
  return { port, session, calls };
}
test('double creation and overlapping status reads cannot mint another intent', async () => {
  const f = fixture();
  await f.session.refresh();
  const gate = deferred<ManagedEnrollmentDto>();
  f.port.start = async (name) => {
    f.calls.push(`start:${name}`);
    return gate.promise;
  };
  const started = f.session.start(' Assistant ');
  await f.session.start('Other');
  await f.session.refresh();
  assert.deepEqual(f.calls, ['status', 'start:Assistant']);
  gate.resolve(view());
  await started;
  assert.equal(f.session.getSnapshot().view?.operation_id, 'operation');
});
test('lost create ACK closes entry until recovery reads the original operation', async () => {
  const f = fixture();
  await f.session.refresh();
  f.port.start = async () => {
    throw Error('ACK lost');
  };
  await f.session.start('Assistant');
  assert.equal(f.session.getSnapshot().ready, false);
  await f.session.start('Different agent');
  f.port.status = async () => view('review');
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().view?.state, 'review');
  assert.equal(f.session.getSnapshot().error, null);
});
test('ambiguous approval requires status recovery, never an automatic resubmit', async () => {
  const f = fixture(view('review'));
  await f.session.refresh();
  f.port.approve = async () => {
    f.calls.push('approve');
    throw Error('ACK lost');
  };
  await f.session.approve();
  await f.session.approve();
  assert.deepEqual(f.calls, ['status', 'approve']);
  f.port.status = async () => view('installing');
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().view?.state, 'installing');
});
test('same-account reopen rejects every old asynchronous completion', async () => {
  const f = fixture();
  const gate = deferred<ManagedEnrollmentDto | null>();
  f.port.status = async () => gate.promise;
  const old = f.session.refresh(),
    ticket = f.session.ticket();
  f.session.retire();
  f.session.activate({ ...owner, epoch: 2 });
  gate.resolve(view('registered'));
  await old;
  assert.equal(f.session.accepts(ticket), false);
  assert.equal(f.session.scopeMatches(owner), false);
  assert.deepEqual(f.session.getSnapshot(), {
    recoveries: [],
    next: null,
    ready: false,
    view: null,
    busy: false,
    error: null,
  });
});
test('closing restores native installation without approving or funding again', async () => {
  const f = fixture(view('installing'));
  await f.session.refresh();
  f.session.retire();
  f.session.activate(owner);
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().view?.state, 'installing');
  assert.deepEqual(f.calls, ['status', 'status']);
});
test('budget uses canonical registration and retains onboarding; chat alone completes it', async () => {
  const f = fixture(view('registered'));
  await f.session.refresh();
  assert.equal(await f.session.open('budget'), 'agent');
  assert.deepEqual(f.calls, ['status', 'verify']);
  assert.equal(await f.session.open('chat'), 'space');
  assert.deepEqual(f.calls, ['status', 'verify', 'verify', 'chat', 'complete']);
});
test('retiring during verification prevents chat creation and completion', async () => {
  const f = fixture(view('registered'));
  await f.session.refresh();
  const gate = deferred<void>();
  f.port.verify = () => gate.promise;
  const open = f.session.open('chat');
  f.session.retire();
  gate.resolve();
  assert.equal(await open, null);
  assert.deepEqual(f.calls, ['status']);
});
test('failed completion retains the installed agent and prevents navigation', async () => {
  const f = fixture(view('registered'));
  await f.session.refresh();
  f.port.complete = async () => {
    throw Error('offline');
  };
  assert.equal(await f.session.open('chat'), null);
  assert.equal(f.session.getSnapshot().view?.state, 'registered');
  assert.equal(f.session.getSnapshot().error, 'open');
  f.port.complete = async () => {
    f.calls.push('complete');
  };
  assert.equal(await f.session.open('chat'), 'space');
});
test('review, expired, rejected and queued operations cannot open a chat', async () => {
  for (const state of ['queued', 'review', 'expired', 'rejected']) {
    const f = fixture(view(state));
    await f.session.refresh();
    assert.equal(await f.session.open('chat'), null);
    assert.deepEqual(f.calls, ['status']);
  }
});

test('only an unapproved terminal offer can be retired for a new attempt', async () => {
  for (const state of ['queued', 'review', 'installing', 'registered']) {
    const f = fixture(view(state));
    await f.session.refresh();
    await f.session.restart();
    assert.deepEqual(f.calls, ['status']);
  }
  const f = fixture(view('expired'));
  await f.session.refresh();
  await f.session.restart();
  assert.deepEqual(f.calls, ['status', 'discard']);
  assert.equal(f.session.getSnapshot().view, null);
  await f.session.start('New helper');
  assert.equal(f.session.getSnapshot().view?.state, 'queued');
});

test('callbacks from a rendered old owner or substituted offer cannot act on the current flow', async () => {
  const f = fixture(view('review'));
  await f.session.refresh();
  const oldTicket = f.session.ticket(),
    oldView = f.session.getSnapshot().view;
  f.session.activate({ ...owner, epoch: 2 });
  await f.session.refresh();
  await f.session.approve(oldTicket, oldView);
  await f.session.start('Old name', oldTicket);
  await f.session.restart(oldTicket, 'operation');
  assert.equal(await f.session.open('chat', oldTicket, 'operation'), null);
  assert.deepEqual(f.calls, ['status', 'status']);
  const current = f.session.ticket();
  const substituted = {
    ...view('review'),
    preview: { ...view('review').preview!, offer_digest: 'different' },
  };
  await f.session.approve(current, substituted);
  assert.deepEqual(f.calls, ['status', 'status']);
});

test('cancel waits for the host and cannot approve, restart or open a chat during resolution', async () => {
  const f = fixture(view('review'));
  await f.session.refresh();
  await f.session.cancel();
  await f.session.approve();
  await f.session.restart();
  assert.equal(await f.session.open('chat'), null);
  assert.equal(f.session.getSnapshot().view?.state, 'cancelling');
  assert.deepEqual(f.calls, ['status', 'cancel']);
  f.port.status = async () => view('cancelled');
  await f.session.refresh();
  await f.session.restart();
  assert.equal(f.session.getSnapshot().view, null);
});
test('lost cancellation ACK requires recovery, and a retired callback cannot cancel a new flow', async () => {
  const f = fixture(view('review'));
  await f.session.refresh();
  const ticket = f.session.ticket();
  f.port.cancel = async () => {
    throw Error('lost ACK');
  };
  await f.session.cancel();
  assert.equal(f.session.getSnapshot().ready, false);
  await f.session.approve();
  assert.deepEqual(f.calls, ['status']);
  f.session.activate({ ...owner, epoch: 2 });
  await f.session.refresh();
  await f.session.cancel(ticket, 'operation');
  assert.deepEqual(f.calls, ['status', 'status']);
});
test('accepted consent wins cancellation through canonical registration, never a phantom cancelled state', async () => {
  const f = fixture(view('review'));
  await f.session.refresh();
  f.port.cancel = async () => view('registered');
  await f.session.cancel();
  assert.equal(f.session.getSnapshot().view?.state, 'registered');
  await f.session.restart();
  assert.equal(f.session.getSnapshot().view?.state, 'registered');
});

test('discovery is read-only, paginates exact native cursors, and resumes only a displayed operation', async () => {
  const f = fixture();
  f.port.discover = async (after) => {
    f.calls.push(`discover:${after}`);
    return {
      requests: [
        { operation_id: after ? 'second' : 'first', name: 'Existing agent', state: 'queued' },
      ],
      next_operation: after ? null : 'first',
    };
  };
  f.port.recover = async (operation) => {
    f.calls.push(`recover:${operation}`);
    return { ...view(), operation_id: operation };
  };
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().view, null);
  assert.deepEqual(f.calls, ['status', 'discover:']);
  await f.session.recover('unobserved');
  assert.equal(f.calls.length, 2);
  await f.session.more();
  assert.deepEqual(
    f.session.getSnapshot().recoveries.map((r) => r.operation_id),
    ['first', 'second'],
  );
  assert.equal(f.session.getSnapshot().next, null);
  await f.session.recover('second');
  assert.equal(f.session.getSnapshot().view?.operation_id, 'second');
  assert.deepEqual(f.session.getSnapshot().recoveries, []);
  assert.deepEqual(f.calls, ['status', 'discover:', 'discover:first', 'recover:second']);
});

test('a late discovery or recovery cannot attach to a replaced account session', async () => {
  for (const phase of ['discover', 'recover']) {
    const f = fixture();
    const gate = deferred<void>();
    const entered = deferred<void>();
    f.port.discover = async () => {
      entered.resolve();
      if (phase === 'discover') await gate.promise;
      return {
        requests: [{ operation_id: 'operation', name: 'Existing', state: 'queued' }],
        next_operation: null,
      };
    };
    const refresh = f.session.refresh();
    if (phase === 'discover') await entered.promise;
    if (phase === 'recover') await refresh;
    let recovering: Promise<void> | undefined;
    if (phase === 'recover') {
      f.port.recover = async () => {
        await gate.promise;
        return view();
      };
      recovering = f.session.recover('operation');
    }
    f.session.activate({ ...owner, device: 'other-device', epoch: 2 });
    gate.resolve();
    await refresh;
    await recovering;
    assert.equal(f.session.getSnapshot().view, null);
    assert.deepEqual(f.session.getSnapshot().recoveries, []);
    assert.equal(f.session.getSnapshot().ready, false);
  }
});

test('unknown recovery acknowledgement blocks a new creation until native status returns the retained original', async () => {
  const f = fixture();
  f.port.discover = async () => ({
    requests: [{ operation_id: 'operation', name: 'Existing', state: 'queued' }],
    next_operation: null,
  });
  f.port.recover = async () => {
    throw Error('ACK lost after native adoption');
  };
  await f.session.refresh();
  await f.session.recover('operation');
  assert.equal(f.session.getSnapshot().ready, false);
  await f.session.start('Different');
  assert(!f.calls.some((s) => s.startsWith('start:')));
  f.port.status = async () => view();
  await f.session.refresh();
  assert.equal(f.session.getSnapshot().view?.operation_id, 'operation');
});

test('failed discovery cannot leave the new-creation form authorized by a previous empty observation', async () => {
  const f = fixture();
  await f.session.refresh();
  f.port.discover = async () => {
    throw Error('directory offline');
  };
  await f.session.refresh();
  await f.session.start('Different');
  assert.equal(f.session.getSnapshot().ready, false);
  assert(!f.calls.some((s) => s.startsWith('start:')));
});

test('Done completes onboarding without granting a conversation or budget', async () => {
  const f = fixture(view('registered'));
  await f.session.refresh();
  assert.equal(await f.session.finish(), true);
  assert.deepEqual(f.calls, ['status', 'complete']);
  assert.equal(f.session.getSnapshot().view, null);
  assert.equal(f.session.getSnapshot().ready, false);
});

test('lost finish acknowledgement and retired completion never enable a different creation', async () => {
  for (const retired of [false, true]) {
    const f = fixture(view('registered'));
    await f.session.refresh();
    const gate = deferred<void>();
    f.port.complete = async () => {
      await gate.promise;
      if (!retired) throw Error('ACK lost');
    };
    const finishing = f.session.finish();
    if (retired) f.session.retire();
    gate.resolve();
    assert.equal(await finishing, false);
    await f.session.start('Different');
    assert.equal(f.session.getSnapshot().ready, false);
    assert(!f.calls.some((s) => s.startsWith('start:')));
  }
});

test('a rendered state can obtain a ticket only for its exact current observation', async () => {
  const f = fixture(view('review'));
  const inactive = f.session.getSnapshot();
  await f.session.refresh();
  assert.equal(f.session.ticket(inactive), null);
  const rendered = f.session.getSnapshot();
  assert.notEqual(f.session.ticket(rendered), null);
  f.session.retire();
  f.session.activate({ ...owner, epoch: 2 });
  await f.session.refresh();
  assert.equal(f.session.ticket(rendered), null);
  assert.notEqual(f.session.ticket(f.session.getSnapshot()), null);
});
