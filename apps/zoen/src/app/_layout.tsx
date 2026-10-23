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
import { NativeCallsProvider } from '../calls/provider';
import { CallReturnLayout } from '../calls/return-layout';
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
import { MessageContextSession } from '../components/message-context-session';

// Cold links keep the real tabs beneath their destination, including on iOS.
export const unstable_settings = { anchor: '(tabs)' };

void SplashScreen.preventAutoHideAsync();
function Navigation() {
  const { scheme, colors, sky, presentation: p, appearanceReady } = useTheme();
  useEffect(() => {
    if (appearanceReady) void SplashScreen.hideAsync();
  }, [appearanceReady]);
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
      {process.env.EXPO_OS !== 'ios' && <StatusBar style={scheme === 'dark' ? 'light' : 'dark'} />}
      <View style={{ flex: 1, backgroundColor: sky.bottom }}>
        <SkyBackground />
        <CallReturnLayout>
          <MessageContextSession>
            <SessionGate>
              <NativeIntentConsumer />
              <Stack
                screenOptions={({ route }) => ({
                  // Tab surfaces own their headings; never restore a native group header.
                  headerShown: route.name !== '(tabs)',
                  headerBackButtonDisplayMode: 'minimal',
                  headerShadowVisible: false,
                  headerTransparent: process.env.EXPO_OS !== 'android',
                  headerBlurEffect: 'none',
                  headerStyle: { backgroundColor: 'transparent' },
                  headerLargeStyle: { backgroundColor: 'transparent' },
                  headerLargeTitleShadowVisible: false,
                  headerTitleStyle: { ...p.typography.title, color: p.navigation.text },
                  headerLargeTitleStyle: { ...p.typography.largeTitle, color: p.navigation.text },
                  scrollEdgeEffects: { top: 'hidden', bottom: 'hidden' },
                  headerTintColor: p.navigation.control,
                  contentStyle: { backgroundColor: 'transparent' },
                  ...(process.env.EXPO_OS === 'ios'
                    ? {
                        statusBarStyle: scheme === 'dark' ? 'light' : 'dark',
                        statusBarAnimation: 'none',
                      }
                    : {}),
                })}
              >
                <Stack.Screen name="(tabs)" options={{ title: '' }} />
                <Stack.Screen name="chat/[id]" options={{ title: '' }} />
                <Stack.Screen
                  name="chat-attachment/[id]"
                  options={{
                    // All attachment categories share this presentation from their
                    // first frame; the child retains its capture/dismissal guard.
                    presentation: process.env.EXPO_OS === 'web' ? 'modal' : 'formSheet',
                    title: t('media.attachmentAdd'),
                    headerTransparent: false,
                    sheetAllowedDetents: [0.65, 1],
                    sheetGrabberVisible: true,
                    sheetExpandsWhenScrolledToEdge: true,
                    gestureEnabled: true,
                  }}
                />
                <Stack.Screen
                  name="miniapps"
                  options={{
                    presentation: process.env.EXPO_OS === 'ios' ? 'fullScreenModal' : 'card',
                    headerShown: false,
                  }}
                />
                <Stack.Screen
                  name="miniapp"
                  options={{
                    presentation: process.env.EXPO_OS === 'web' ? 'card' : 'fullScreenModal',
                    orientation: 'default',
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
          </MessageContextSession>
        </CallReturnLayout>
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
                  <NativeCallsProvider>
                    <HomeProvider>
                      <ConversationCardsProvider>
                        <NativeSurfacesProvider>
                          <NotificationProvider>
                            <Navigation />
                          </NotificationProvider>
                        </NativeSurfacesProvider>
                      </ConversationCardsProvider>
                    </HomeProvider>
                  </NativeCallsProvider>
                </ClientProvider>
              </DialogsProvider>
            </ThemeProvider>
          </KeyboardMotionProvider>
        </KeyboardProvider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}
