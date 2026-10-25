import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { ClientStore } from './store.ts';
import { Driver, draftSession } from './store.fixtures.ts';
import { context, peer } from './community-members.fixtures.ts';
import * as flow from '../communities/member-actions.ts';
import * as addition from '../communities/member-addition.ts';
import * as port from '../communities/member-context-port.ts';
import * as ownership from './account-scope.ts';
import type { SpaceMemberContext } from './generated/SpaceMemberContext';

const require = createRequire(import.meta.url);
const { transformSync } = require('@babel/core');
const file = new URL('../communities/use-member-actions.ts', import.meta.url);
const compiled = transformSync(readFileSync(file, 'utf8'), {
  filename: file.pathname,
  babelrc: false,
  configFile: false,
  presets: [['babel-preset-expo', { jsxRuntime: 'automatic' }]],
  plugins: [['babel-plugin-react-compiler', { target: '19' }]],
  caller: { name: 'metro', platform: 'ios', isDev: false, supportsStaticESM: false },
}).code as string;
type Controller = {
  session: flow.MemberActionsSession;
  addition: addition.MemberAdditionSession;
  additionState: addition.MemberAdditionState;
  available: boolean;
  state: flow.MemberActionsState;
};

/** Same real React + installed Expo focus setup as settings-route-react, for the
 * actual compiled member hook/port/session. Store retirement is real, not simulated. */
export async function memberHookReactFixture() {
  const root = createReactTestRoot();
  const driver = new Driver(),
    snapshot = draftSession();
  const room = snapshot.conversations[0]!;
  room.kind = 'Community';
  room.members.push(context().member!);
  const calls: any[] = [];
  let nextRead: Promise<SpaceMemberContext> | null = null;
  let candidateState: SpaceMemberContext | null = null;
  const value = () => ({
    ...context(),
    space_id: room.id,
    identity_id: snapshot.account!.identity_id,
    device_id: snapshot.account!.device_id,
  });
  driver.opening = async () => structuredClone(snapshot);
  driver.operation = async (command) => {
    calls.push(structuredClone(command));
    if (command.op === 'snapshot') return structuredClone(snapshot);
    if (command.op === 'space_member_context') {
      if (command.identity_id === snapshot.account!.identity_id)
        return {
          ...value(),
          member_id: command.identity_id,
          member: { ...value().member!, id: command.identity_id },
        };
      const pending = nextRead;
      nextRead = null;
      return pending ?? candidateState ?? value();
    }
    if (command.op === 'find_member_candidates')
      return candidateState
        ? [candidateState]
        : [
            {
              ...value(),
              state: 'Removed',
              can_add: true,
              delivery_ready: false,
              member: null,
              role: null,
              candidate: value().member,
            },
          ];
    if (command.op === 'add_member_reviewed') {
      candidateState = { ...value(), can_add: false, delivery_ready: false, state: 'Updating' };
      return candidateState;
    }
    if (command.op === 'remove_member_reviewed')
      return { ...value(), state: 'Removing', can_remove: false };
    throw Error(`Unexpected member operation ${command.op}`);
  };
  const store = new ClientStore(driver);
  await store.open('account');
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
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react-native': {
      Platform: { OS: 'ios' },
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
    '../core/provider': {
      // The production provider's same subscription contract; no hook flattening.
      useClient: () => ({
        store,
        ...React.useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot),
      }),
    },
    '../core/account-scope': ownership,
    './member-actions': flow,
    './member-addition': addition,
    './member-context-port': port,
  };
  const module = { exports: {} as { useMemberActions(space: string): Controller } };
  runInNewContext(compiled, {
    module,
    exports: module.exports,
    require(name: string) {
      if (name.startsWith('@babel/runtime/helpers/')) return require(name);
      assert.ok(name in ports, name);
      return ports[name];
    },
  });
  let controller: Controller | null = null;
  function Surface() {
    const value = module.exports.useMemberActions(room.id);
    React.useLayoutEffect(() => {
      controller = value;
    }, [value]);
    return null;
  }
  return {
    store,
    snapshot,
    calls,
    errors: root.errors,
    current() {
      assert.ok(controller);
      return controller;
    },
    expectedScope: () => ownership.accountScopeKey(ownership.accountScope(store)),
    render: () => root.render(React.createElement(Surface)),
    act: root.act,
    open: () =>
      root.act(() => {
        controller?.session.open(peer);
      }),
    value,
    memberState(next: Partial<SpaceMemberContext>) {
      candidateState = { ...value(), ...next };
    },
    canonicalChange() {
      driver.changed?.({ kind: 'change', spaceIds: [room.id] });
    },
    read(pending: Promise<SpaceMemberContext>) {
      nextRead = pending;
    },
    async reopen(change?: 'identity' | 'device' | 'relay') {
      await root.act(async () => {
        await store.close();
        if (change === 'identity')
          snapshot.account = { ...snapshot.account!, identity_id: 'e'.repeat(64) };
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
