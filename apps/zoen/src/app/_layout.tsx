import { t } from '../i18n/index.ts';
import {
  ThemeProvider as NavigationThemeProvider,
  DarkTheme,
  DefaultTheme,
  Stack,
} from 'expo-router';
import { useEffect } from 'react';
import { View } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { KeyboardProvider } from 'react-native-keyboard-controller';
import { KeyboardMotionProvider } from '../components/keyboard-motion';
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
import { SkyBackground } from '../components/sky-background';
import { HomeProvider } from '../home/provider';
import { ConversationCardsProvider } from '../miniapps/conversation-provider';

// Cold links keep the real tabs beneath their destination, including on iOS.
export const unstable_settings = { anchor: '(tabs)' };

void SplashScreen.preventAutoHideAsync();
function Navigation() {
  const { scheme, colors, sky } = useTheme();
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
          background: 'transparent',
          card: colors.surface,
          text: colors.text,
          primary: colors.accent,
          border: colors.line,
        },
      }}
    >
      <StatusBar style={scheme === 'dark' ? 'light' : 'dark'} />
      <View style={{ flex: 1, backgroundColor: sky.bottom }}>
        <SkyBackground />
        <SessionGate>
          <NativeIntentConsumer />
          <Stack
            screenOptions={{
              headerBackButtonDisplayMode: 'minimal',
              headerShadowVisible: false,
              headerTransparent: process.env.EXPO_OS !== 'android',
              headerBlurEffect: 'none',
              headerStyle: { backgroundColor: 'transparent' },
              headerLargeStyle: { backgroundColor: 'transparent' },
              headerLargeTitleShadowVisible: false,
              headerTitleStyle: { color: colors.text },
              headerLargeTitleStyle: { color: colors.text },
              scrollEdgeEffects: { top: 'hidden', bottom: 'hidden' },
              headerTintColor: colors.text,
              contentStyle: { backgroundColor: 'transparent' },
            }}
          >
            <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
            <Stack.Screen name="chat/[id]" options={{ title: '' }} />
            <Stack.Screen
              name="miniapp"
              options={{
                presentation: process.env.EXPO_OS === 'web' ? 'card' : 'fullScreenModal',
                animation: process.env.EXPO_OS === 'web' ? undefined : 'slide_from_bottom',
                gestureEnabled: true,
                headerShown: false,
              }}
            />
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
      </View>
    </NavigationThemeProvider>
  );
}
export default function RootLayout() {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <KeyboardProvider>
          <KeyboardMotionProvider>
            <ThemeProvider>
              <DialogsProvider>
                <ClientProvider>
                  <HomeProvider>
                    <ConversationCardsProvider>
                      <NativeSurfacesProvider>
                        <NotificationProvider>
                          <Navigation />
                        </NotificationProvider>
                      </NativeSurfacesProvider>
                    </ConversationCardsProvider>
                  </HomeProvider>
                </ClientProvider>
              </DialogsProvider>
            </ThemeProvider>
          </KeyboardMotionProvider>
        </KeyboardProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
