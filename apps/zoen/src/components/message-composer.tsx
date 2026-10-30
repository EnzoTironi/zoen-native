import { createRef, type ReactNode, type Ref, type ComponentProps } from 'react';
import { Text, View, Pressable, type TextInputInstance } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useKeyboardMotion } from './keyboard-motion';
import {
  composerFieldInsets,
  composerSourceMinimumHeight,
  telegramComposer,
} from '../core/telegram-composer-presentation';
import Animated, { useAnimatedStyle } from 'react-native-reanimated';
import { t } from '../i18n/index.ts';
import { conversationQueryFits, type ConversationSearchState } from '../core/conversation-search';
import { MediaComposer } from '../media/MediaComposer';
import type { AttachmentComposerField } from '../media/attachment-composer';
import { AttachmentReplyContext } from '../media/attachment-reply-context';
import { ComposerReplyContext } from './composer-reply-context';
import { useTheme } from '../theme';
import { ComposerAction } from './composer-action';
import { ComposerGlassContainer, ComposerGlassSurface } from './composer-glass';
import {
  ComposerMotionSlot,
  composerFrameTransition,
  useComposerActionMotion,
} from './composer-action-motion';
import { composerExternalWidth } from '../core/telegram-composer-motion';
import { ConversationComposerAccess } from './conversation-composer-access';
import { ComposerTextField } from './composer-text-field';
import { NativeButton } from './primitives';
import { useClient } from '../core/provider';
import { homeOwnerFor } from '../home/access';
import { ComposerGlyph } from './composer-control-surface';
import {
  AttachmentKeyboardProvider,
  useAttachmentKeyboard,
} from '../media/attachment-keyboard-context';

function ComposerInputAccessory({ minimumHeight }: { minimumHeight: number }) {
  const keyboard = useAttachmentKeyboard();
  const { active } = useComposerActionMotion();
  const { presentation: p } = useTheme();
  if (!keyboard) return null;
  const emoji = keyboard.kind === 'emoji';
  return (
    <Pressable
      testID="composer-input-mode"
      accessible={active}
      accessibilityElementsHidden={!active}
      importantForAccessibility={active ? 'auto' : 'no-hide-descendants'}
      accessibilityRole="button"
      accessibilityLabel={t(emoji ? 'composerKeyboard.keyboard' : 'composerKeyboard.emoji')}
      hitSlop={6}
      disabled={!active}
      onPress={() => {
        if (active) keyboard.toggleEmoji();
      }}
      style={{
        width: telegramComposer.inputModeControl,
        height: minimumHeight,
        alignItems: 'center',
        justifyContent: 'center',
      }}
    >
      <ComposerGlyph name={emoji ? 'keyboard' : 'face.smiling'} color={p.chat.input.control} />
    </Pressable>
  );
}

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
  onFocus,
  onBlur,
  onSend,
  onMediaSent,
  sending,
  editing = false,
  allowEmptyEdit = false,
  replyTo = null,
  thread = false,
  threadRootId,
  inputRef,
  mediaEnabled = true,
  suspended = false,
  search,
  context,
  draft,
}: {
  spaceId: string;
  value: string;
  onChangeText: (text: string) => void;
  onFocus?: () => void;
  onBlur: () => void;
  onSend: () => void;
  onMediaSent?: () => void;
  sending: boolean;
  editing?: boolean;
  allowEmptyEdit?: boolean;
  replyTo?: string | null;
  thread?: boolean;
  threadRootId?: string;
  inputRef?: Ref<TextInputInstance>;
  mediaEnabled?: boolean;
  suspended?: boolean;
  search?: ComposerSearchMode;
  context?: Pick<
    ComponentProps<typeof ComposerReplyContext>,
    'entry' | 'edit' | 'quote' | 'shared'
  > & { onCancel: () => void };
  draft?: { ready: boolean; error: boolean; rejected?: boolean; onRetry: () => void };
}) {
  const { colors, scheme, presentation: p, reducedMotion } = useTheme();
  const client = useClient();
  const sessionScope = JSON.stringify([
    homeOwnerFor(client),
    client.store.sessionEpoch,
    client.snapshot?.account?.relay_url ?? null,
    client.store.isSessionRetiring,
  ]);
  const insets = useSafeAreaInsets();
  const { progress } = useKeyboardMotion();
  const restingBottom = Math.max(insets.bottom, 12);
  const keyboardPadding = useAnimatedStyle(() => ({
    paddingBottom: restingBottom + (8 - restingBottom) * Math.min(1, Math.max(0, progress.value)),
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
  const minimumHeight = composerSourceMinimumHeight(p.typography.message.fontSize) ?? 40;
  function controls({
    attachment,
    voice,
    review,
    files,
  }: {
    attachment: ReactNode;
    voice: ReactNode;
    review?: ReactNode;
    files?: AttachmentComposerField;
  }) {
    const fieldText = files ? files.value : value;
    const native = process.env.EXPO_OS === 'ios';
    // The native send action belongs inside the field; the empty microphone
    // remains a separate control. Telegram f1dd7a2 panel:3407,3453.
    const inlineAction =
      native && (search ? canAsk : Boolean(files || hasText || editing || !voice));
    const hasInputMode = native && !search;
    const inputInsets = composerFieldInsets({
      fontSize: p.typography.message.fontSize,
      hasSend: inlineAction,
      accessoryButtonsWidth: hasInputMode ? 34 : 0,
    }).total;
    const send = createRef<() => void>();
    const field = (
      <ComposerTextField
        richText={!search}
        inputRef={inputRef}
        bindSend={(capture) => {
          send.current = capture;
        }}
        onSend={onSend}
        context={JSON.stringify([
          spaceId,
          threadRootId ?? null,
          search ? 'search' : files ? 'caption' : editing ? 'edit' : 'normal',
          files?.mutationId ?? (editing ? (context?.edit?.id ?? context?.entry?.id ?? null) : null),
          sessionScope,
        ])}
        testID="message-input"
        accessibilityLabel={
          search
            ? t('chat.searchThisChat')
            : files
              ? t('media.attachmentCaption', { position: files.position, total: files.total })
              : editing
                ? t('chat.editMessage')
                : t('chat.message')
        }
        placeholder={
          search
            ? t('chat.searchThisChat')
            : files || allowEmptyEdit
              ? t('media.attachmentCaptionPlaceholder')
              : thread
                ? t('chat.continueTheConversation')
                : t('chat.message')
        }
        placeholderTextColor={p.chat.input.placeholder}
        selectionColor={p.chat.input.action}
        keyboardAppearance={scheme === 'dark' ? 'dark' : 'light'}
        autoCorrect={search ? false : undefined}
        autoCapitalize={search ? 'none' : undefined}
        multiline
        returnKeyType={search ? 'search' : 'default'}
        submitBehavior={search ? 'submit' : undefined}
        onSubmitEditing={search?.onSearch}
        editable={files ? files.editable : draft?.ready !== false}
        value={fieldText}
        onChangeText={files ? files.onChange : onChangeText}
        onFocus={onFocus}
        onBlur={onBlur}
        style={{
          width: native ? '100%' : undefined,
          minWidth: native ? 0 : undefined,
          minHeight: native ? minimumHeight : 52,
          maxHeight: 160,
          paddingLeft: native ? 12 : 18,
          paddingRight: native ? inputInsets.right : 18,
          // f1dd7a2: internal5/4 + regular text-container4.5/5.5.
          paddingTop: native ? inputInsets.top : 15,
          paddingBottom: native ? inputInsets.bottom : 14,
          color: p.chat.input.text,
          fontSize: native ? p.typography.message.fontSize : 16,
          lineHeight: native ? undefined : 22,
        }}
      />
    );
    const sendAction =
      search && canAsk ? (
        <ComposerAction
          testID="ask-conversation"
          name="arrow.up"
          minimumHeight={minimumHeight}
          label={t('chat.askAboutChat')}
          prominent
          onPress={search.onAsk}
        />
      ) : !search && files ? (
        <ComposerAction
          testID="send-attachments"
          name="arrow.up"
          minimumHeight={minimumHeight}
          label={
            files.retry
              ? t('media.attachmentsRetry')
              : t('media.attachmentSendCount', { count: files.remaining })
          }
          prominent
          disabled={files.sending || files.remaining === 0 || sending}
          onPress={() => files.onSend()}
        />
      ) : (
        <ComposerAction
          testID="send-message"
          name={editing ? 'checkmark' : 'arrow.up'}
          minimumHeight={minimumHeight}
          label={editing ? t('chat.saveEdit') : t('chat.sendMessage')}
          prominent
          disabled={(!hasText && !allowEmptyEdit) || sending || draft?.ready === false}
          onPress={() => send.current?.()}
        />
      );
    const searchNavigation = (
      <View
        style={{ flexDirection: 'row', alignItems: 'center', gap: 2, minHeight: native ? 40 : 52 }}
      >
        <ComposerAction
          testID="older-search-result"
          name="chevron.up"
          size={44}
          label={t('chat.olderSearchResult')}
          disabled={
            !search ||
            search.state.loading ||
            search.state.index < 0 ||
            (!search.state.cursor && search.state.index >= search.state.hits.length - 1)
          }
          onPress={() => search?.onMove('older')}
        />
        <ComposerAction
          testID="newer-search-result"
          name="chevron.down"
          size={44}
          label={t('chat.newerSearchResult')}
          disabled={!search || search.state.loading || search.state.index <= 0}
          onPress={() => search?.onMove('newer')}
        />
      </View>
    );
    const action = search
      ? canAsk
        ? sendAction
        : searchNavigation
      : files || hasText || editing || !voice
        ? sendAction
        : voice;
    const fieldContents = (
      <>
        {field}
        {native ? (
          <>
            <ComposerMotionSlot
              kind="accessory"
              active={hasInputMode}
              hasSend={inlineAction}
              style={{ position: 'absolute', bottom: 0, right: 4 }}
            >
              <ComposerInputAccessory minimumHeight={minimumHeight} />
            </ComposerMotionSlot>
            <ComposerMotionSlot
              kind="send"
              active={inlineAction}
              style={{ position: 'absolute', bottom: 0, right: 0 }}
            >
              {sendAction}
            </ComposerMotionSlot>
          </>
        ) : null}
      </>
    );
    return (
      <>
        {files?.scope.replyTo ? (
          <AttachmentReplyContext
            scope={files.scope}
            onCancel={
              files.targetFrozen || (files.scope.thread && files.scope.replyTo === threadRootId)
                ? undefined
                : context?.onCancel
            }
          />
        ) : !files && context ? (
          <ComposerReplyContext
            entry={context.entry}
            quote={context.quote}
            shared={context.shared}
            spaceId={spaceId}
            threadRootId={threadRootId}
            editing={editing}
            onCancel={context.onCancel}
          />
        ) : null}
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
          <>
            {context?.quote || context?.shared ? (
              <Text
                accessibilityLiveRegion="polite"
                style={{ color: colors.secondary, paddingHorizontal: 12, paddingBottom: 6 }}
              >
                {t('chat.quoteTextReplyOnly')}
              </Text>
            ) : null}
            {review}
          </>
        )}
        <ComposerGlassContainer
          testID="message-composer-row"
          style={{ flexDirection: 'row', gap: native ? 6 : 10, alignItems: 'flex-end' }}
        >
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
          <Animated.View
            testID="message-composer-field-frame"
            layout={native ? composerFrameTransition(reducedMotion) : undefined}
            style={{
              flex: 1,
              marginRight: native
                ? composerExternalWidth({
                    searching: Boolean(search),
                    hasSend: inlineAction,
                    hasVoice: Boolean(voice),
                  })
                : 0,
            }}
          >
            <ComposerGlassSurface
              testID="message-composer-field-surface"
              cornerRadius={native ? Math.floor(minimumHeight / 2) : 27}
            >
              {fieldContents}
            </ComposerGlassSurface>
          </Animated.View>
          {native ? (
            <>
              <ComposerMotionSlot
                kind="media"
                active={!search && !inlineAction && Boolean(voice)}
                // Source: text moves the retained mic offscreen; only search fades it.
                visible={!search && Boolean(voice)}
                style={{ position: 'absolute', bottom: 0, right: 0, width: 40, height: 40 }}
              >
                {voice}
              </ComposerMotionSlot>
              <ComposerMotionSlot
                kind="search"
                active={Boolean(search && !canAsk)}
                style={{ position: 'absolute', bottom: 0, right: 0 }}
              >
                {searchNavigation}
              </ComposerMotionSlot>
            </>
          ) : (
            action
          )}
        </ComposerGlassContainer>
      </>
    );
  }
  const composer = (
    <AttachmentKeyboardProvider
      scope={JSON.stringify([sessionScope, spaceId, threadRootId ?? null])}
      enabled={!search && !suspended}
    >
      <Animated.View
        style={[
          {
            // f1dd7a2 textFieldInsets:1432; separate action gap:3537.
            paddingLeft: insets.left + (process.env.EXPO_OS === 'ios' ? 8 : 12),
            paddingRight: insets.right + (process.env.EXPO_OS === 'ios' ? 8 : 12),
            paddingTop: process.env.EXPO_OS === 'ios' ? 0 : 10,
            gap: process.env.EXPO_OS === 'ios' ? 0 : 8,
          },
          // Choose the native layout on JS, outside the UI-runtime worklet.
          process.env.EXPO_OS === 'ios' ? { paddingBottom: restingBottom } : keyboardPadding,
        ]}
      >
        {draft?.error ? (
          <View
            style={{ flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 8 }}
          >
            <Text
              accessibilityRole="alert"
              style={{ flex: 1, color: colors.secondary, fontSize: 12 }}
            >
              {t(
                draft.rejected
                  ? editing
                    ? 'chat.editCouldNotBeSaved'
                    : 'chat.draftSendRejected'
                  : draft.ready
                    ? 'chat.draftSaveFailed'
                    : 'chat.draftLoadFailed',
              )}
            </Text>
            <NativeButton
              testID="retry-message-draft"
              label={t('chat.tryAgain2')}
              secondary
              onPress={draft.onRetry}
            />
          </View>
        ) : null}
        {mediaEnabled ? (
          <MediaComposer
            key={`${spaceId}:${threadRootId ?? 'chat'}`}
            spaceId={spaceId}
            replyTo={replyTo}
            threadRootId={threadRootId ?? null}
            thread={thread}
            disabled={sending || editing || Boolean(search) || draft?.ready === false}
            suspended={suspended || Boolean(search) || Boolean(context?.quote || context?.shared)}
            onSent={onMediaSent}
            renderControls={({ attachment, voice, review, files }) =>
              controls({
                attachment: editing ? null : attachment,
                // Keep the same gesture owner mounted while editing hides its slot.
                voice,
                review,
                files: editing || search ? undefined : files,
              })
            }
          />
        ) : (
          controls({ attachment: null, voice: null })
        )}
      </Animated.View>
    </AttachmentKeyboardProvider>
  );
  return (
    <ConversationComposerAccess spaceId={spaceId} searching={Boolean(search)}>
      {composer}
    </ConversationComposerAccess>
  );
}
