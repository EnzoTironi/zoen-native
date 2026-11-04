import assert from 'node:assert/strict';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import { build } from 'esbuild';
import { AgentControlSession } from '../agents/agent-control-session.ts';
import type { AgentControlDto } from './generated/AgentControlDto';
import { deferred } from './store.fixtures.ts';

type Element = { type: string; props: Record<string, any> };
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../agents/agent-control-view.tsx', import.meta.url))],
  bundle: false,
  platform: 'node',
  format: 'cjs',
  write: false,
  jsx: 'automatic',
});
const module = { exports: {} as { AgentControlView(props: unknown): Element } };
runInNewContext(compiled.outputFiles[0]!.text, {
  module,
  exports: module.exports,
  require: (name: string) => {
    if (name === 'react/jsx-runtime')
      return {
        jsx: (type: string, props: Element['props']) => ({ type, props }),
        jsxs: (type: string, props: Element['props']) => ({ type, props }),
      };
    if (name === 'react-native') return { Text: 'Text', View: 'View' };
    if (name === '../components/primitives') return { NativeButton: 'Button' };
    if (name === '../i18n/index.ts') return { t: (key: string) => key };
    if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
    throw Error(name);
  },
});

test('status refresh retains the visible lifecycle action but disables both native and retained presses', async () => {
  const scope = { identity: 'owner', device: 'device', relay: 'relay', epoch: 1 };
  const pending = deferred<AgentControlDto>();
  let reads = 0,
    writes = 0;
  const session = new AgentControlSession(
    {
      scope: () => scope,
      read: async () =>
        ++reads === 1
          ? {
              status: 'Active',
              revision: '1',
              pending: null,
              operation_id: null,
              outcome: 'Observed',
            }
          : pending.promise,
      control: async () => {
        writes++;
      },
      retry: async () => {
        writes++;
      },
    },
    'agent',
  );
  session.activate(scope);
  await session.refresh();
  const action = () => {
    const view = module.exports.AgentControlView({ state: session.getSnapshot(), session });
    return (view.props.children as Element[]).find(
      (child) => child?.props?.testID === 'agent-lifecycle-change',
    )!;
  };
  const before = action();
  assert.equal(before.props.label, 'agents.pauseAgent');
  assert.equal(before.props.disabled, false);
  const refresh = session.refresh();
  const during = action();
  assert.equal(during.props.label, before.props.label);
  assert.equal(during.props.disabled, true);
  before.props.onPress();
  during.props.onPress();
  assert.equal(writes, 0);
  pending.resolve({
    status: 'Paused',
    revision: '2',
    pending: null,
    operation_id: null,
    outcome: 'Observed',
  });
  await refresh;
  const after = action();
  assert.equal(after.props.label, 'agents.resumeAgent');
  assert.equal(after.props.disabled, false);
  assert.equal(writes, 0);
});
