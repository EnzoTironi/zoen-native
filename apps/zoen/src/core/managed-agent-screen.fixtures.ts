import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { runInNewContext } from 'node:vm';
import * as React from 'react';
import type { useManagedAgent } from '../managed-agent/use-managed-agent';
import type { CreateAgentScreen } from '../managed-agent/create-agent-screen';
import { compileExpoSource } from './expo-compiler.fixtures.ts';
const require = createRequire(import.meta.url);
const code = compileExpoSource(
  new URL('../managed-agent/create-agent-screen.tsx', import.meta.url),
);
type Button = { testID?: string; label: string; disabled?: boolean; onPress(): void };
type Field = { testID: string; value: string; onChangeText(value: string): void };

/** Only native presentation is replaced; the optimized screen and its callbacks are real. */
export function managedAgentScreen(useHook: typeof useManagedAgent, back: () => void) {
  const buttons = new Map<string, Button>(),
    fields = new Map<string, Field>();
  const module = { exports: {} as { CreateAgentScreen: typeof CreateAgentScreen } };
  const children = ({ children }: { children: React.ReactNode }) => children;
  runInNewContext(code, {
    module,
    exports: module.exports,
    process: { env: { EXPO_OS: 'ios' } },
    require: (name: string) => {
      if (name === 'react') return React;
      if (
        name === 'react/compiler-runtime' ||
        name === 'react/jsx-runtime' ||
        name.startsWith('@babel/runtime/')
      )
        return require(name);
      if (name === 'expo-router') return { router: { back } };
      if (name === 'react-native')
        return { Keyboard: { dismiss() {} }, View: children, Text: () => null };
      if (name === '../theme') return { useTheme: () => ({ presentation: {} }) };
      if (name === '../i18n') return { t: (key: string) => key };
      if (name === './use-managed-agent') return { useManagedAgent: useHook };
      if (name === '../components/settings-screen') return { SettingsScreen: children };
      if (name === '../profile/profile-header') return { ProfileHeader: () => null };
      if (name === '../profile/profile-section')
        return { ProfileSection: children, ProfileInfoRow: () => null };
      if (name === '../components/primitives')
        return {
          Loading: () => null,
          NativeButton: (props: Button) => {
            React.useLayoutEffect(() => {
              const key = props.testID ?? props.label;
              buttons.set(key, props);
              return () => {
                buttons.delete(key);
              };
            }, [props]);
            return null;
          },
          Field: (props: Field) => {
            React.useLayoutEffect(() => {
              fields.set(props.testID, props);
              return () => {
                fields.delete(props.testID);
              };
            }, [props]);
            return null;
          },
        };
      throw Error(name);
    },
  });
  return {
    Screen: module.exports.CreateAgentScreen,
    buttons,
    type(value: string) {
      const field = fields.get('managed-agent-name');
      assert.ok(field, 'The name field must be visible');
      field.onChangeText(value);
    },
    press(key: string) {
      const button = buttons.get(key);
      assert.ok(button, `Expected visible button ${key}`);
      assert.equal(!!button.disabled, false, `${key} must be enabled`);
      button.onPress();
    },
  };
}
