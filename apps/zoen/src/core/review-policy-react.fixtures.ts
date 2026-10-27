import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import * as ownership from './account-scope.ts';
import * as decision from './tool-request-decision.ts';
import * as review from './tool-review-renewal.ts';
import * as policy from '../owner-policy/session.ts';
import * as policyCore from '../owner-policy/core-port.ts';
import { imageToolReviewValid } from './image-tool-review.ts';
import * as siteSource from '../sites/source.ts';
import * as appReview from '../miniapps/app-request-review.ts';
import * as documentAuthority from '../documents/document-authority.ts';
import type { useToolReview } from '../agents/use-tool-review.ts';
import type { useOwnerPolicy } from '../owner-policy/use-owner-policy.ts';
import type { AgentRequestDto } from './generated/AgentRequestDto';
import type { DecideOutcome } from './generated/DecideOutcome';
import type { OwnerPolicyOptionsStatus } from './generated/OwnerPolicyOptionsStatus';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const compile = (file: URL) =>
  transformSync(
    readFileSync(
      file.pathname.endsWith('/agents/request-card.tsx') && process.env.ZOEN_REQUEST_CARD_SOURCE
        ? process.env.ZOEN_REQUEST_CARD_SOURCE
        : file,
      'utf8',
    ),
    {
      filename: file.pathname,
      babelrc: false,
      configFile: false,
      presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
      plugins: [['babel-plugin-react-compiler', { target: '19' }]],
      caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
    },
  ).code as string;
const compiled = {
  review: compile(new URL('../agents/use-tool-review.ts', import.meta.url)),
  lease: compile(new URL('../agents/use-request-decision-lease.ts', import.meta.url)),
  policy: compile(new URL('../owner-policy/use-owner-policy.ts', import.meta.url)),
};
for (const source of Object.values(compiled)) assert.match(source, /react\/compiler-runtime/);
const presentation = {
  card: compile(new URL('../agents/request-card.tsx', import.meta.url)),
  recovery: compile(new URL('../agents/tool-review-recovery.tsx', import.meta.url)),
  action: compile(new URL('../agents/app-request-review-action.tsx', import.meta.url)),
};
assert.match(presentation.recovery, /react\/compiler-runtime/);
// RequestCard's existing try/finally fallback is checked against its baseline
// by tool-review-ui.fixture; this consumer still runs its actual Babel output.
/** Actual compiled hooks, sessions, ports and Store lifecycle. Only native/Core
 * boundaries are synthetic; no financial operation or external service is used. */
export async function reviewPolicyReactFixture(
  kind: 'review' | 'policy' | 'card',
  decisionFlow?: {
    confirm(): Promise<boolean>;
    submit?(): Promise<DecideOutcome>;
    resolved?(): void;
  },
) {
  const root = createReactTestRoot();
  const driver = new Driver(),
    snapshot = draftSession();
  const room = snapshot.conversations[0]!;
  const owner = room.members[0]!;
  const agent = {
    ...owner,
    id: 'agent',
    name: 'Agent',
    kind: 'Agent' as const,
    is_mine: true,
    is_me: false,
    owner_id: owner.id,
  };
  room.kind = 'Direct';
  room.members = [owner, agent];
  room.counterpart = agent;
  const target = { spaceId: room.id, agentId: agent.id, purpose: 'account' as const };
  // Expired synthetic tool request permits a real review ticket, never approval.
  const request: AgentRequestDto = {
    id: 'old-request',
    agent,
    status: 'Approved',
    space_id: room.id,
    space_title: 'Agent',
    title: 'Synthetic request',
    detail: '',
    audience: 'This chat',
    action_label: 'Review',
    cost_cents: null,
    reason: '',
    opened_ms: '1',
    resolved_ms: null,
    item_id: null,
    line_id: null,
    action_key: '',
    can_always_approve: false,
    by_standing: false,
    tool_approval: {
      content_hash: 'ab'.repeat(32),
      artifact_id: 'item',
      artifact_title: 'Synthetic Page',
      version: 1,
      operation: 'Replace block',
      expires_ms: '1',
      review: {
        state: 'Available',
        root_request_id: 'old-request',
        predecessor_request_id: null,
        successor_request_id: null,
      },
    },
  };
  if (decisionFlow)
    Object.assign(request, { status: 'Pending', tool_approval: null, can_always_approve: true });
  const calls: any[] = [];
  let requests: Promise<AgentRequestDto[]> | null = null;
  let inquiry: Promise<OwnerPolicyOptionsStatus> | null = null;
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'requests') {
      const pending = requests;
      requests = null;
      if (pending) return pending;
      const row = structuredClone(request);
      if (
        kind === 'card' &&
        row.tool_approval &&
        Number(row.tool_approval.expires_ms) <= Date.now()
      ) {
        row.status = 'Stale';
        row.tool_approval!.review!.state = 'Available';
      }
      return [row];
    }
    if (decisionFlow && (command.op === 'decide_request' || command.op === 'decide_tool_request'))
      return (
        decisionFlow.submit?.() ?? {
          request: { ...request, status: 'Approved' },
          message: 'Accepted decision',
          standing_grant_id: null,
          also_resolved: 0,
        }
      );
    if (command.op === 'find_account_policy_options') return null;
    if (command.op === 'request_account_policy_options') {
      const pending = inquiry;
      inquiry = null;
      if (pending) return pending;
      throw Error('Synthetic offline Core boundary');
    }
    throw Error(`Unexpected review/policy operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
  const buttons = new Map<string, Record<string, any>>();
  const texts = new Map<string, React.ReactNode>();
  const passthrough = ({ children }: { children?: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children);
  function NativeButton(props: Record<string, any>) {
    React.useLayoutEffect(() => {
      buttons.set(props.label, props);
      return () => {
        if (buttons.get(props.label) === props) buttons.delete(props.label);
      };
    }, [props]);
    return null;
  }
  function NativeText({ children }: { children?: React.ReactNode }) {
    const id = React.useId();
    React.useLayoutEffect(() => {
      texts.set(id, children);
      return () => {
        texts.delete(id);
      };
    }, [id, children]);
    return null;
  }
  let foreground = 'active',
    focused = true;
  const appListeners = new Set<(next: string) => void>();
  const focusListeners = { focus: new Set<() => void>(), blur: new Set<() => void>() };
  const navigation = {
    isFocused: () => focused,
    addListener(name: 'focus' | 'blur', callback: () => void) {
      focusListeners[name].add(callback);
      return () => focusListeners[name].delete(callback);
    },
  };
  const loaded = { exports: {} },
    focus = { exports: {} as { useFocusEffect: unknown } };
  runInNewContext(
    readFileSync(require.resolve('expo-router/build/link/useLoadedNavigation.js'), 'utf8'),
    {
      module: loaded,
      exports: loaded.exports,
      require(name: string) {
        if (name === 'react') return React;
        if (name === '../react-navigation/native')
          return {
            useNavigation: () => navigation,
            NavigationContainerRefContext: React.createContext({}),
          };
        throw Error(name);
      },
    },
  );
  runInNewContext(readFileSync(require.resolve('expo-router/build/useFocusEffect.js'), 'utf8'), {
    module: focus,
    exports: focus.exports,
    require(name: string) {
      if (name === 'react') return React;
      if (name === './link/useLoadedNavigation') return loaded.exports;
      if (name === './useNavigation') return { useNavigation: () => navigation };
      throw Error(name);
    },
    process: { env: { NODE_ENV: 'production' } },
  });
  const ports: Record<string, unknown> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      View: passthrough,
      Pressable: passthrough,
      Text: NativeText,
      AppState: {
        get currentState() {
          return foreground;
        },
        addEventListener(_: string, callback: (next: string) => void) {
          appListeners.add(callback);
          return { remove: () => appListeners.delete(callback) };
        },
      },
    },
    'expo-router': { useFocusEffect: focus.exports.useFocusEffect },
    '../documents/document-authority.ts': documentAuthority,
    '../core/provider': {
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../core/account-scope': ownership,
    '../core/account-scope.ts': ownership,
    '../core/tool-request-decision': decision,
    '../core/tool-review-renewal.ts': review,
    './core-port': policyCore,
    './session': policy,
    '../theme': {
      useTheme: () => ({
        colors: { text: '#111', secondary: '#555', danger: '#a00', accent: '#07f' },
      }),
    },
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en-US' },
    '../components/primitives': { Surface: passthrough, Avatar: () => null, NativeButton },
    '../components/dialogs': {
      useDialogs: () => ({
        confirm:
          decisionFlow?.confirm ?? (async () => assert.fail('Expiry cannot request consent')),
      }),
    },
    '../core/presentation': { money: () => '' },
    '../core/image-tool-review': { imageToolReviewValid },
    '../sites/source': siteSource,
    './drawing-tool-review': {
      DrawingToolReview: () => assert.fail('Read drawing has no mutation preview'),
    },
    './site-tool-review': {
      SiteToolReview: () => assert.fail('Drawing cannot load a Site preview'),
    },
  };
  const module = {
    exports: {} as {
      useToolReview: typeof useToolReview;
      useOwnerPolicy: typeof useOwnerPolicy;
    },
  };
  function load(source: string, target: { exports: Record<string, any> }) {
    runInNewContext(source, {
      setTimeout,
      clearTimeout,
      Date,
      module: target,
      exports: target.exports,
      require(name: string) {
        if (name.startsWith('@babel/runtime/helpers/')) return require(name);
        assert.ok(name in ports, name);
        return ports[name];
      },
    });
  }
  load(compiled[kind === 'card' ? 'review' : kind], module);
  const card = { exports: {} as Record<string, any> };
  if (kind === 'card') {
    const lease = { exports: {} };
    load(compiled.lease, lease);
    ports['./use-request-decision-lease'] = lease.exports;
    ports['../miniapps/app-request-review'] = appReview;
    ports['../i18n'] = ports['../i18n/index.ts'];
    const action = { exports: {} };
    load(presentation.action, action);
    ports['./app-request-review-action'] = action.exports;
    ports['./use-tool-review'] = module.exports;
    const recovery = { exports: {} };
    load(presentation.recovery, recovery);
    ports['./tool-review-recovery'] = recovery.exports;
    load(presentation.card, card);
  }
  let reviewController: ReturnType<typeof useToolReview> | null = null;
  let policyController: ReturnType<typeof useOwnerPolicy> | null = null;
  function ReviewSurface() {
    const value = module.exports.useToolReview(request.id);
    React.useLayoutEffect(() => {
      reviewController = value;
    }, [value]);
    return null;
  }
  function PolicySurface() {
    const value = module.exports.useOwnerPolicy(target);
    React.useLayoutEffect(() => {
      policyController = value;
    }, [value]);
    return null;
  }
  return {
    store,
    snapshot,
    calls,
    request,
    target,
    buttons,
    visibleText: () => Array.from(texts.values()).flat(Infinity).join(' '),
    errors: root.errors,
    review() {
      assert.ok(reviewController);
      return reviewController;
    },
    policy() {
      assert.ok(policyController);
      return policyController;
    },
    expectedPolicyScope: () =>
      policy.policyScopeKey(policyCore.policyAvailability(store, target).scope),
    render: () =>
      root.render(
        kind === 'card'
          ? React.createElement(card.exports.RequestCard, {
              request,
              embedded: true,
              onResolved: decisionFlow?.resolved,
            })
          : React.createElement(kind === 'review' ? ReviewSurface : PolicySurface),
      ),
    act: root.act,
    read(pending: Promise<AgentRequestDto[]>) {
      requests = pending;
    },
    inquire(pending: Promise<OwnerPolicyOptionsStatus>) {
      inquiry = pending;
    },
    async reopen(change?: 'identity' | 'device' | 'relay') {
      await root.act(async () => {
        await store.close();
        if (change === 'identity') {
          snapshot.account = { ...snapshot.account!, identity_id: 'e'.repeat(64) };
          owner.id = snapshot.account.identity_id;
          agent.owner_id = owner.id;
        }
        if (change === 'device')
          snapshot.account = { ...snapshot.account!, device_id: 'f'.repeat(64) };
        if (change === 'relay')
          snapshot.account = { ...snapshot.account!, relay_url: 'https://other-relay.invalid/' };
        await store.open('account');
      });
    },
    foreground(next: string) {
      foreground = next;
      for (const callback of appListeners) callback(next);
    },
    focus(next: boolean) {
      focused = next;
      for (const callback of focusListeners[next ? 'focus' : 'blur']) callback();
    },
    async close() {
      await root.close();
      await store.close();
    },
  };
}
