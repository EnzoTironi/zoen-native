import { t, formatLocale } from '../../../i18n/index.ts';
import { ProfileAvatar } from '../../../profile/profile-avatar';
import { useCallback, useState } from 'react';
import { Stack, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { Text, View } from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useCoreQuery } from '../../../core/query';
import { useClient } from '../../../core/provider';
import { messageText } from '../../../core/presentation';
import { EmptyState, Loading, NativeButton } from '../../../components/primitives';
import { useTheme } from '../../../theme';
import { MessageComposer } from '../../../components/message-composer';
import { ConversationViewport } from '../../../components/conversation-viewport';
import { MessageMedia } from '../../../media/MediaAttachment';

export default function Thread() {
  const { space, root } = useLocalSearchParams<{ space: string; root: string }>();
  const { store } = useClient();
  useFocusEffect(useCallback(() => () => store.setTyping(space, false), [space, store]));
  const {
    data: entries,
    error,
    retry,
  } = useCoreQuery({ op: 'thread', space_id: space, root_id: root });
  const { colors } = useTheme();
  const [draft, setDraft] = useState('');
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  async function send() {
    if (!draft.trim() || busy) return;
    const text = draft;
    store.setTyping(space, false);
    setBusy(true);
    setFailure(null);
    try {
      await store.request({
        op: 'send_message',
        space_id: space,
        text,
        reply_to: root,
        thread: true,
      });
      setDraft((value) => (value === text ? '' : value));
    } catch (reason) {
      setFailure(reason instanceof Error ? reason.message : t('chat.theReplyCouldNotBeSent'));
    } finally {
      setBusy(false);
    }
  }
  return (
    <KeyboardAvoidingView
      behavior="padding"
      automaticOffset
      style={{ flex: 1, backgroundColor: 'transparent' }}
    >
      <Stack.Screen options={{ title: t('chat.replies') }} />
      <ConversationViewport
        composer={
          <>
            {failure || error ? (
              <Text
                accessibilityRole="alert"
                selectable
                style={{ color: colors.danger, padding: 16 }}
              >
                {failure ?? error}
              </Text>
            ) : null}
            <MessageComposer
              spaceId={space}
              replyTo={root}
              thread
              value={draft}
              sending={busy}
              onChangeText={(text) => {
                setDraft(text);
                store.setTyping(space, Boolean(text.trim()));
              }}
              onBlur={() => store.setTyping(space, false)}
              onSend={() => {
                void send();
              }}
            />
          </>
        }
      >
        {(composerSpacer) =>
          error && !entries ? (
            <EmptyState title={t('chat.theRepliesCouldNotBeOpened')} detail={error}>
              <NativeButton label={t('chat.tryAgain2')} onPress={retry} />
            </EmptyState>
          ) : !entries ? (
            <Loading />
          ) : (
            <FlashList
              data={entries}
              keyExtractor={(entry) => entry.id}
              keyboardDismissMode="interactive"
              keyboardShouldPersistTaps="handled"
              contentInsetAdjustmentBehavior="automatic"
              ListFooterComponent={composerSpacer}
              contentContainerStyle={{
                paddingHorizontal: 20,
                paddingTop: 20,
                paddingBottom: 20,
                gap: 18,
              }}
              maintainVisibleContentPosition={{
                startRenderingFromBottom: false,
                autoscrollToBottomThreshold: 0.15,
              }}
              renderItem={({ item, index }) => (
                <View
                  style={{
                    flexDirection: 'row',
                    gap: 12,
                    paddingBottom: index === 0 ? 22 : 4,
                    borderBottomWidth: index === 0 ? 1 : 0,
                    borderColor: colors.line,
                  }}
                >
                  <ProfileAvatar
                    person={item.author}
                    name={item.author.name}
                    color={item.author.tint_hex}
                    agent={item.author.kind === 'Agent'}
                    size={36}
                  />
                  <View style={{ flex: 1, gap: 7 }}>
                    <View style={{ flexDirection: 'row', alignItems: 'baseline', gap: 10 }}>
                      <Text style={{ color: colors.text, fontSize: 15, fontWeight: '600' }}>
                        {item.author.name}
                      </Text>
                      <Text style={{ color: colors.secondary, fontSize: 11 }}>
                        {new Date(Number(item.at_ms)).toLocaleTimeString(formatLocale, {
                          hour: '2-digit',
                          minute: '2-digit',
                        })}
                      </Text>
                    </View>
                    {!item.voice &&
                    !('Message' in item.kind && item.kind.Message.card?.kind_id === 'file') ? (
                      <Text selectable style={{ color: colors.text, fontSize: 17, lineHeight: 25 }}>
                        {messageText(item)}
                      </Text>
                    ) : null}
                    <MessageMedia entry={item} />
                    {index === 0 ? (
                      <Text style={{ color: colors.secondary, fontSize: 12 }}>
                        {t('chat.originalMessage')}
                      </Text>
                    ) : null}
                  </View>
                </View>
              )}
            />
          )
        }
      </ConversationViewport>
    </KeyboardAvoidingView>
  );
}
