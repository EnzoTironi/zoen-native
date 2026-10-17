import { t } from '../i18n/index.ts';
import {
  ThemeProvider as NavigationThemeProvider,
  DarkTheme,
  DefaultTheme,
  Stack,
} from 'expo-router';
import { useEffect } from 'react';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { KeyboardProvider } from 'react-native-keyboard-controller';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import * as SplashScreen from 'expo-splash-screen';
import { StatusBar } from 'expo-status-bar';
import { ClientProvider } from '../core/provider';
import { ThemeProvider, useTheme } from '../theme';
import { SessionGate } from '../components/session-gate';
import { DialogsProvider } from '../components/dialogs';
import { NotificationProvider } from '../notifications/provider';
import { NativeSurfacesProvider } from '../native-surfaces/provider';
import { NativeIntentConsumer } from '../native-surfaces/intent-consumer';

// Cold links keep the real tabs beneath their destination, including on iOS.
export const unstable_settings = { anchor: '(tabs)' };

void SplashScreen.preventAutoHideAsync();
function Navigation() {
  const { scheme, colors } = useTheme();
  useEffect(() => {
    void SplashScreen.hideAsync();
  }, []);
  const base = scheme === 'dark' ? DarkTheme : DefaultTheme;
  return (
    <NavigationThemeProvider
      value={{
        ...base,
        colors: {
          ...base.colors,
          background: colors.background,
          card: colors.surface,
          text: colors.text,
          primary: colors.accent,
          border: colors.line,
        },
      }}
    >
      <StatusBar style={scheme === 'dark' ? 'light' : 'dark'} />
      <SessionGate>
        <NativeIntentConsumer />
        <Stack
          screenOptions={{
            headerBackButtonDisplayMode: 'minimal',
            headerShadowVisible: false,
            contentStyle: { backgroundColor: colors.background },
          }}
        >
          <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
          <Stack.Screen name="chat/[id]" options={{ title: '' }} />
          <Stack.Screen
            name="new-chat"
            options={{
              title: t('common.newChat'),
              presentation: 'formSheet',
              sheetGrabberVisible: true,
            }}
          />
          <Stack.Screen name="item/[id]" options={{ title: t('common.details') }} />
        </Stack>
      </SessionGate>
    </NavigationThemeProvider>
  );
}
export default function RootLayout() {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <KeyboardProvider>
          <ThemeProvider>
            <DialogsProvider>
              <ClientProvider>
                <NativeSurfacesProvider>
                  <NotificationProvider>
                    <Navigation />
                  </NotificationProvider>
                </NativeSurfacesProvider>
              </ClientProvider>
            </DialogsProvider>
          </ThemeProvider>
        </KeyboardProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
