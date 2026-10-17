import { useCallback, useEffect, useRef, useState } from 'react';
import { Stack, router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { FlashList, type FlashListRef } from '@shopify/flash-list';
import { Host, BottomSheet, Column, Button } from '@expo/ui';
import * as Clipboard from 'expo-clipboard';
import * as Haptics from 'expo-haptics';
import { KeyboardAvoidingView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Pressable, Text, TextInput, View, type TextInputInstance } from 'react-native';
import { useDialogs } from '../../components/dialogs';
import { useClient } from '../../core/provider';
import type { TimelineEntry } from '../../core/types';
import { messageText } from '../../core/presentation';
import { useTheme } from '../../theme';
import { Avatar, EmptyState, Icon, IconButton, Loading } from '../../components/primitives';
import { MediaComposer } from '../../media/MediaComposer';
import { MessageMedia } from '../../media/MediaAttachment';

export default function Chat() {
  const { id, jump } = useLocalSearchParams<{ id: string; jump?: string }>();
  const { store, snapshot, timelines, notice } = useClient();
  const { colors, scheme } = useTheme();
  const { confirm, inform } = useDialogs();
  const insets = useSafeAreaInsets();
  const input = useRef<TextInputInstance>(null);
  const list = useRef<FlashListRef<TimelineEntry>>(null);
  const [draft, setDraft] = useState('');
  const [reply, setReply] = useState<TimelineEntry | null>(null);
  const [selected, setSelected] = useState<TimelineEntry | null>(null);
  const [sending, setSending] = useState(false);
  const [editing, setEditing] = useState<TimelineEntry | null>(null);
  const [editDraft, setEditDraft] = useState('');
  const [loadedSpace, setLoadedSpace] = useState<string | null>(null);
  const [scrollTarget, setScrollTarget] = useState<string | null>(null);
  const composerText = editing ? editDraft : draft;
  const chat = snapshot?.conversations.find((conversation) => conversation.id === id);
  const page = timelines[id];
  useFocusEffect(
    useCallback(() => {
      store.selectSpace(id);
      return () => store.selectSpace(null);
    }, [id, store]),
  );
  const reveal = useCallback(
    async (messageId: string) => {
      const index = await store.revealMessage(id, messageId);
      if (index < 0) throw new Error('Esta mensagem não está mais no histórico deste aparelho.');
      return messageId;
    },
    [id, store],
  );
  useFocusEffect(
    useCallback(() => {
      if (!jump) return;
      let active = true;
      void reveal(jump)
        .then((target) => {
          if (active) setScrollTarget(target);
        })
        .catch((reason: unknown) => {
          if (active)
            void inform(
              'Mensagem indisponível',
              reason instanceof Error ? reason.message : 'Tente novamente.',
            );
        });
      return () => {
        active = false;
      };
    }, [jump, reveal, inform]),
  );
  useEffect(() => {
    if (!scrollTarget || loadedSpace !== id || !page) return;
    const index = page.entries.findIndex((entry) => entry.id === scrollTarget);
    if (index < 0) return;
    // Wait until the loaded history has committed before addressing list indices.
    const frame = requestAnimationFrame(() => {
      void list.current?.scrollToIndex({ index, animated: true, viewPosition: 0.5 }).catch(() => {
        void inform('Não foi possível abrir a mensagem', 'Tente novamente.');
      });
      setScrollTarget(null);
    });
    return () => cancelAnimationFrame(frame);
  }, [scrollTarget, loadedSpace, id, page, inform]);
  const last = page?.entries.at(-1)?.id;
  useFocusEffect(
    useCallback(() => {
      if (last) void store.driver.request({ op: 'mark_read', space_id: id }).catch(() => {});
    }, [last, id, store]),
  );

  async function send() {
    if (!composerText.trim() || sending) return;
    const text = composerText;
    setSending(true);
    try {
      if (editing)
        await store.request({ op: 'edit_message', space_id: id, message_id: editing.id, text });
      else
        await store.request({
          op: 'send_message',
          space_id: id,
          text,
          reply_to: reply?.id ?? null,
          thread: false,
        });
      if (editing) {
        setEditing(null);
        setEditDraft('');
      } else {
        setDraft((value) => (value === text ? '' : value));
        setReply(null);
      }
    } catch {
      /* Keep the text for a retry. */
    } finally {
      setSending(false);
    }
  }

  if (!chat)
    return (
      <EmptyState
        title="Conversa indisponível"
        detail="Volte às suas conversas e tente abrir novamente."
      />
    );
  return (
    <KeyboardAvoidingView
      behavior="padding"
      automaticOffset
      style={{ flex: 1, backgroundColor: colors.surface }}
    >
      <Stack.Screen
        options={{
          title: chat.title,
          headerRight: () => (
            <Pressable
              accessibilityRole="button"
              accessibilityLabel="Detalhes da conversa"
              onPress={() => router.push({ pathname: '/space/[id]', params: { id } })}
            >
              <Avatar name={chat.title} size={34} color={chat.counterpart?.tint_hex} />
            </Pressable>
          ),
        }}
      />
      {notice ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={`${notice}. Dispensar aviso`}
          onPress={() => store.dismissNotice()}
          style={{ padding: 12, backgroundColor: colors.muted }}
        >
          <Text accessibilityRole="alert" style={{ color: colors.danger, fontSize: 13 }}>
            {notice}
          </Text>
        </Pressable>
      ) : null}
      {!page ? (
        <Loading />
      ) : (
        <FlashList
          key={id}
          ref={list}
          onLoad={() => setLoadedSpace(id)}
          data={page.entries}
          keyExtractor={(entry) => entry.id}
          keyboardDismissMode="interactive"
          keyboardShouldPersistTaps="handled"
          contentContainerStyle={{ paddingHorizontal: 16, paddingTop: 20, paddingBottom: 12 }}
          maintainVisibleContentPosition={{
            startRenderingFromBottom: true,
            autoscrollToBottomThreshold: 0.12,
          }}
          onStartReached={() => {
            if (page.has_more) void store.loadTimeline(id, true).catch(() => {});
          }}
          onStartReachedThreshold={0.1}
          ListEmptyComponent={
            <EmptyState
              title="O começo de algo bom"
              detail="Escreva a primeira mensagem desta conversa."
            />
          }
          renderItem={({ item: entry, index }) => {
            const mine = entry.author.is_me;
            const text = messageText(entry);
            const previous = page.entries[index - 1];
            const next = page.entries[index + 1];
            const grouped =
              previous?.author.id === entry.author.id &&
              Number(entry.at_ms) - Number(previous.at_ms) < 180_000;
            const lastInGroup = next?.author.id !== entry.author.id;
            const card = 'Message' in entry.kind ? entry.kind.Message.card : null;
            const system = 'System' in entry.kind || 'Background' in entry.kind;
            if (system)
              return (
                <Text
                  selectable
                  style={{
                    fontSize: 12,
                    lineHeight: 19,
                    color: colors.secondary,
                    textAlign: 'center',
                    marginVertical: 14,
                  }}
                >
                  {text}
                </Text>
              );
            return (
              <View
                style={{
                  alignItems: mine ? 'flex-end' : 'flex-start',
                  marginTop: grouped ? 3 : 16,
                  marginBottom: lastInGroup ? 5 : 0,
                }}
              >
                {!mine && !grouped && chat.kind !== 'Direct' ? (
                  <Text
                    style={{
                      color: colors.secondary,
                      fontWeight: '600',
                      fontSize: 12,
                      marginBottom: 5,
                      marginLeft: 12,
                    }}
                  >
                    {entry.author.name}
                  </Text>
                ) : null}
                <Pressable
                  accessible={!entry.voice && card?.kind_id !== 'file'}
                  onLongPress={() => {
                    void Haptics.selectionAsync().catch(() => {});
                    setSelected(entry);
                  }}
                  accessibilityRole="button"
                  accessibilityLabel={`${entry.author.name}: ${text}`}
                  accessibilityHint="Mantenha pressionado para responder ou ver ações"
                  style={{
                    maxWidth: '87%',
                    borderRadius: 23,
                    borderCurve: 'continuous',
                    paddingHorizontal: 15,
                    paddingTop: 11,
                    paddingBottom: 8,
                    backgroundColor: mine ? colors.mine : colors.theirs,
                  }}
                >
                  {entry.reply_to ? (
                    <Pressable
                      accessibilityRole="button"
                      accessibilityLabel="Ir à mensagem citada"
                      onPress={() => {
                        void reveal(entry.reply_to!.id)
                          .then((target) => {
                            setScrollTarget(target);
                          })
                          .catch(
                            () =>
                              void inform(
                                'Não foi possível abrir a mensagem',
                                'Tente novamente em instantes.',
                              ),
                          );
                      }}
                      style={{
                        borderLeftWidth: 3,
                        borderLeftColor: colors.accent,
                        paddingLeft: 9,
                        paddingVertical: 5,
                        marginBottom: 7,
                        opacity: 0.8,
                      }}
                    >
                      <Text
                        style={{
                          color: mine ? colors.onMine : colors.text,
                          fontWeight: '600',
                          fontSize: 12,
                        }}
                      >
                        {entry.reply_to.author.name}
                      </Text>
                      <Text
                        numberOfLines={2}
                        style={{ color: mine ? colors.onMine : colors.secondary, fontSize: 13 }}
                      >
                        {entry.reply_to.text}
                      </Text>
                    </Pressable>
                  ) : null}
                  {entry.forwarded ? (
                    <Text
                      style={{
                        color: mine ? colors.onMine : colors.secondary,
                        fontSize: 12,
                        fontStyle: 'italic',
                        marginBottom: 5,
                      }}
                    >
                      Encaminhada
                    </Text>
                  ) : null}
                  {!entry.voice && card?.kind_id !== 'file' ? (
                    <Text
                      selectable
                      style={{
                        fontSize: 16,
                        lineHeight: 23,
                        color: mine ? colors.onMine : colors.text,
                        fontStyle: entry.deleted ? 'italic' : 'normal',
                      }}
                    >
                      {entry.deleted ? 'Mensagem apagada' : text}
                    </Text>
                  ) : null}
                  <MessageMedia entry={entry} />
                  {entry.voice || card?.kind_id === 'file' ? (
                    <IconButton
                      label={`Ações da mensagem de ${entry.author.name}`}
                      name={{ ios: 'ellipsis', android: 'more_horiz', web: 'more_horiz' }}
                      onPress={() => setSelected(entry)}
                    />
                  ) : null}
                  {card && card.kind_id !== 'file' ? (
                    <Pressable
                      accessibilityRole="button"
                      accessibilityLabel={`Abrir ${card.title}`}
                      onPress={() =>
                        router.push({ pathname: '/item/[id]', params: { id: card.item_id } })
                      }
                      style={{
                        padding: 14,
                        marginTop: 10,
                        borderRadius: 16,
                        backgroundColor: colors.surface,
                      }}
                    >
                      <Text
                        style={{
                          color: colors.accent,
                          fontSize: 12,
                          fontWeight: '700',
                          marginBottom: 4,
                        }}
                      >
                        {card.kind_label}
                      </Text>
                      <Text style={{ fontSize: 18, fontWeight: '600', color: colors.text }}>
                        {card.title}
                      </Text>
                      <Text
                        numberOfLines={3}
                        style={{ color: colors.secondary, lineHeight: 20, marginTop: 5 }}
                      >
                        {card.summary}
                      </Text>
                    </Pressable>
                  ) : null}
                  <View
                    style={{
                      flexDirection: 'row',
                      justifyContent: 'flex-end',
                      alignItems: 'center',
                      gap: 5,
                      marginTop: 5,
                    }}
                  >
                    <Text
                      style={{
                        color: mine ? colors.onMine : colors.secondary,
                        opacity: 0.65,
                        fontSize: 10,
                      }}
                    >
                      {entry.edited ? 'editada · ' : ''}
                      {new Date(Number(entry.at_ms)).toLocaleTimeString([], {
                        hour: '2-digit',
                        minute: '2-digit',
                      })}
                    </Text>
                    {mine ? (
                      <Text
                        style={{
                          color: entry.delivery === 'Read' ? colors.read : colors.onMine,
                          fontSize: 11,
                        }}
                      >
                        {entry.delivery === 'Sending'
                          ? '◷'
                          : entry.delivery === 'Failed'
                            ? '!'
                            : ['Read', 'Delivered'].includes(entry.delivery)
                              ? '✓✓'
                              : '✓'}
                      </Text>
                    ) : null}
                  </View>
                </Pressable>
                {entry.reactions.length ? (
                  <View style={{ flexDirection: 'row', gap: 5, marginTop: 4 }}>
                    {entry.reactions.map((reaction) => (
                      <Pressable
                        key={reaction.emoji}
                        accessibilityRole="button"
                        accessibilityLabel={`${reaction.emoji}, ${reaction.people.length} pessoas`}
                        onPress={() => {
                          void store
                            .request({
                              op: 'react',
                              space_id: id,
                              message_id: entry.id,
                              emoji: reaction.mine ? null : reaction.emoji,
                            })
                            .catch(() => {});
                        }}
                        style={{
                          borderRadius: 14,
                          backgroundColor: colors.muted,
                          borderWidth: reaction.mine ? 1 : 0,
                          borderColor: colors.accent,
                          paddingHorizontal: 8,
                          paddingVertical: 4,
                        }}
                      >
                        <Text style={{ fontSize: 13, color: colors.text }}>
                          {reaction.emoji} {reaction.people.length}
                        </Text>
                      </Pressable>
                    ))}
                  </View>
                ) : null}
                {entry.thread_replies > 0 ? (
                  <Pressable
                    accessibilityRole="button"
                    accessibilityLabel={`Ver ${entry.thread_replies} respostas`}
                    onPress={() =>
                      router.push({
                        pathname: '/thread/[space]/[root]',
                        params: { space: id, root: entry.id },
                      })
                    }
                    style={{ paddingHorizontal: 12, paddingVertical: 10 }}
                  >
                    <Text style={{ color: colors.accent, fontSize: 13, fontWeight: '600' }}>
                      {entry.thread_replies} {entry.thread_replies === 1 ? 'resposta' : 'respostas'}{' '}
                      →
                    </Text>
                  </Pressable>
                ) : null}
              </View>
            );
          }}
        />
      )}
      {reply || editing ? (
        <View
          style={{
            marginHorizontal: 16,
            paddingHorizontal: 14,
            paddingVertical: 10,
            borderRadius: 18,
            backgroundColor: colors.muted,
            flexDirection: 'row',
            alignItems: 'center',
          }}
        >
          <View style={{ flex: 1, gap: 4 }}>
            <Text style={{ color: colors.accent, fontSize: 12, fontWeight: '600' }}>
              {editing ? 'Editar mensagem' : `Respondendo a ${reply?.author.name}`}
            </Text>
            <Text numberOfLines={1} style={{ color: colors.secondary, fontSize: 13 }}>
              {messageText((editing ?? reply)!)}
            </Text>
          </View>
          <IconButton
            label="Cancelar"
            name={{ ios: 'xmark', android: 'close', web: 'close' }}
            onPress={() => {
              if (editing) {
                setEditing(null);
                setEditDraft('');
              } else setReply(null);
            }}
          />
        </View>
      ) : null}
      <View
        style={{
          paddingHorizontal: 12,
          paddingTop: 8,
          paddingBottom: Math.max(insets.bottom, 12),
          flexDirection: 'row',
          gap: 8,
          alignItems: 'flex-end',
          backgroundColor: colors.surface,
        }}
      >
        {!editing ? (
          <MediaComposer
            spaceId={id}
            replyTo={reply?.id ?? null}
            disabled={sending}
            onSent={() => setReply(null)}
          />
        ) : null}
        <View
          style={{
            flex: 1,
            flexDirection: 'row',
            alignItems: 'flex-end',
            minHeight: 48,
            paddingHorizontal: 15,
            borderRadius: 25,
            borderCurve: 'continuous',
            backgroundColor: colors.muted,
          }}
        >
          <TextInput
            ref={input}
            testID="message-input"
            accessibilityLabel={editing ? 'Editar mensagem' : 'Mensagem'}
            placeholder="Mensagem"
            placeholderTextColor={colors.tertiary}
            multiline
            value={composerText}
            onChangeText={editing ? setEditDraft : setDraft}
            style={{
              flex: 1,
              maxHeight: 160,
              minHeight: 48,
              paddingTop: 13,
              paddingBottom: 12,
              color: colors.text,
              fontSize: 16,
              lineHeight: 22,
            }}
          />
        </View>
        <Pressable
          testID="send-message"
          accessibilityRole="button"
          accessibilityLabel={editing ? 'Salvar edição' : 'Enviar mensagem'}
          disabled={!composerText.trim() || sending}
          onPress={() => {
            void send();
          }}
          style={{
            width: 48,
            height: 48,
            borderRadius: 24,
            backgroundColor: composerText.trim() ? colors.accent : colors.muted,
            alignItems: 'center',
            justifyContent: 'center',
            opacity: sending ? 0.5 : 1,
          }}
        >
          <Icon
            name={{ ios: 'arrow.up', android: 'arrow_upward', web: 'arrow_upward' }}
            color={composerText.trim() ? colors.onAccent : colors.secondary}
          />
        </Pressable>
      </View>
      <Host colorScheme={scheme} seedColor={colors.accent}>
        <BottomSheet isPresented={selected !== null} onDismiss={() => setSelected(null)}>
          <Column spacing={8}>
            <Button
              label="Responder"
              variant="text"
              onPress={() => {
                setEditing(null);
                setReply(selected);
                setSelected(null);
                input.current?.focus();
              }}
            />
            <Button
              label="Abrir respostas"
              variant="text"
              onPress={() => {
                if (selected)
                  router.push({
                    pathname: '/thread/[space]/[root]',
                    params: { space: id, root: selected.id },
                  });
                setSelected(null);
              }}
            />
            {selected && !selected.deleted ? (
              <Button
                label="Encaminhar"
                variant="text"
                onPress={() => {
                  router.push({
                    pathname: '/forward/[space]/[message]',
                    params: { space: id, message: selected.id },
                  });
                  setSelected(null);
                }}
              />
            ) : null}
            <Button
              label="Copiar texto"
              variant="text"
              onPress={() => {
                if (selected) void Clipboard.setStringAsync(messageText(selected));
                setSelected(null);
              }}
            />
            <Button
              label="Curtir ♥"
              variant="text"
              onPress={() => {
                if (selected)
                  void store
                    .request({ op: 'react', space_id: id, message_id: selected.id, emoji: '❤️' })
                    .catch(() => {});
                setSelected(null);
              }}
            />
            {selected?.author.is_me &&
            !selected.deleted &&
            !selected.voice &&
            'Message' in selected.kind &&
            !selected.kind.Message.card ? (
              <Button
                label="Editar"
                variant="text"
                onPress={() => {
                  setEditing(selected);
                  setEditDraft(messageText(selected));
                  setSelected(null);
                  input.current?.focus();
                }}
              />
            ) : null}
            {selected?.author.is_me && !selected.deleted ? (
              <Button
                label="Apagar para todos"
                variant="text"
                onPress={() => {
                  const entry = selected;
                  setSelected(null);
                  void confirm({
                    title: 'Apagar mensagem?',
                    message: 'A mensagem será removida para todas as pessoas desta conversa.',
                    confirmLabel: 'Apagar',
                    destructive: true,
                  }).then((accepted) => {
                    if (accepted)
                      void store
                        .request({ op: 'delete_message', space_id: id, message_id: entry.id })
                        .catch(() => {});
                  });
                }}
              />
            ) : null}
          </Column>
        </BottomSheet>
      </Host>
    </KeyboardAvoidingView>
  );
}
