import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import * as sessionModule from '../agent-enrollment/session.ts';
import { createEnrollmentPort } from '../agent-enrollment/core-port.ts';
import type { useAgentEnrollment } from '../agent-enrollment/use-agent-enrollment';
import type { AgentEnrollmentPreviewDto } from './generated/AgentEnrollmentPreviewDto.ts';
import type { AgentEnrollmentStatusDto } from './generated/AgentEnrollmentStatusDto.ts';
import type { ClientState } from './store.ts';
import type { Command } from './types.ts';

const output = await build({
  stdin: {
    contents: readFileSync(
      new URL('../agent-enrollment/use-agent-enrollment.ts', import.meta.url),
      'utf8',
    ),
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  plugins: [
    {
      name: 'enrollment-hook-platform-ports',
      setup(builder) {
        builder.onResolve({ filter: /./ }, ({ path }) => ({ path, external: true }));
      },
    },
  ],
});
const preview: AgentEnrollmentPreviewDto = {
  agent_id: 'agent',
  offer_digest: 'digest',
  name: 'Booking helper',
  handle: 'booking',
  persona: 'Plans trips.',
  rules: ['Ask before booking.'],
  pairing_check: '123456',
  expires_ms: '9999999999999',
};
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}
async function settle() {
  for (let i = 0; i < 16; i++) await Promise.resolve();
}
function fixture(platform = 'ios') {
  const state = {
    phase: 'ready',
    mode: 'account',
    erasePending: false,
    snapshot: {
      account: {
        identity_id: 'owner',
        device_id: 'phone',
        relay_url: 'https://relay.example',
        registered: true,
        unlocked: true,
        setup_pending: false,
      },
    },
  } as ClientState;
  const calls: Command[] = [];
  const listeners = new Set<() => void>();
  const cleanups = new Set<() => Promise<void>>();
  let rpc: (command: Command) => Promise<unknown> = async (command) =>
    command.op === 'agent_enrollment_status' ? null : command.op === 'find_people' ? [] : preview;
  const request = async (command: Command) => {
    calls.push(command);
    return rpc(command);
  };
  const store = {
    getSnapshot: () => state,
    driver: { request },
    request,
    subscribe: (callback: () => void) => {
      listeners.add(callback);
      return () => {
        listeners.delete(callback);
      };
    },
    registerSessionCleanup: (callback: () => Promise<void>) => {
      cleanups.add(callback);
      return () => {
        cleanups.delete(callback);
      };
    },
  };
  let cursor = 0;
  const slots: any[] = [];
  const memo = (factory: () => any, deps: unknown[]) => {
    const i = cursor++;
    if (!slots[i] || !deps.every((value, index) => Object.is(value, slots[i].deps[index])))
      slots[i] = { value: factory(), deps };
    return slots[i].value;
  };
  let focus: (() => void | (() => void)) | undefined;
  let cleanup: (() => void) | undefined;
  let nextFocus: (() => void | (() => void)) | undefined;
  let barcode: ((result: { data: string }) => void) | undefined;
  let launches = 0,
    dismissals = 0;
  let permission: () => Promise<{ granted: boolean }> = async () => ({ granted: true });
  const module: { exports: Record<string, any> } = { exports: {} };
  runInNewContext(output.outputFiles[0]!.text, {
    module,
    process: { env: { EXPO_OS: platform } },
    require: (name: string) => {
      if (name === 'react')
        return {
          useMemo: memo,
          useCallback: (fn: () => any, deps: unknown[]) => memo(() => fn, deps),
          useRef: (value: unknown) => {
            const i = cursor++;
            return (slots[i] ??= { current: value });
          },
          useState: (initial: unknown) => {
            const i = cursor++;
            slots[i] ??= { value: initial };
            return [
              slots[i].value,
              (next: any) => {
                slots[i].value = typeof next === 'function' ? next(slots[i].value) : next;
              },
            ];
          },
          useSyncExternalStore: (_subscribe: unknown, snapshot: () => unknown) => snapshot(),
        };
      if (name === 'expo-router')
        return {
          useFocusEffect: (callback: typeof focus) => {
            if (callback !== focus) nextFocus = callback;
          },
        };
      if (name === 'expo-camera')
        return {
          useCameraPermissions: () => [null, () => permission()],
          CameraView: {
            isModernBarcodeScannerAvailable: true,
            onModernBarcodeScanned: (callback: typeof barcode) => {
              barcode = callback;
              return {
                remove: () => {
                  barcode = undefined;
                },
              };
            },
            launchScanner: async () => {
              launches++;
            },
            dismissScanner: async () => {
              dismissals++;
            },
          },
        };
      if (name === '../core/provider') return { useClient: () => ({ store, ...state }) };
      if (name === './core-port') return { createEnrollmentPort };
      if (name === './session') return sessionModule;
      throw Error(`Unexpected hook import: ${name}`);
    },
  });
  return {
    calls,
    state,
    cleanups,
    handle: (handler: typeof rpc) => {
      rpc = handler;
    },
    permission: (request: typeof permission) => {
      permission = request;
    },
    changed: () => {
      for (const listener of listeners) listener();
    },
    barcode: () => barcode,
    counts: () => ({ launches, dismissals }),
    render: () => {
      cursor = 0;
      const value = module.exports.useAgentEnrollment() as ReturnType<typeof useAgentEnrollment>;
      if (nextFocus) {
        cleanup?.();
        focus = nextFocus;
        nextFocus = undefined;
        cleanup = focus!() || undefined;
      }
      return value;
    },
    close: () => {
      cleanup?.();
      cleanup = undefined;
    },
  };
}

test('actual hook probes read-only before enabling input; old-core failure never dispatches approval', async () => {
  const f = fixture();
  const pending = deferred<null>();
  f.handle(() => pending.promise);
  let flow = f.render();
  assert.equal(flow.availability, 'loading');
  flow.setCode('code');
  await flow.scan();
  assert.equal(f.calls.length, 1);
  assert.equal(f.calls[0]!.op, 'agent_enrollment_status');
  assert.deepEqual(f.counts(), { launches: 0, dismissals: 0 });
  pending.reject(Error('unknown command in older core'));
  await settle();
  flow = f.render();
  assert.equal(flow.availability, 'restore-error');
  flow.setCode('code');
  await flow.session.check();
  assert.equal(f.calls.length, 1);
  f.handle(async () => null);
  flow.retryRestore();
  f.render();
  await settle();
  assert.equal(f.render().availability, 'ready');
  f.close();
});

test('actual native scan callback only previews, consumes duplicate events and never approves', async () => {
  const f = fixture();
  f.render();
  await settle();
  const flow = f.render();
  await flow.scan();
  const scan = f.barcode()!;
  scan({ data: 'synthetic-code' });
  scan({ data: 'duplicate' });
  await settle();
  assert.deepEqual(
    f.calls.map((command) => command.op),
    ['agent_enrollment_status', 'agent_enrollment_preview'],
  );
  assert.equal(f.render().state.phase, 'review');
  assert.equal(f.render().state.review!.pairing, '123456');
  assert.deepEqual(f.counts(), { launches: 1, dismissals: 1 });
  f.close();
});

test('actual scanner denial leaves paste available and dispatches no preview or approval', async () => {
  const f = fixture();
  f.permission(async () => ({ granted: false }));
  f.render();
  await settle();
  await f.render().scan();
  assert.equal(f.render().scannerError, 'denied');
  assert.deepEqual(
    f.calls.map((command) => command.op),
    ['agent_enrollment_status'],
  );
  assert.equal(f.counts().launches, 0);
  f.close();
});

test('retirement while camera permission is pending prevents launch and late barcode publication', async () => {
  const f = fixture();
  const permission = deferred<{ granted: boolean }>();
  f.permission(() => permission.promise);
  f.render();
  await settle();
  const flow = f.render();
  const scan = flow.scan();
  const barcode = f.barcode()!;
  f.close();
  permission.resolve({ granted: true });
  await scan;
  barcode({ data: 'late-code' });
  await settle();
  assert.equal(f.counts().launches, 0);
  assert.equal(flow.session.getSnapshot().code, '');
  assert.equal(flow.session.getSnapshot().review, null);
  assert.deepEqual(
    f.calls.map((command) => command.op),
    ['agent_enrollment_status'],
  );
});

for (const field of ['identity_id', 'device_id', 'relay_url'] as const) {
  test(`actual hook immediately retires scan and code when ${field} changes`, async () => {
    const f = fixture();
    f.render();
    await settle();
    const flow = f.render();
    await flow.scan();
    const oldScan = f.barcode()!;
    f.state.snapshot!.account![field] = 'other';
    f.changed();
    oldScan({ data: 'old-code' });
    assert.equal(flow.session.getSnapshot().code, '');
    assert.equal(flow.session.getSnapshot().review, null);
    assert.deepEqual(
      f.calls.map((command) => command.op),
      ['agent_enrollment_status'],
    );
    f.close();
  });
}

test('cold receipt restores waiting and reconciles existing APIs without an enrollment send', async () => {
  const f = fixture();
  const receipt: AgentEnrollmentStatusDto = {
    preview,
    delivery: {
      receipt_id: 'receipt',
      agent_id: 'agent',
      offer_digest: 'digest',
      outcome: 'Unknown',
    },
  };
  f.handle(async (command) => (command.op === 'agent_enrollment_status' ? receipt : []));
  f.render();
  await settle();
  const flow = f.render();
  assert.equal(flow.state.phase, 'waiting');
  assert.equal(flow.state.outcome, 'Unknown');
  assert.equal(flow.state.code, '');
  assert.equal(flow.session.confirmation(), null);
  assert.deepEqual(
    f.calls.map((command) => command.op),
    ['agent_enrollment_status', 'agent_enrollment_status', 'find_people'],
  );
  f.close();
});

test('unsupported non-iOS flow has no native enrollment RPC or scanner action', async () => {
  const f = fixture('web');
  f.render();
  await settle();
  const flow = f.render();
  assert.equal(flow.availability, 'unsupported');
  await flow.scan();
  flow.setCode('code');
  assert.equal(f.calls.length, 0);
  assert.equal(f.counts().launches, 0);
  f.close();
});
