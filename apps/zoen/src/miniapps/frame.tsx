import { t } from '../i18n/index.ts';
import { forwardRef, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { Platform, Text, View } from 'react-native';
import { WebView } from 'react-native-webview';
import { randomUUID } from 'expo-crypto';
import { DECKCRAFT_ENGINE_REVISION } from './canonical-binary';
import {
  deckcraftInputOrderQaEnabled,
  deckcraftInputOrderQaLabel,
  deckcraftInputOrderQaScript,
  decodeDeckcraftInputOrderQa,
  type DeckcraftInputOrderQa,
} from './deckcraft-input-order-qa';
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
  const inputOrderQa = deckcraftInputOrderQaEnabled(
    Platform.OS,
    process.env.EXPO_PUBLIC_ZOEN_DECKCRAFT_INPUT_QA,
    runtimePackage,
    DECKCRAFT_ENGINE_REVISION,
  );
  const document = useMemo(() => {
    const nonce = randomUUID();
    const qaSession = inputOrderQa ? randomUUID() : null;
    return {
      nonce,
      qaSession,
      qaScript: qaSession ? deckcraftInputOrderQaScript(qaSession) : undefined,
      html: nativeMiniappHtml(html, 'Zoen', nonce, runtimePackage),
    };
  }, [html, runtimePackage, inputOrderQa]);
  const [qaState, setQaState] = useState<{
    owner: typeof document;
    trace: DeckcraftInputOrderQa;
  } | null>(null);
  // Layout/theme updates must not submit a fresh native source and reload the App.
  const source = useMemo(() => ({ html: document.html }), [document]);
  const active = useRef<typeof document | null>(document);
  const inputAllowed = useRef(!blocked);
  useLayoutEffect(() => {
    inputAllowed.current = !blocked;
  }, [blocked]);
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
        setQaState(null);
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
        onTouchStart={() => {
          if (Platform.OS === 'ios' && active.current === document && inputAllowed.current)
            view.current?.requestFocus();
        }}
        onShouldStartLoadWithRequest={nativeMiniappNavigation}
        onOpenWindow={() => {}}
        onLoadEnd={() => {
          if (active.current === document)
            view.current?.injectJavaScript(nativeMiniappCommand(document.nonce, 'title', title));
        }}
        onMessage={(event) => {
          if (active.current !== document) return;
          if (inputOrderQa && document.qaSession) {
            const trace = decodeDeckcraftInputOrderQa(event.nativeEvent.data, document.qaSession);
            if (trace) {
              setQaState({ owner: document, trace });
              return;
            }
          }
          if (!nativeMiniappMessageSource(event.nativeEvent.url, Platform.OS)) return;
          const message = nativeMiniappMessage(event.nativeEvent.data, document.nonce);
          if (message !== null) onMessage(message);
        }}
        onError={(event) => {
          if (active.current === document) onError(event.nativeEvent.description);
        }}
        onContentProcessDidTerminate={() => {
          if (active.current === document)
            onError(t('miniapps.theSystemClosedThisInterfaceRefreshToOpen'));
        }}
        style={{ backgroundColor: 'transparent' }}
      />
      {inputOrderQa ? (
        <Text
          testID="deckcraft-input-order-qa"
          accessible
          accessibilityLabel={deckcraftInputOrderQaLabel(
            qaState?.owner === document ? qaState.trace : null,
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
          {deckcraftInputOrderQaLabel(qaState?.owner === document ? qaState.trace : null)}
        </Text>
      ) : null}
    </View>
  );
});
