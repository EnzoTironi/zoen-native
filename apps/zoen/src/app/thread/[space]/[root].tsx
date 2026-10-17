import { ProfileAvatar } from '../../../profile/profile-avatar';
import { useCallback, useState } from 'react';
import { Stack, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { FlashList } from '@shopify/flash-list';
import { Text, TextInput, View } from 'react-native';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useCoreQuery } from '../../../core/query';
import { useClient } from '../../../core/provider';
import { messageText } from '../../../core/presentation';
import { EmptyState, Loading, NativeButton } from '../../../components/primitives';
import { useTheme } from '../../../theme';
import { MediaComposer } from '../../../media/MediaComposer';
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
  const insets = useSafeAreaInsets();
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
      setFailure(reason instanceof Error ? reason.message : 'Não foi possível responder.');
    } finally {
      setBusy(false);
    }
  }
  return (
    <KeyboardAvoidingView
      behavior="padding"
      automaticOffset
      style={{ flex: 1, backgroundColor: colors.surface }}
    >
      <Stack.Screen options={{ title: 'Respostas' }} />
      {error && !entries ? (
        <EmptyState title="Não foi possível abrir as respostas" detail={error}>
          <NativeButton label="Tentar novamente" onPress={retry} />
        </EmptyState>
      ) : !entries ? (
        <Loading />
      ) : (
        <FlashList
          data={entries}
          keyExtractor={(entry) => entry.id}
          keyboardDismissMode="interactive"
          keyboardShouldPersistTaps="handled"
          contentContainerStyle={{ padding: 20, gap: 18 }}
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
                    {new Date(Number(item.at_ms)).toLocaleTimeString([], {
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
                  <Text style={{ color: colors.secondary, fontSize: 12 }}>Mensagem original</Text>
                ) : null}
              </View>
            </View>
          )}
        />
      )}
      {failure || error ? (
        <Text accessibilityRole="alert" selectable style={{ color: colors.danger, padding: 16 }}>
          {failure ?? error}
        </Text>
      ) : null}
      <View
        style={{
          padding: 16,
          paddingBottom: Math.max(16, insets.bottom),
          flexDirection: 'row',
          alignItems: 'flex-end',
          gap: 10,
        }}
      >
        <MediaComposer spaceId={space} replyTo={root} thread disabled={busy} />
        <TextInput
          multiline
          accessibilityLabel="Responder na conversa"
          value={draft}
          onChangeText={(text) => {
            setDraft(text);
            store.setTyping(space, !!text.trim());
          }}
          onBlur={() => store.setTyping(space, false)}
          placeholder="Continuar a conversa…"
          placeholderTextColor={colors.tertiary}
          style={{
            flex: 1,
            minHeight: 48,
            maxHeight: 160,
            padding: 14,
            borderRadius: 20,
            color: colors.text,
            backgroundColor: colors.muted,
            fontSize: 16,
          }}
        />
        <NativeButton
          label={busy ? 'Enviando…' : 'Enviar'}
          disabled={busy || !draft.trim()}
          onPress={() => {
            void send();
          }}
        />
      </View>
    </KeyboardAvoidingView>
  );
}
