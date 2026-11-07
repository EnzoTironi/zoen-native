import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { dirname } from 'node:path';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { build } from 'esbuild';
import type { RequestCard } from '../agents/request-card';
import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { Command } from './types';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
const require = createRequire(import.meta.url);
function compileCard(path: string) {
  const diagnostics: string[] = [];
  const output = require('@babel/core').transformSync(readFileSync(path, 'utf8'), {
    filename: path,
    babelrc: false,
    configFile: false,
    presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
    plugins: [
      [
        'babel-plugin-react-compiler',
        {
          target: '19',
          logger: {
            logEvent(
              _file: unknown,
              event: { kind: string; detail?: { options?: { reason?: string } } },
            ) {
              if (event.kind === 'CompileError' || event.kind === 'CompileDiagnostic')
                diagnostics.push(event.detail?.options?.reason ?? event.kind);
            },
          },
        },
      ],
    ],
    caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: true },
  }).code as string;
  // Preserve the existing card's normal compiler fallback; the lease still
  // must optimize. Do not suppress any new compiler diagnostic.
  assert.deepEqual(diagnostics, [
    "(BuildHIR::lowerStatement) Handle TryStatement with a finalizer ('finally') clause",
  ]);
  return output;
}
const compiled = await build({
  entryPoints: [fileURLToPath(new URL('../agents/request-card.tsx', import.meta.url))],
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
  external: [
    'react',
    'react/compiler-runtime',
    'react/jsx-runtime',
    'expo-router',
    'react-native',
    '../core/provider',
    '../components/primitives',
    '../components/dialogs',
    '../i18n/index.ts',
    '../theme',
    './tool-review-recovery',
    './drawing-tool-review',
    './site-tool-review',
    './app-request-review-action',
  ],
  plugins: [
    {
      name: 'actual-expo-compiler',
      setup(build) {
        build.onLoad(
          { filter: /(request-card\.tsx|use-request-decision-lease\.ts)$/ },
          ({ path }) => ({
            contents: path.endsWith('request-card.tsx')
              ? compileCard(path)
              : compileExpoSource(path, true),
            loader: 'js',
            resolveDir: dirname(path),
          }),
        );
      },
    },
  ],
});
type Button = { label: string; disabled?: boolean; onPress(): void };
/** Actual Expo-compiled card, optimized lease, React and ClientStore. Native results and presentation are synthetic. */
export async function conversationRequestFixture() {
  const root = createReactTestRoot(),
    driver = new Driver(),
    snapshot = draftSession();
  const owner = snapshot.account;
  assert.ok(owner);
  const member = snapshot.conversations[0]?.members[0];
  assert.ok(member);
  const request: AgentRequestDto = {
    id: 'search-review',
    space_id: snapshot.conversations[0]!.id,
    space_title: 'Our chat',
    agent: {
      ...member,
      id: 'agent',
      name: 'Assistant',
      kind: 'Agent',
      handle: 'assistant',
      is_me: false,
      is_mine: true,
      owner_id: owner.identity_id,
    },
    title: 'Search conversation history',
    detail: 'Search for the earlier decision',
    audience: 'This conversation',
    action_label: 'Search history',
    cost_cents: null,
    reason: 'Needs your decision',
    status: 'Pending',
    opened_ms: String(Date.now()),
    resolved_ms: null,
    item_id: null,
    line_id: null,
    action_key: 'search-history',
    can_always_approve: false,
    by_standing: false,
    tool_approval: {
      content_hash: 'ab'.repeat(32),
      artifact_id: '',
      artifact_title: 'Our chat',
      version: 0,
      operation: 'Search history',
      expires_ms: String(Date.now() + 900_000),
      canonical_tool: 'search_conversation_history',
      preview_kind: 'conversation',
    },
  };
  snapshot.requests = [request];
  const commands: Command[] = [],
    buttons = new Map<string, Button>();
  let resolved = 0;
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    if (command.op === 'snapshot') return structuredClone(snapshot);
    commands.push(command);
    if (command.op === 'decide_tool_request') {
      snapshot.requests = snapshot.requests.map((row) => ({
        ...row,
        status:
          command.decision === 'ApproveOnce'
            ? 'Approved'
            : command.decision === 'Deny'
              ? 'Denied'
              : 'Cancelled',
      }));
      return { message: 'Decision saved', also_resolved: 0 };
    }
    throw Error(`Unexpected native command ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const appListeners = new Set<(value: string) => void>();
  const app = {
    currentState: 'active',
    addEventListener(_: string, callback: (value: string) => void) {
      appListeners.add(callback);
      return { remove: () => appListeners.delete(callback) };
    },
  };
  const useClient = () => ({
    store,
    ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
  });
  const children = ({ children }: { children: React.ReactNode }) => children;
  const module = { exports: {} as { RequestCard: typeof RequestCard } };
  runInNewContext(compiled.outputFiles[0]!.text, {
    module,
    exports: module.exports,
    require: (name: string) => {
      if (name === 'react') return React;
      if (
        name === 'react/compiler-runtime' ||
        name === 'react/jsx-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'react-native')
        return { View: children, Text: () => null, Pressable: children, AppState: app };
      if (name === 'expo-router')
        return {
          router: { push: () => assert.fail('A conversation review has no App to open') },
          useFocusEffect: (effect: React.EffectCallback) => React.useEffect(effect, [effect]),
        };
      if (name === '../core/provider') return { useClient };
      if (name === '../i18n/index.ts') return { t: (key: string) => key, formatLocale: 'en-US' };
      if (name === '../theme') return { useTheme: () => ({ colors: {} }) };
      if (name === '../components/dialogs')
        return {
          useDialogs: () => ({ confirm: () => assert.fail('No standing consent for a tool call') }),
        };
      if (name === '../components/primitives')
        return {
          Avatar: () => null,
          Surface: children,
          NativeButton: (props: Button) => {
            React.useLayoutEffect(() => {
              buttons.set(props.label, props);
              return () => {
                buttons.delete(props.label);
              };
            }, [props]);
            return null;
          },
        };
      if (name === './tool-review-recovery') return { ToolReviewRecovery: () => null };
      if (name === './drawing-tool-review')
        return { DrawingToolReview: () => assert.fail('No drawing in a conversation review') };
      if (name === './site-tool-review')
        return { SiteToolReview: () => assert.fail('No site in a conversation review') };
      if (name === './app-request-review-action')
        return { AppRequestReviewAction: () => assert.fail('No App in a conversation review') };
      throw Error(name);
    },
  });
  function Probe() {
    const state = useClient();
    const current = state.snapshot?.requests[0];
    return current
      ? React.createElement(module.exports.RequestCard, {
          request: current,
          onResolved: () => {
            resolved++;
          },
        })
      : null;
  }
  await root.render(React.createElement(Probe));
  return {
    root,
    store,
    snapshot,
    commands,
    buttons,
    request,
    resolved: () => resolved,
    async press(label: string) {
      const button = buttons.get(label);
      assert.ok(button, `Expected visible ${label}`);
      assert.equal(!!button.disabled, false);
      await root.act(button.onPress);
    },
    async reopen() {
      await root.act(() => store.close());
      await root.act(() => store.open('account'));
    },
    async close() {
      await root.close();
      await store.close();
      assert.deepEqual(root.errors, []);
    },
  };
}
