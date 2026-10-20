import { t } from '../i18n/index.ts';
import { forwardRef, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Platform, Text, View } from 'react-native';
import { WebView } from 'react-native-webview';
import { randomUUID } from 'expo-crypto';
import {
  deckcraftFocusQaEnabled,
  deckcraftFocusQaLabel,
  deckcraftFocusQaScript,
  decodeDeckcraftFocusQa,
  emptyDeckcraftFocusQa,
  type DeckcraftFocusQaSlots,
} from './deckcraft-focus-qa';
import type { MiniappFrameHandle, MiniappFrameProps } from './frame-types';
import {
  nativeMiniappCommand,
  nativeMiniappHtml,
  nativeMiniappMessage,
  nativeMiniappMessageSource,
  nativeMiniappNavigation,
} from './host-transport';

export const MiniappFrame = forwardRef<MiniappFrameHandle, MiniappFrameProps>(function MiniappFrame(
  { html, onMessage, onError, height, title, blocked, fill = false, runtimePackage },
  ref,
) {
  const view = useRef<WebView<object>>(null);
  const focusQa = deckcraftFocusQaEnabled(
    Platform.OS,
    process.env.EXPO_PUBLIC_ZOEN_DECKCRAFT_FOCUS_QA,
    runtimePackage,
  );
  const document = useMemo(() => {
    const nonce = randomUUID();
    const qaSession = focusQa ? randomUUID() : null;
    return {
      nonce,
      qaSession,
      qaScript: qaSession ? deckcraftFocusQaScript(qaSession) : undefined,
      html: nativeMiniappHtml(html, 'Zoen', nonce, runtimePackage),
    };
  }, [html, runtimePackage, focusQa]);
  const [qaState, setQaState] = useState<{
    owner: typeof document;
    slots: DeckcraftFocusQaSlots;
  }>(() => ({ owner: document, slots: emptyDeckcraftFocusQa() }));
  // Layout/theme updates must not submit a fresh native source and reload the App.
  const source = useMemo(() => ({ html: document.html }), [document]);
  const active = useRef<typeof document | null>(document);
  useLayoutEffect(() => {
    active.current = document;
    const target = view.current;
    return () => {
      if (active.current !== document) return;
      active.current = null;
      target?.injectJavaScript(nativeMiniappCommand(document.nonce, 'dispose'));
    };
  }, [document]);
  useLayoutEffect(() => {
    if (active.current === document)
      view.current?.injectJavaScript(nativeMiniappCommand(document.nonce, 'title', title));
  }, [document, title]);
  useImperativeHandle(
    ref,
    () => ({
      dispose: () => {
        if (active.current !== document) return;
        active.current = null;
        view.current?.stopLoading();
        view.current?.injectJavaScript(nativeMiniappCommand(document.nonce, 'dispose'));
      },
      send: (message) => {
        if (active.current === document)
          view.current?.injectJavaScript(nativeMiniappCommand(document.nonce, 'send', message));
      },
    }),
    [document],
  );
  return (
    <View
      pointerEvents={blocked ? 'none' : 'auto'}
      accessibilityElementsHidden={blocked}
      testID="miniapp-native-frame"
      style={fill ? { flex: 1, minHeight: 0 } : { height, minHeight: 400 }}
    >
      <WebView<object>
        ref={view}
        source={source}
        injectedJavaScriptBeforeContentLoaded={document.qaScript}
        injectedJavaScriptBeforeContentLoadedForMainFrameOnly={
          document.qaScript ? false : undefined
        }
        originWhitelist={['*']}
        incognito
        sharedCookiesEnabled={false}
        thirdPartyCookiesEnabled={false}
        cacheEnabled={false}
        domStorageEnabled={false}
        javaScriptCanOpenWindowsAutomatically={false}
        allowFileAccess={false}
        allowFileAccessFromFileURLs={false}
        allowUniversalAccessFromFileURLs={false}
        geolocationEnabled={false}
        mediaCapturePermissionGrantType="deny"
        mediaPlaybackRequiresUserAction
        allowsInlineMediaPlayback={false}
        allowsAirPlayForMediaPlayback={false}
        allowsBackForwardNavigationGestures={false}
        setSupportMultipleWindows={false}
        webviewDebuggingEnabled={false}
        automaticallyAdjustContentInsets={false}
        contentInsetAdjustmentBehavior="never"
        bounces={false}
        accessibilityLabel={title}
        onShouldStartLoadWithRequest={nativeMiniappNavigation}
        onOpenWindow={() => {}}
        onLoadEnd={() => {
          if (active.current === document)
            view.current?.injectJavaScript(nativeMiniappCommand(document.nonce, 'title', title));
        }}
        onMessage={(event) => {
          if (active.current !== document) return;
          if (focusQa && document.qaSession) {
            const snapshot = decodeDeckcraftFocusQa(event.nativeEvent.data, document.qaSession);
            if (snapshot) {
              setQaState((prior) => ({
                owner: document,
                slots: {
                  ...(prior.owner === document ? prior.slots : emptyDeckcraftFocusQa()),
                  [snapshot.slot]: snapshot,
                },
              }));
              return;
            }
          }
          if (!nativeMiniappMessageSource(event.nativeEvent.url, Platform.OS)) return;
          const message = nativeMiniappMessage(event.nativeEvent.data, document.nonce);
          if (message !== null) onMessage(message);
        }}
        onError={(event) => onError(event.nativeEvent.description)}
        onContentProcessDidTerminate={() =>
          onError(t('miniapps.theSystemClosedThisInterfaceRefreshToOpen'))
        }
        style={{ backgroundColor: 'transparent' }}
      />
      {focusQa ? (
        <Text
          testID="deckcraft-focus-qa"
          accessible
          accessibilityLabel={deckcraftFocusQaLabel(
            qaState.owner === document ? qaState.slots : emptyDeckcraftFocusQa(),
          )}
          pointerEvents="none"
          style={{
            position: 'absolute',
            top: 0,
            left: 0,
            width: 1,
            height: 1,
            fontSize: 1,
            color: 'transparent',
            overflow: 'hidden',
          }}
        >
          {deckcraftFocusQaLabel(
            qaState.owner === document ? qaState.slots : emptyDeckcraftFocusQa(),
          )}
        </Text>
      ) : null}
    </View>
  );
});
