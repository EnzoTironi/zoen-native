// SPDX-License-Identifier: GPL-2.0-or-later
// Telegram-iOS f1dd7a2: ThemeSettingsChatPreviewItem uses the normal chat artwork,
// selected wallpaper and message font. Preview content never enters the message store.
import { Text, View } from 'react-native';
import { t } from '../i18n';
import { useTheme } from '../theme';
import { MessageBubbleGraphic, MessageReceiptGraphic } from './message-graphics';
import { SkyLayer } from './sky-background';

/** Derived from the running presentation, including restored and automatic changes. */
export function ThemeChatPreview() {
  const { presentation: p, sky } = useTheme();
  return (
    <View
      testID="theme-chat-preview"
      pointerEvents="none"
      style={{
        borderRadius: 10,
        overflow: 'hidden',
        paddingVertical: 8,
        paddingHorizontal: 4,
        gap: 4,
      }}
    >
      <SkyLayer sky={sky} />
      {(['incoming', 'outgoing'] as const).map((direction) => {
        const incoming = direction === 'incoming';
        const face = p.chat[direction];
        return (
          <View
            key={direction}
            testID={`theme-chat-preview-${direction}`}
            style={{
              alignSelf: incoming ? 'flex-start' : 'flex-end',
              maxWidth: '90%',
              minHeight: 35,
              paddingTop: 6,
              paddingBottom: 6,
              paddingLeft: incoming ? 17 : 11,
              paddingRight: incoming ? 11 : 17,
            }}
          >
            <MessageBubbleGraphic
              ownerKey={`theme-preview-${direction}`}
              incoming={incoming}
              above={false}
              below={false}
              fill={face.fill}
              fillColors={face.fillColors}
            />
            <Text style={{ ...p.typography.message, color: face.text }}>
              {t(incoming ? 'settings.themePreviewReceived' : 'settings.themePreviewOwn')}
            </Text>
            <View
              style={{
                alignSelf: 'flex-end',
                flexDirection: 'row',
                alignItems: 'center',
                gap: 5,
                paddingTop: 2,
              }}
            >
              <Text
                style={{
                  color: face.secondary,
                  fontSize: 11,
                  lineHeight: 14,
                  fontVariant: ['tabular-nums'],
                }}
              >
                18:20
              </Text>
              {!incoming ? (
                <MessageReceiptGraphic shape="double" color={face.secondary} reducedMotion />
              ) : null}
            </View>
          </View>
        );
      })}
    </View>
  );
}
