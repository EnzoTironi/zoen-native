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
import {
  decodeWordcraftRuntimeQa,
  wordcraftRuntimeQaCommand,
  wordcraftRuntimeQaEnabled,
  wordcraftRuntimeQaLabel,
  wordcraftRuntimeQaOwnerReady,
  wordcraftRuntimeQaScript,
  type WordcraftRuntimeQa,
} from './wordcraft-runtime-qa';
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
  const wordQa = wordcraftRuntimeQaEnabled(
    Platform.OS,
    process.env.EXPO_PUBLIC_ZOEN_WORDCRAFT_RUNTIME_QA,
    runtimePackage,
  );
  const document = useMemo(() => {
    const nonce = randomUUID();
    const qaSession = inputOrderQa || wordQa ? randomUUID() : null;
    return {
      nonce,
      qaSession,
      qaScript: qaSession
        ? deckcraftInputOrderQaScript(qaSession, wordQa ? 'wordcraft_canvas' : 'deckcraft_canvas') +
          (wordQa ? wordcraftRuntimeQaScript(qaSession) : '')
        : undefined,
      html: nativeMiniappHtml(html, 'Zoen', nonce, runtimePackage),
    };
  }, [html, runtimePackage, inputOrderQa, wordQa]);
  const [qaState, setQaState] = useState<{
    owner: typeof document;
    trace: DeckcraftInputOrderQa;
  } | null>(null);
  const [wordQaState, setWordQaState] = useState<{
    owner: typeof document;
    trace: WordcraftRuntimeQa;
  } | null>(null);
  const [nativeQa, setNativeQa] = useState<{
    owner: typeof document;
    touches: number;
    requests: number;
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
        setWordQaState(null);
        setNativeQa(null);
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
          if (Platform.OS === 'ios' && active.current === document && inputAllowed.current) {
            const target = view.current;
            target?.requestFocus();
            if (wordQa)
              setNativeQa((previous) => ({
                owner: document,
                touches: Math.min(65535, (previous?.owner === document ? previous.touches : 0) + 1),
                requests: Math.min(
                  65535,
                  (previous?.owner === document ? previous.requests : 0) + (target ? 1 : 0),
                ),
              }));
          }
        }}
        onShouldStartLoadWithRequest={nativeMiniappNavigation}
        onOpenWindow={() => {}}
        onLoadEnd={() => {
          if (active.current === document)
            view.current?.injectJavaScript(nativeMiniappCommand(document.nonce, 'title', title));
        }}
        onMessage={(event) => {
          if (active.current !== document) return;
          if ((inputOrderQa || wordQa) && document.qaSession) {
            if (wordQa) {
              const snapshot = decodeWordcraftRuntimeQa(event.nativeEvent.data, document.qaSession);
              if (snapshot) {
                setWordQaState({ owner: document, trace: snapshot });
                return;
              }
            }
            const trace = decodeDeckcraftInputOrderQa(event.nativeEvent.data, document.qaSession);
            if (trace) {
              setQaState({ owner: document, trace });
              return;
            }
          }
          if (!nativeMiniappMessageSource(event.nativeEvent.url, Platform.OS)) return;
          const message = nativeMiniappMessage(event.nativeEvent.data, document.nonce);
          if (message !== null) {
            if (wordQa && document.qaSession) {
              const ready = wordcraftRuntimeQaOwnerReady(message);
              if (ready !== null)
                view.current?.injectJavaScript(
                  wordcraftRuntimeQaCommand(document.qaSession, ready),
                );
            }
            onMessage(message);
          }
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
      {inputOrderQa || wordQa ? (
        <Text
          testID={wordQa ? 'wordcraft-input-order-qa' : 'deckcraft-input-order-qa'}
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
      {wordQa ? (
        <Text
          testID="wordcraft-runtime-qa"
          accessible
          accessibilityLabel={
            wordcraftRuntimeQaLabel(wordQaState?.owner === document ? wordQaState.trace : null) +
            `; RNtouches=${nativeQa?.owner === document ? nativeQa.touches : 0}; RNfocusRequests=${nativeQa?.owner === document ? nativeQa.requests : 0}`
          }
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
        />
      ) : null}
    </View>
  );
});
