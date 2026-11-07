import * as React from 'react';
import { createRequire } from 'node:module';
import { memberHookReactFixture } from './community-members-hook-react.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import * as ownership from './account-scope.ts';
import * as actions from '../communities/space-invite-actions.ts';
import * as dates from './message-info-presentation.ts';
import type { InviteDto } from './generated/InviteDto';
import type { useSpaceInvite } from '../communities/use-space-invite';
const require = createRequire(import.meta.url);
compileExpoSource(new URL('../communities/use-space-invite.ts', import.meta.url));
compileExpoSource(new URL('../communities/space-invite-action.tsx', import.meta.url));

/** Actual optimized sheet/controller/member lease and Store; native UI and RPC are ports. */
export async function spaceInviteFixture() {
  const f = await memberHookReactFixture();
  const requests: any[] = [],
    shares: string[] = [],
    copies: string[] = [];
  const controls = new Map<string, any>();
  let create: () => Promise<InviteDto> = async () => ({
    code: 'ABCD234567',
    link: 'https://zoen.invalid/j/ABCD234567',
    expires_at_ms: '1800000000000',
  });
  let permission = true;
  const operation = f.driver.operation;
  f.driver.operation = async (command) => {
    if (command.op === 'create_invite_reviewed') {
      requests.push(structuredClone(command));
      return create();
    }
    const value = await operation(command);
    if (command.op === 'space_member_context')
      return { ...(value as any), role: 'owner', can_manage_members: permission };
    return value;
  };
  const common: Record<string, any> = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    '../core/provider': {
      useClient: () => ({
        store: f.store,
        ...React.useSyncExternalStore(f.store.subscribe, f.store.getSnapshot, f.store.getSnapshot),
      }),
    },
    '../core/account-scope': ownership,
    './use-member-actions': { useMemberActions: f.useMemberActions },
    './space-invite-actions': actions,
  };
  const hook = loadEditorAgent('../communities/use-space-invite.ts', common).useSpaceInvite;
  let current: ReturnType<typeof useSpaceInvite> | null = null;
  const useCapture = (id: string, props: any) => {
    React.useLayoutEffect(() => {
      controls.set(id, props);
      return () => {
        if (controls.get(id) === props) controls.delete(id);
      };
    }, [id, props]);
  };
  function View(props: any) {
    return props.children;
  }
  function Button(props: any) {
    useCapture(props.testID ?? props.label, props);
    return null;
  }
  function Sheet(props: any) {
    useCapture('sheet', props);
    return props.isPresented ? props.children : null;
  }
  function Picker(props: any) {
    useCapture(props.testID, props);
    return null;
  }
  Picker.Item = () => null;
  const component = loadEditorAgent('../communities/space-invite-action.tsx', {
    ...common,
    '@expo/ui': {
      Host: View,
      Column: View,
      Row: View,
      Text: () => null,
      RNHostView: View,
      BottomSheet: Sheet,
      Picker,
    },
    'react-native': {
      View,
      ScrollView: View,
      Text: () => null,
      Share: {
        share: async ({ message }: any) => {
          shares.push(message);
        },
      },
    },
    'expo-clipboard': {
      setStringAsync: async (link: string) => {
        copies.push(link);
      },
    },
    '../components/primitives': { NativeButton: Button, IconButton: Button, Loading: () => null },
    '../profile/profile-section': {
      ProfileInfoRow: (props: any) => {
        useCapture('row', props);
        return null;
      },
    },
    '../core/message-info-presentation': dates,
    '../i18n/index.ts': { t: (key: string) => key, formatLocale: 'en' },
    '../theme': {
      useTheme: () => ({
        scheme: 'light',
        presentation: {
          list: {
            accent: '#007aff',
            background: '#fff',
            text: '#111',
            secondary: '#666',
            destructive: '#f00',
          },
        },
      }),
    },
    './use-space-invite': {
      useSpaceInvite: (space: string) => {
        const value = hook(space);
        React.useLayoutEffect(() => {
          current = value;
        }, [value]);
        return value;
      },
    },
  }).SpaceInviteAction;
  return {
    ...f,
    requests,
    shares,
    copies,
    controls,
    render: () =>
      f.renderContent(React.createElement(component, { space: f.snapshot.conversations[0]!.id })),
    current: () => {
      if (!current) throw Error('Missing invite controller');
      return current;
    },
    create: (next: typeof create) => {
      create = next;
    },
    permission: (next: boolean) => {
      permission = next;
    },
    press: async (id: string) => {
      const control = controls.get(id);
      if (!control || control.disabled) throw Error(`Unavailable ${id}`);
      await f.act(() => control.onPress());
    },
  };
}
