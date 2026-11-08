import * as React from 'react';
import { createRequire } from 'node:module';
import { memberHookReactFixture } from './community-members-hook-react.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
import * as scopes from './account-scope.ts';
import * as sessions from '../backup/session.ts';
import type { useBackup } from '../backup/use-backup';
import type { BackupStatusDto } from './generated/BackupStatusDto';
const require = createRequire(import.meta.url);
compileExpoSource(new URL('../backup/use-backup.ts', import.meta.url));
compileExpoSource(new URL('../app/backup.tsx', import.meta.url));
export async function backupReactFixture() {
  const f = await memberHookReactFixture();
  const controls = new Map<string, any>(),
    texts = new Map<object, unknown>();
  const requests: any[] = [],
    copies: string[] = [];
  let status: BackupStatusDto = {
    enabled: false,
    mode: '',
    last_backup_ms: '0',
    bytes: '0',
    in_progress: false,
  };
  const key = '1234 '.repeat(15) + '1234';
  let operation: (command: any) => Promise<unknown> = async (command) => {
    if (command.op === 'backup_status') return structuredClone(status);
    if (command.op === 'backup_create_recovery_key') return key;
    if (command.op === 'backup_turn_off') {
      status = { ...status, enabled: false };
      return null;
    }
    status = {
      ...status,
      enabled: true,
      mode: command.op === 'backup_password' ? 'password' : 'recovery_key',
      last_backup_ms: '1000',
    };
    return structuredClone(status);
  };
  const original = f.driver.operation;
  f.driver.operation = async (command) => {
    if (command.op.startsWith('backup_')) {
      requests.push(structuredClone(command));
      return operation(command);
    }
    return original(command);
  };
  let confirm: () => Promise<boolean> = async () => true;
  let copy: (value: string) => Promise<boolean> = async (value) => {
    copies.push(value);
    return true;
  };
  const appListeners = new Set<(value: string) => void>();
  const app = {
    currentState: 'active',
    addEventListener: (_event: string, callback: (value: string) => void) => {
      appListeners.add(callback);
      return { remove: () => appListeners.delete(callback) };
    },
  };
  let focused = true;
  const focusListeners = new Set<() => void>();
  const useFocusEffect = (callback: () => void | (() => void)) => {
    const focus = React.useSyncExternalStore(
      (listener) => {
        focusListeners.add(listener);
        return () => {
          focusListeners.delete(listener);
        };
      },
      () => focused,
      () => focused,
    );
    React.useEffect(() => (focus ? callback() : undefined), [focus, callback]);
  };
  let guard: { blocked: boolean; callback: (value: any) => void } | null = null;
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
  function Text(props: any) {
    const id = React.useMemo(() => ({}), []);
    React.useLayoutEffect(() => {
      texts.set(id, props.children);
      return () => {
        texts.delete(id);
      };
    }, [id, props.children]);
    return null;
  }
  function Button(props: any) {
    useCapture(props.testID ?? props.label, props);
    return null;
  }
  function Check(props: any) {
    useCapture(props.testID, props);
    return props.children;
  }
  const common = {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': { AppState: app, ScrollView: View, View, Text, Pressable: Check },
    'expo-router': {
      useFocusEffect,
      Stack: { Screen: () => null },
      usePreventRemove: (blocked: boolean, callback: any) => {
        React.useLayoutEffect(() => {
          guard = { blocked, callback };
        }, [blocked, callback]);
      },
    },
    '../core/provider': {
      useClient: () => ({
        store: f.store,
        ...React.useSyncExternalStore(f.store.subscribe, f.store.getSnapshot, f.store.getSnapshot),
      }),
    },
    '../core/account-scope': scopes,
    './session': sessions,
    'expo-clipboard': { setStringAsync: (value: string) => copy(value) },
    '../theme': {
      useTheme: () => ({
        colors: { text: '#111', secondary: '#666', danger: '#f00', accent: '#00f' },
      }),
    },
    '../i18n': { t: (value: string) => value, formatLocale: 'en' },
    '../components/dialogs': {
      useDialogs: () => ({ confirm: () => confirm(), inform: async () => {} }),
    },
    '../components/primitives': {
      NativeButton: Button,
      Field: Button,
      Surface: View,
      EmptyState: Button,
    },
  };
  const hook = loadEditorAgent('../backup/use-backup.ts', common).useBackup;
  let controller: ReturnType<typeof useBackup> | null = null;
  const Component = loadEditorAgent('../app/backup.tsx', {
    ...common,
    '../backup/use-backup': {
      useBackup: () => {
        const value = hook();
        React.useLayoutEffect(() => {
          controller = value;
        }, [value]);
        return value;
      },
    },
  }).default;
  return {
    ...f,
    requests,
    copies,
    controls,
    key,
    renderBackup: () => f.renderContent(React.createElement(Component)),
    current: () => {
      if (!controller) throw Error('Backup controller missing');
      return controller;
    },
    text: () => [...texts.values()].flat().join(' '),
    operation: (next: typeof operation) => {
      operation = next;
    },
    status: (next: Partial<BackupStatusDto>) => {
      status = { ...status, ...next };
    },
    confirm: (next: typeof confirm) => {
      confirm = next;
    },
    copy: (next: typeof copy) => {
      copy = next;
    },
    guard: () => guard,
    press: async (id: string) => {
      const value = controls.get(id);
      if (!value || value.disabled) throw Error(`Unavailable ${id}`);
      await f.act(() => value.onPress());
    },
    input: (id: string, value: string) => f.act(() => controls.get(id).onChangeText(value)),
    app: (value: string) =>
      f.act(() => {
        app.currentState = value;
        for (const callback of appListeners) callback(value);
      }),
    focusBackup: (value: boolean) =>
      f.act(() => {
        focused = value;
        for (const callback of focusListeners) callback();
      }),
  };
}
