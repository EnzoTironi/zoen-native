import { t } from '../i18n/index.ts';
import { forwardRef, useImperativeHandle, useRef } from 'react';
import { View } from 'react-native';
import { WebView } from 'react-native-webview';
import type { MiniappFrameHandle, MiniappFrameProps } from './frame-types';

export const MiniappFrame = forwardRef<MiniappFrameHandle, MiniappFrameProps>(function MiniappFrame(
  { html, onMessage, onError, height, title, blocked },
  ref,
) {
  const view = useRef<WebView<object>>(null);
  useImperativeHandle(
    ref,
    () => ({
      dispose: () => {
        view.current?.stopLoading();
        view.current?.injectJavaScript('window.location.replace("about:blank");true;');
      },
      send: (message) =>
        view.current?.injectJavaScript(
          `window.__zoenDeliver?.(JSON.parse(${JSON.stringify(JSON.stringify(message))}));true;`,
        ),
    }),
    [],
  );
  return (
    <View
      pointerEvents={blocked ? 'none' : 'auto'}
      accessibilityElementsHidden={blocked}
      style={{ height, minHeight: 400 }}
    >
      <WebView<object>
        ref={view}
        source={{ html }}
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
        onShouldStartLoadWithRequest={(request) =>
          request.url === 'about:blank' || request.url === ''
        }
        onOpenWindow={() => {}}
        onMessage={(event) => {
          if (event.nativeEvent.url === 'about:blank' || event.nativeEvent.url === '')
            onMessage(event.nativeEvent.data);
        }}
        onError={(event) => onError(event.nativeEvent.description)}
        onContentProcessDidTerminate={() =>
          onError(t('miniapps.theSystemClosedThisInterfaceRefreshToOpen'))
        }
        style={{ backgroundColor: 'transparent' }}
      />
    </View>
  );
});
