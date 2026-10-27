import * as React from 'react';
import { createRequire } from 'node:module';
import { createReactTestRoot } from './react-test-root.fixtures.ts';
import { loadEditorAgent } from './editor-agent-compile.fixtures.ts';
import { telegramPresentation } from '../theme/telegram.ts';

type Props = Record<string, any>;
const require = createRequire(import.meta.url);

/** Actual Expo/Compiler layouts; native-stack rendering and app services are ports.
 * This inspects the options handed to native-stack, not UIKit rotation/pixels. */
export async function appOrientationLayout(app = false) {
  const root = createReactTestRoot();
  let captured: Props | null = null;
  function Stack(props: Props) {
    React.useLayoutEffect(() => {
      captured = props;
      return () => {
        captured = null;
      };
    }, [props]);
    return null;
  }
  Stack.Screen = () => null;
  const children = ({ children }: Props) => children;
  const theme = {
    scheme: 'dark',
    colors: {},
    sky: { bottom: '#000' },
    presentation: telegramPresentation('night'),
    appearanceReady: true,
  };
  const ports: Record<string, any> = {
    react: React,
    'react/jsx-runtime': require('react/jsx-runtime'),
    'react/compiler-runtime': require('react/compiler-runtime'),
    'expo-router': {
      Stack,
      ThemeProvider: children,
      DarkTheme: { colors: {} },
      DefaultTheme: { colors: {} },
    },
    'react-native': { View: children },
    'react-native-gesture-handler': { GestureHandlerRootView: children },
    'react-native-keyboard-controller': { KeyboardProvider: children },
    'react-native-safe-area-context': { SafeAreaProvider: children },
    'expo-splash-screen': { preventAutoHideAsync: async () => {}, hideAsync: async () => {} },
    'expo-status-bar': { StatusBar: () => null },
    '../i18n/index.ts': { t: (key: string) => key },
    '../components/keyboard-motion': { KeyboardMotionProvider: children },
    '../calls/provider': { NativeCallsProvider: children },
    '../calls/return-layout': { CallReturnLayout: children },
    '../core/provider': { ClientProvider: children },
    '../theme': { ThemeProvider: children, useTheme: () => theme },
    '../../theme': { useTheme: () => theme },
    '../components/session-gate': { SessionGate: children },
    '../components/dialogs': { DialogsProvider: children },
    '../notifications/provider': { NotificationProvider: children },
    '../native-surfaces/provider': { NativeSurfacesProvider: children },
    '../native-surfaces/intent-consumer': { NativeIntentConsumer: () => null },
    '../components/sky-background': { SkyBackground: () => null },
    '../../components/sky-background': { SkyScene: children },
    '../../miniapps/app-sheet-chrome': { AppSheetChrome: children },
    '../home/provider': { HomeProvider: children },
    '../miniapps/conversation-provider': { ConversationCardsProvider: children },
    '../components/message-context-session': { MessageContextSession: children },
  };
  const Layout = loadEditorAgent(
    app ? '../app/miniapp/_layout.tsx' : '../app/_layout.tsx',
    ports,
  ).default;
  await root.render(React.createElement(Layout));
  if (!captured || root.errors.length) {
    await root.close();
    throw root.errors[0] ?? Error('Actual layout did not provide its Stack options');
  }
  const stack = captured as Props;
  const screen = (name: string) =>
    React.Children.toArray(stack.children).find(
      (child) => React.isValidElement<Props>(child) && child.props.name === name,
    ) as React.ReactElement<Props> | undefined;
  const options = (name: string) => ({
    ...(typeof stack.screenOptions === 'function'
      ? stack.screenOptions({ route: { name } })
      : stack.screenOptions),
    ...screen(name)?.props.options,
  });
  await root.close();
  return { options, errors: root.errors };
}
