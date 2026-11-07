import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as React from 'react';
import { createRequire } from 'node:module';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { createReactTestRoot } from './react-test-root.fixtures.ts';

const require = createRequire(import.meta.url);
test('return control preserves its touch target, safe area and scoped failure while loading', async () => {
  const root = createReactTestRoot();
  let button: any = null;
  let position: any = null;
  let spinner = false;
  function Pressable(props: any) {
    React.useLayoutEffect(() => {
      button = props;
      return () => {
        button = null;
      };
    }, [props]);
    return props.children;
  }
  function View(props: any) {
    if (props.pointerEvents) position = props.style;
    return props.children;
  }
  const Control = loadEditorAgent('../components/conversation-return-button.tsx', {
    react: React,
    'react/compiler-runtime': require('react/compiler-runtime'),
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react-native': {
      Pressable,
      View,
      ActivityIndicator: () => {
        spinner = true;
        return null;
      },
    },
    'react-native-safe-area-context': { useSafeAreaInsets: () => ({ right: 12 }) },
    './navigation-glass': { NavigationGlass: ({ children }: any) => children },
    './primitives': { Icon: () => null },
    '../theme': { useTheme: () => ({ presentation: { navigation: { control: '#000' } } }) },
    '../i18n/index.ts': { t: (key: string) => key },
  }).ConversationReturnButton;
  const errors: unknown[] = [];
  const failure = new Error('offline');
  let active = true;
  const navigation = {
    canReturn: true,
    returning: false,
    current: () => active,
    restore: async () => {
      throw failure;
    },
  };
  const render = () =>
    root.render(
      React.createElement(Control, {
        navigation: { ...navigation },
        onError: (error: unknown) => errors.push(error),
      }),
    );
  try {
    await render();
    assert.equal(position.right, 22);
    assert.equal(position.bottom, '100%');
    assert.equal(button.style({ pressed: false }).width, 44);
    assert.equal(button.style({ pressed: false }).height, 44);
    assert.equal(button.accessibilityRole, 'button');
    assert.equal(button.accessibilityLabel, 'chat.returnToMessage');
    await root.act(() => button.onPress());
    assert.equal(errors[0], failure);
    active = false;
    await root.act(() => button.onPress());
    assert.equal(errors.length, 1);
    navigation.returning = true;
    await render();
    assert.equal(button.disabled, true);
    assert.equal(button.accessibilityState.busy, true);
    assert.equal(spinner, true);
    navigation.canReturn = false;
    await render();
    assert.equal(button, null);
    assert.deepEqual(root.errors, []);
  } finally {
    await root.close();
  }
});
