import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import type { ReactNode, Ref } from 'react';
import { TextInput, View, type TextInputInstance } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
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
}) {
  const { colors, scheme } = useTheme();
  const insets = useSafeAreaInsets();
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
  function controls({ attachment, voice }: { attachment: ReactNode; voice: ReactNode }) {
    return (
      <>
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
      </>
    );
  }
  return (
    <View
      style={{
        paddingHorizontal: 12,
        paddingTop: 10,
        paddingBottom: Math.max(insets.bottom, 12),
        flexDirection: 'row',
        gap: 10,
        alignItems: 'flex-end',
      }}
    >
      <MediaComposer
        spaceId={spaceId}
        replyTo={replyTo}
        thread={thread}
        disabled={sending || editing}
        onSent={onMediaSent}
        renderControls={({ attachment, voice }) =>
          controls({ attachment: editing ? null : attachment, voice: editing ? null : voice })
        }
      />
    </View>
  );
}
