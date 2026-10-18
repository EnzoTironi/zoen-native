import { GlassView, isLiquidGlassAvailable } from 'expo-glass-effect';
import type { ReactNode, Ref } from 'react';
import { Text, TextInput, View, type TextInputInstance } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useKeyboardMotion } from './keyboard-motion';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';
import { t } from '../i18n/index.ts';
import { conversationQueryFits, type ConversationSearchState } from '../core/conversation-search';
import { MediaComposer } from '../media/MediaComposer';
import { useTheme } from '../theme';
import { ComposerAction } from './composer-action';

export type ComposerSearchMode = {
  state: ConversationSearchState;
  status: string;
  onClose: () => void;
  onMove: (direction: 'older' | 'newer') => void;
  onAsk: () => void;
  onSearch: () => void;
};

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
  threadRootId,
  inputRef,
  mediaEnabled = true,
  suspended = false,
  search,
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
  threadRootId?: string;
  inputRef?: Ref<TextInputInstance>;
  mediaEnabled?: boolean;
  suspended?: boolean;
  search?: ComposerSearchMode;
}) {
  const { colors, scheme } = useTheme();
  const insets = useSafeAreaInsets();
  const { progress } = useKeyboardMotion();
  const restingBottom = Math.max(insets.bottom, 12);
  const keyboardPadding = useAnimatedStyle(() => ({
    // The iOS viewport moves this stable layout and reserves it with native insets.
    paddingBottom:
      process.env.EXPO_OS === 'ios'
        ? restingBottom
        : restingBottom + (8 - restingBottom) * Math.min(1, Math.max(0, progress.value)),
  }));
  const hasText = Boolean(value.trim());
  const canAsk = Boolean(
    search &&
    search.state.completed &&
    !search.state.loading &&
    !search.state.error &&
    !search.state.hits.length &&
    !search.state.cursor &&
    search.state.query.trim() &&
    conversationQueryFits(search.state.query.trim()),
  );
  const glass = process.env.EXPO_OS === 'ios' && isLiquidGlassAvailable();
  const field = (
    <TextInput
      ref={inputRef}
      testID="message-input"
      accessibilityLabel={
        search ? t('chat.searchThisChat') : editing ? t('chat.editMessage') : t('chat.message')
      }
      placeholder={
        search
          ? t('chat.searchThisChat')
          : thread
            ? t('chat.continueTheConversation')
            : t('chat.message')
      }
      placeholderTextColor={colors.tertiary}
      multiline
      returnKeyType={search ? 'search' : 'default'}
      submitBehavior={search ? 'submit' : undefined}
      onSubmitEditing={search?.onSearch}
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
        {search ? (
          <Text
            accessibilityLiveRegion="polite"
            numberOfLines={3}
            style={{
              color: search.state.error ? colors.danger : colors.secondary,
              fontSize: 12,
              lineHeight: 17,
              paddingHorizontal: 8,
            }}
          >
            {search.status}
          </Text>
        ) : (
          review
        )}
        <View style={{ flexDirection: 'row', gap: 10, alignItems: 'flex-end' }}>
          {search ? (
            <ComposerAction
              testID="close-conversation-search"
              name="xmark"
              label={t('chat.closeSearch')}
              onPress={search.onClose}
            />
          ) : (
            attachment
          )}
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
          {search ? (
            canAsk ? (
              <ComposerAction
                testID="ask-conversation"
                name="arrow.up"
                label={t('chat.askAboutChat')}
                prominent
                onPress={search.onAsk}
              />
            ) : (
              <View style={{ flexDirection: 'row', alignItems: 'center', gap: 2, minHeight: 52 }}>
                <ComposerAction
                  testID="older-search-result"
                  name="chevron.up"
                  size={44}
                  label={t('chat.olderSearchResult')}
                  disabled={
                    search.state.loading ||
                    search.state.index < 0 ||
                    (!search.state.cursor && search.state.index >= search.state.hits.length - 1)
                  }
                  onPress={() => search.onMove('older')}
                />
                <ComposerAction
                  testID="newer-search-result"
                  name="chevron.down"
                  size={44}
                  label={t('chat.newerSearchResult')}
                  disabled={search.state.loading || search.state.index <= 0}
                  onPress={() => search.onMove('newer')}
                />
              </View>
            )
          ) : hasText || editing || !voice ? (
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
          key={`${spaceId}:${threadRootId ?? 'chat'}`}
          spaceId={spaceId}
          replyTo={replyTo}
          thread={thread}
          disabled={sending || editing || Boolean(search)}
          suspended={suspended || Boolean(search)}
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
