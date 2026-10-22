import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import { Stack, router, type NativeStackHeaderProps } from 'expo-router';
import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { PixelRatio, Pressable, View, useWindowDimensions } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { chatHeaderLayout, chatHeaderTitlePlacement } from '../core/telegram-chat-header-layout';
import { t } from '../i18n/index.ts';
import { ProfileAvatar } from '../profile/profile-avatar';
import { useTheme } from '../theme';
import { ChatTitle, type ChatHeaderProps } from './chat-title';
import { Icon } from './primitives';
import { NavigationBackdrop } from './navigation-backdrop';

export function ChatHeader(props: ChatHeaderProps) {
  return (
    <Stack.Screen
      options={{
        title: props.title,
        headerShown: true,
        headerTransparent: true,
        // NativeStackView hides its UIKit bar for this public replacement and
        // measures its full height. ScreenStackItem/stack gestures remain native.
        header: (native) => <ChatNavigationBar {...props} native={native} />,
      }}
    />
  );
}

function ChatNavigationBar({
  native,
  ...props
}: ChatHeaderProps & { native: NativeStackHeaderProps }) {
  const { presentation: p, scheme } = useTheme();
  const { width, fontScale } = useWindowDimensions();
  const insets = useSafeAreaInsets();
  const search = props.search;
  const layout = chatHeaderLayout({
    width,
    top: insets.top,
    left: insets.left,
    right: insets.right,
    back: native.back != null,
    search: search != null,
  });
  const geometry = [
    props.id,
    width,
    insets.left,
    insets.right,
    !!native.back,
    !!search,
    fontScale,
  ].join('|');
  const render = JSON.stringify([geometry, props.title, props.activityLabel]);
  const currentRender = useRef<string | null>(render);
  useLayoutEffect(() => {
    currentRender.current = render;
    return () => {
      if (currentRender.current === render) currentRender.current = null;
    };
  }, [render]);
  const [measured, setMeasured] = useState<{ geometry: string; width: number } | null>(null);
  const titlePlacement = chatHeaderTitlePlacement(
    width,
    layout,
    measured?.geometry === geometry ? measured.width : layout.availableWidth,
    PixelRatio.get(),
  );
  const group = (children: ReactNode) =>
    isLiquidGlassAvailable() ? (
      <GlassView
        isInteractive
        colorScheme={scheme}
        glassEffectStyle="regular"
        style={{ borderRadius: 22 }}
      >
        {children}
      </GlassView>
    ) : (
      <View style={{ borderRadius: 22, backgroundColor: p.navigation.background }}>{children}</View>
    );
  return (
    <View
      testID="chat-navigation-bar"
      pointerEvents="box-none"
      style={{ height: layout.height, width: '100%' }}
    >
      <NavigationBackdrop height={layout.height} solidHeight={insets.top} />
      {native.back ? (
        <View style={{ position: 'absolute', top: layout.controlTop, left: layout.leading }}>
          {group(
            <Pressable
              testID="chat-back"
              accessibilityRole="button"
              accessibilityLabel={t('access.back')}
              onPress={() => native.navigation.goBack()}
              style={({ pressed }) => ({
                width: 44,
                height: 44,
                alignItems: 'center',
                justifyContent: 'center',
                opacity: pressed ? 0.65 : 1,
              })}
            >
              <Icon
                name={{ ios: 'chevron.left', android: 'arrow_back', web: 'arrow_back' }}
                size={22}
                color={p.navigation.control}
              />
            </Pressable>,
          )}
        </View>
      ) : null}
      <View
        testID="chat-title-position"
        style={{
          position: 'absolute',
          top: layout.controlTop,
          left: titlePlacement.x,
          maxWidth: layout.availableWidth,
        }}
      >
        <ChatTitle
          {...props}
          availableWidth={layout.availableWidth}
          onLayout={(event) => {
            if (currentRender.current !== render) return;
            const measuredWidth = event.nativeEvent.layout.width;
            if (!Number.isFinite(measuredWidth) || measuredWidth < 0) return;
            const boundedWidth = Math.min(layout.availableWidth, measuredWidth);
            setMeasured((prior) =>
              prior?.geometry === geometry && prior.width === boundedWidth
                ? prior
                : { geometry, width: boundedWidth },
            );
          }}
        />
      </View>
      <View style={{ position: 'absolute', top: layout.controlTop, left: layout.trailing }}>
        {group(
          <View style={{ flexDirection: 'row', width: layout.trailingWidth }}>
            {search ? (
              <Pressable
                testID="chat-search"
                accessibilityRole="button"
                accessibilityLabel={t(search.active ? 'chat.closeSearch' : 'chat.searchThisChat')}
                onPress={search.onToggle}
                style={({ pressed }) => ({
                  width: 44,
                  height: 44,
                  alignItems: 'center',
                  justifyContent: 'center',
                  opacity: pressed ? 0.65 : 1,
                })}
              >
                <Icon
                  name={{ ios: 'magnifyingglass', android: 'search', web: 'search' }}
                  size={22}
                  color={p.navigation.control}
                />
              </Pressable>
            ) : null}
            <Pressable
              testID="chat-details-avatar"
              accessibilityRole="button"
              accessibilityLabel={props.title}
              accessibilityHint={t('chat.chatDetails')}
              onPress={() => router.push({ pathname: '/space/[id]', params: { id: props.id } })}
              style={({ pressed }) => ({
                width: 44,
                height: 44,
                alignItems: 'center',
                justifyContent: 'center',
                opacity: pressed ? 0.65 : 1,
              })}
            >
              <ProfileAvatar
                person={props.counterpart}
                name={props.title}
                color={props.counterpart?.tint_hex}
                size={38}
              />
            </Pressable>
          </View>,
        )}
      </View>
    </View>
  );
}
