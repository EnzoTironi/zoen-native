import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AppState, Text, View } from 'react-native';
import {
  KeyboardAwareScrollView,
  type KeyboardAwareScrollViewRef,
} from 'react-native-keyboard-controller';
import { Stack, router, useFocusEffect, usePreventRemove } from 'expo-router';
import { randomUUID } from 'expo-crypto';
import { useDialogs } from '../components/dialogs';
import { Field, Loading, NativeButton } from '../components/primitives';
import { useClient } from '../core/provider';
import { canConsumeAcceptedMediaReply } from '../core/message-draft';
import { homeOwnerFor, homeSpacesFor } from '../home/access';
import { t } from '../i18n';
import { useTheme } from '../theme';
import type { ConversationAttachmentCategory } from './attachment-categories';
import { captureArticleDraft, stageContactAttachment } from './attachment-route-actions';
import { articleText, type SharedContact } from './attachment-content';
import { ArticleEditor } from './article-editor';
import type { ArticleEditorHandle } from './article-editor.types';
import { chooseSharedContact, prepareContactFile } from './attachment-route-platform';
import { sensitiveCacheWork } from './cache-work';
import { LocationPicker } from './location-picker';
import { PollAttachmentEditor } from './poll-attachment-editor';
import { captureMediaReply, type MediaReplyTarget } from './media-reply';

import type { AttachmentDestination } from './attachment-route-types';

const noSubscribe = () => () => {};
const noSnapshot = () => null;

export function AttachmentRoute({
  kind,
  destination: target,
}: {
  kind: ConversationAttachmentCategory;
  destination: AttachmentDestination;
}) {
  const dialogs = useDialogs();
  const client = useClient();
  const { store } = client;
  const mediaDriver = store.driver.media;
  const { colors } = useTheme();
  const channel = useMemo(
    () =>
      client.phase === 'ready' && client.snapshot
        ? store.messageDrafts.channel(target.id, target.thread)
        : null,
    [store, target.id, target.thread, client.snapshot, client.phase],
  );
  const media = useMemo(
    () =>
      client.phase === 'ready' && client.snapshot
        ? store.mediaDrafts.channel(target.id, target.thread)
        : null,
    [store, target.id, target.thread, client.snapshot, client.phase],
  );
  const normal = useSyncExternalStore(
    channel?.controller.subscribe ?? noSubscribe,
    channel ? () => channel.controller.getSnapshot(channel.scope) : noSnapshot,
    noSnapshot,
  );
  const input = useRef<ArticleEditorHandle>(null);
  const scroll = useRef<KeyboardAwareScrollViewRef>(null);
  const routeClosed = useRef(false);
  const editLease = useRef(0);
  const mounted = useRef(true),
    focused = useRef(false),
    foreground = useRef(AppState.currentState === 'active');
  const working = useRef(false),
    contactAttempted = useRef(false);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null),
    [leaving, setLeaving] = useState(false);
  const [source, setSource] = useState(false);
  const [pickerActivation, setPickerActivation] = useState<number | null>(null);
  const editorGeneration = useMemo(() => ({ source }), [source]);
  const liveEditor = useRef<object | null>(null);
  useLayoutEffect(() => {
    liveEditor.current = editorGeneration;
    return () => {
      if (liveEditor.current === editorGeneration) liveEditor.current = null;
    };
  }, [editorGeneration]);
  const [contact, setContact] = useState<(SharedContact & { reply: MediaReplyTarget }) | null>(
    null,
  );
  const [heldContact, setHeldContact] = useState(false);
  const allowed = useCallback(() => {
    const state = store.getSnapshot();
    const owner = homeOwnerFor(state);
    return Boolean(
      channel &&
      !store.isSessionRetiring &&
      state.phase === 'ready' &&
      owner?.mode === 'account' &&
      owner.identityId === target.identity &&
      owner.deviceId === target.device &&
      String(store.sessionEpoch) === target.epoch &&
      homeSpacesFor(state).has(target.id) &&
      store.messageDrafts.isCurrent(channel),
    );
  }, [channel, store, target]);
  const current = useCallback(
    () =>
      !routeClosed.current && mounted.current && focused.current && foreground.current && allowed(),
    [allowed],
  );
  // Explicit snapshot dependency: do not memoize a mutable controller's initial state as route admission.
  const renderOwner = homeOwnerFor(client);
  const available = Boolean(
    channel &&
    client.phase === 'ready' &&
    renderOwner?.mode === 'account' &&
    renderOwner.identityId === target.identity &&
    renderOwner.deviceId === target.device &&
    String(store.sessionEpoch) === target.epoch &&
    homeSpacesFor(client).has(target.id),
  );
  useEffect(() => {
    mounted.current = true;
    if (channel && allowed()) void channel.controller.activate(channel.scope).catch(() => {});
    return () => {
      mounted.current = false;
      if (channel) void channel.controller.flush(channel.scope).catch(() => {});
    };
  }, [channel, allowed]);
  useFocusEffect(
    useCallback(() => {
      editLease.current++;
      focused.current = true;
      setPickerActivation(foreground.current ? editLease.current : null);
      return () => {
        editLease.current++;
        focused.current = false;
        setPickerActivation(null);
      };
    }, []),
  );
  useEffect(() => {
    const listener = AppState.addEventListener('change', (state) => {
      editLease.current++;
      foreground.current = state === 'active';
      setPickerActivation(focused.current && foreground.current ? editLease.current : null);
      if (state !== 'active' && channel && allowed())
        void channel.controller.flush(channel.scope).catch(() => {});
    });
    return () => listener.remove();
  }, [channel, allowed]);
  async function captureArticle() {
    if (!current() || !channel) throw new Error('retired');
    await captureArticleDraft(channel, () => input.current!.capture(), current);
  }
  async function perform(work: () => Promise<void>) {
    if (!current() || working.current) return;
    working.current = true;
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (reason) {
      if (mounted.current)
        setError(
          reason instanceof Error && reason.message === 'article-limit'
            ? t('attachmentTabs.limit')
            : t('attachmentTabs.failed'),
        );
    } finally {
      working.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  const disableGuard = usePreventRemove(
    kind === 'article' && available && Boolean(normal?.loaded) && !leaving,
    ({ repeat }) => {
      void perform(async () => {
        await captureArticle();
        routeClosed.current = true;
        setLeaving(true);
        repeat();
      });
    },
  );
  async function close() {
    if (kind === 'article' && normal?.loaded) await captureArticle();
    if (!current()) return;
    disableGuard();
    routeClosed.current = true;
    setLeaving(true);
    router.back();
  }
  async function sendArticle() {
    if (!channel) return;
    await captureArticle();
    if (!channel.controller.getSnapshot(channel.scope).text.trim()) return;
    await channel.controller.send(channel.scope);
    if (store.messageDrafts.isCurrent(channel)) void store.refresh().catch(() => {});
    if (current()) {
      disableGuard();
      routeClosed.current = true;
      setLeaving(true);
      router.back();
    }
  }
  function captureReply(): MediaReplyTarget {
    if (!channel || !current()) throw new Error('retired');
    const state = channel.controller.getSnapshot(channel.scope);
    if (!state.loaded) throw new Error('retired');
    return Object.freeze({ reply_to: state.replyTo, ...captureMediaReply(state) });
  }
  async function stageContact() {
    if (!contact || !media || !channel || !store.mediaDrafts.isCurrent(media)) return;
    if (contactAttempted.current) {
      await media.flush();
      if (current()) {
        disableGuard();
        routeClosed.current = true;
        setLeaving(true);
        router.back();
      }
      return;
    }
    await sensitiveCacheWork.run(() =>
      stageContactAttachment({
        media,
        contact,
        current: () => current() && store.mediaDrafts.isCurrent(media),
        replyTo: () => contact.reply.reply_to,
        reviewedReply: () => contact.reply,
        prepare: prepareContactFile,
        nextId: randomUUID,
        admitted: () => {
          contactAttempted.current = true;
          setHeldContact(true);
        },
      }),
    );
    if (current()) {
      disableGuard();
      routeClosed.current = true;
      setLeaving(true);
      router.back();
    }
  }
  const locked = busy || !available;
  const title = t(`attachmentTabs.${kind}`);
  return (
    <KeyboardAwareScrollView
      ref={scroll}
      onLayout={() => {
        // Detent/rotation relayouts can leave the keyboard open without a new focus event.
        if (current()) scroll.current?.assureFocusedInputVisible();
      }}
      bottomOffset={16}
      testID={`attachment-${kind}-route`}
      style={{ flex: 1 }}
      contentInsetAdjustmentBehavior="automatic"
      automaticallyAdjustKeyboardInsets={false}
      keyboardShouldPersistTaps="handled"
      contentContainerStyle={{
        padding: 16,
        paddingBottom: 48,
        gap: 18,
        flexGrow: 1,
        maxWidth: 820,
        width: '100%',
        alignSelf: 'center',
      }}
    >
      <Stack.Screen
        options={{
          title,
          headerLeft: () => (
            <NativeButton
              secondary
              label={t('attachmentTabs.close')}
              disabled={busy}
              onPress={() => {
                if (!available) {
                  disableGuard();
                  routeClosed.current = true;
                  setLeaving(true);
                  router.back();
                } else void perform(close);
              }}
            />
          ),
          ...(kind === 'article'
            ? {
                headerRight: () => (
                  <NativeButton
                    label={t('attachmentTabs.send')}
                    disabled={locked || !normal?.loaded || !normal.text.trim()}
                    onPress={() => {
                      void perform(sendArticle);
                    }}
                  />
                ),
              }
            : {}),
        }}
      />
      {!available ? (
        <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
          {t('attachmentTabs.unavailable')}
        </Text>
      ) : !normal?.loaded ? (
        <Loading />
      ) : (
        <>
          {kind === 'article' ? (
            <>
              <NativeButton
                secondary
                label={t(source ? 'attachmentTabs.visual' : 'attachmentTabs.source')}
                disabled={locked}
                onPress={() => {
                  void perform(async () => {
                    await captureArticle();
                    if (current()) setSource(!source);
                  });
                }}
              />
              <ArticleEditor
                key={String(source)}
                ref={input}
                initial={normal.text}
                source={source}
                disabled={locked}
                canEdit={() =>
                  current() && !working.current && liveEditor.current === editorGeneration
                }
                captureEditLease={() => {
                  const generation = editLease.current;
                  return () =>
                    generation === editLease.current &&
                    current() &&
                    !working.current &&
                    liveEditor.current === editorGeneration;
                }}
                onChange={(text) => {
                  if (
                    liveEditor.current !== editorGeneration ||
                    !current() ||
                    working.current ||
                    !channel
                  )
                    return;
                  try {
                    channel.controller.setText(channel.scope, articleText(text));
                    setError(null);
                  } catch {
                    setError(t('attachmentTabs.limit'));
                  }
                }}
              />
              <Text style={{ color: colors.secondary }}>{t('attachmentTabs.saved')}</Text>
            </>
          ) : null}
          {kind === 'location' && mediaDriver && pickerActivation !== null ? (
            <LocationPicker
              key={JSON.stringify([target, pickerActivation])}
              media={mediaDriver}
              channel={media}
              current={() =>
                current() &&
                editLease.current === pickerActivation &&
                store.driver.media === mediaDriver &&
                Boolean(media && store.mediaDrafts.isCurrent(media))
              }
              replyTo={() => channel?.controller.getSnapshot(channel.scope).replyTo ?? null}
              reviewedReply={captureReply}
              onStaged={() => {
                if (!current()) return;
                disableGuard();
                routeClosed.current = true;
                setLeaving(true);
                router.back();
              }}
            />
          ) : null}
          {kind === 'contact' ? (
            <>
              <NativeButton
                label={t('attachmentTabs.chooseContact')}
                disabled={locked || heldContact}
                onPress={() => {
                  void perform(async () => {
                    if (contactAttempted.current) return;
                    const reply = captureReply();
                    const value = await chooseSharedContact(current);
                    if (current() && value) setContact({ ...value, reply });
                  });
                }}
              />
              {contact ? (
                <>
                  <Text style={{ color: colors.secondary }}>
                    {t('attachmentTabs.contactPreview')}
                  </Text>
                  <Field
                    label={t('attachmentTabs.contactName')}
                    value={contact.name}
                    editable={!locked && !heldContact}
                    onChangeText={(name) => {
                      if (current() && !working.current && !contactAttempted.current)
                        setContact({ ...contact, name });
                    }}
                  />
                  {contact.phones.map((phone, index) => (
                    <Field
                      key={index}
                      label={`${t('attachmentTabs.phone')} ${index + 1}`}
                      value={phone}
                      keyboardType="phone-pad"
                      editable={!locked && !heldContact}
                      onChangeText={(value) => {
                        if (current() && !working.current && !contactAttempted.current)
                          setContact({
                            ...contact,
                            phones: contact.phones.map((old, position) =>
                              position === index ? value : old,
                            ),
                          });
                      }}
                    />
                  ))}
                  <NativeButton
                    label={t(
                      heldContact ? 'attachmentTabs.retryContact' : 'attachmentTabs.addContact',
                    )}
                    disabled={locked}
                    onPress={() => {
                      void perform(stageContact);
                    }}
                  />
                  {heldContact ? (
                    <Text style={{ color: colors.secondary }}>
                      {t('attachmentTabs.contactPending')}
                    </Text>
                  ) : null}
                </>
              ) : null}
            </>
          ) : null}
          {kind === 'poll' && media && pickerActivation !== null ? (
            <PollAttachmentEditor
              key={JSON.stringify([target, pickerActivation])}
              channel={media}
              reply={{ reply_to: normal.replyTo, ...captureMediaReply(normal) }}
              current={() =>
                current() &&
                editLease.current === pickerActivation &&
                store.mediaDrafts.isCurrent(media)
              }
              onCreated={() => {
                void store.refresh().catch(() => {});
                if (!current()) return;
                disableGuard();
                routeClosed.current = true;
                setLeaving(true);
                router.back();
              }}
              onReplyCancelled={(previous) => {
                if (!channel || !store.messageDrafts.isCurrent(channel)) return;
                const value = channel.controller.getSnapshot(channel.scope);
                if (
                  canConsumeAcceptedMediaReply(
                    {
                      reply_to: value.replyTo,
                      ...captureMediaReply(value),
                      editing: Boolean(value.edit),
                    },
                    previous,
                  )
                )
                  channel.controller.setReply(channel.scope, null);
              }}
            />
          ) : null}
        </>
      )}
      {normal?.error || error ? (
        <View style={{ gap: 12 }}>
          <Text selectable accessibilityRole="alert" style={{ color: colors.danger }}>
            {error ?? t('attachmentTabs.failed')}
          </Text>
          {normal?.error && channel ? (
            <NativeButton
              label={t('documents.tryAgain')}
              disabled={busy}
              onPress={() => {
                void perform(async () => {
                  await channel.controller.retry(channel.scope);
                });
              }}
            />
          ) : null}
          {kind === 'article' ? (
            <NativeButton
              secondary
              label={t('attachmentTabs.backWithoutCapture')}
              disabled={busy}
              onPress={() => {
                void perform(async () => {
                  const leave = await dialogs.confirm({
                    title: t('attachmentTabs.backWithoutCapture'),
                    message: t('attachmentTabs.unsavedCapture'),
                    confirmLabel: t('attachmentTabs.backWithoutCapture'),
                    cancelLabel: t('attachmentTabs.keepEditing'),
                    destructive: true,
                  });
                  if (!leave || !current()) return;
                  routeClosed.current = true;
                  disableGuard();
                  setLeaving(true);
                  router.back();
                });
              }}
            />
          ) : null}
        </View>
      ) : null}
    </KeyboardAwareScrollView>
  );
}
