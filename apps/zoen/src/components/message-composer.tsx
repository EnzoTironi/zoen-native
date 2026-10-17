import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import type { ReactNode, Ref } from 'react';
import { TextInput, View, type TextInputInstance } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useKeyboardMotion } from './keyboard-motion';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';
import { t } from '../i18n/index.ts';
import { MediaComposer } from '../media/MediaComposer';
import { useTheme } from '../theme';
import { ComposerAction } from './composer-action';

export function MessageComposer({
  spaceId,
  value,
  onChangeText,
  onBlur,
  onSend,
  onMediaSent,
  sending,
  editing = false,
  replyTo = null,
  thread = false,
  inputRef,
  mediaEnabled = true,
  suspended = false,
}: {
  spaceId: string;
  value: string;
  onChangeText: (text: string) => void;
  onBlur: () => void;
  onSend: () => void;
  onMediaSent?: () => void;
  sending: boolean;
  editing?: boolean;
  replyTo?: string | null;
  thread?: boolean;
  inputRef?: Ref<TextInputInstance>;
  mediaEnabled?: boolean;
  suspended?: boolean;
}) {
  const { colors, scheme } = useTheme();
  const insets = useSafeAreaInsets();
  const { progress } = useKeyboardMotion();
  const restingBottom = Math.max(insets.bottom, 12);
  const keyboardPadding = useAnimatedStyle(() => ({
    paddingBottom: restingBottom + (8 - restingBottom) * Math.min(1, Math.max(0, progress.value)),
  }));
  const hasText = Boolean(value.trim());
  const glass = process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable();
  const field = (
    <TextInput
      ref={inputRef}
      testID="message-input"
      accessibilityLabel={editing ? t('chat.editMessage') : t('chat.message')}
      placeholder={thread ? t('chat.continueTheConversation') : t('chat.message')}
      placeholderTextColor={colors.tertiary}
      multiline
      value={value}
      onChangeText={onChangeText}
      onBlur={onBlur}
      style={{
        minHeight: 52,
        maxHeight: 160,
        paddingHorizontal: 18,
        paddingTop: 15,
        paddingBottom: 14,
        color: colors.text,
        fontSize: 16,
        lineHeight: 22,
      }}
    />
  );
  function controls({
    attachment,
    voice,
    review,
  }: {
    attachment: ReactNode;
    voice: ReactNode;
    review?: ReactNode;
  }) {
    return (
      <>
        {review}
        <View style={{ flexDirection: 'row', gap: 10, alignItems: 'flex-end' }}>
          {attachment}
          {glass ? (
            <GlassView
              colorScheme={scheme}
              glassEffectStyle="regular"
              style={{ flex: 1, borderRadius: 27 }}
            >
              {field}
            </GlassView>
          ) : (
            <View style={{ flex: 1, borderRadius: 27, backgroundColor: colors.surface }}>
              {field}
            </View>
          )}
          {hasText || editing || !voice ? (
            <ComposerAction
              testID="send-message"
              name={editing ? 'checkmark' : 'arrow.up'}
              label={editing ? t('chat.saveEdit') : t('chat.sendMessage')}
              prominent
              disabled={!hasText || sending}
              onPress={onSend}
            />
          ) : (
            voice
          )}
        </View>
      </>
    );
  }
  return (
    <Animated.View
      style={[
        {
          paddingHorizontal: 12,
          paddingTop: 10,
          gap: 8,
        },
        keyboardPadding,
      ]}
    >
      {mediaEnabled ? (
        <MediaComposer
          spaceId={spaceId}
          replyTo={replyTo}
          thread={thread}
          disabled={sending || editing}
          suspended={suspended}
          onSent={onMediaSent}
          renderControls={({ attachment, voice, review }) =>
            controls({
              attachment: editing ? null : attachment,
              voice: editing ? null : voice,
              review,
            })
          }
        />
      ) : (
        controls({ attachment: null, voice: null })
      )}
    </Animated.View>
  );
}
