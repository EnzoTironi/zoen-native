import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { AppState, Linking, ScrollView, Text, View } from 'react-native';
import { Stack, router, useFocusEffect, useLocalSearchParams, usePreventRemove } from 'expo-router';
import { randomUUID } from 'expo-crypto';
import { useDialogs } from '../../components/dialogs';
import { Field, Loading, NativeButton } from '../../components/primitives';
import { useClient } from '../../core/provider';
import { homeOwnerFor, homeSpacesFor } from '../../home/access';
import { t } from '../../i18n';
import { useTheme } from '../../theme';
import {
  conversationAttachmentCategory,
  type ConversationAttachmentCategory,
} from '../../media/attachment-categories';
import { captureArticleDraft, stageContactAttachment } from '../../media/attachment-route-actions';
import {
  appendAttachmentText,
  articleText,
  pollArguments,
  type SharedContact,
} from '../../media/attachment-content';
import { ArticleEditor } from '../../media/article-editor';
import type { ArticleEditorHandle } from '../../media/article-editor.types';
import {
  chooseSharedContact,
  chooseStaticLocation,
  prepareContactFile,
} from '../../media/attachment-route-platform';
import { sensitiveCacheWork } from '../../media/cache-work';

const noSubscribe = () => () => {};
const noSnapshot = () => null;
type Destination = {
  id: string;
  thread: string | null;
  identity: string;
  device: string;
  epoch: string;
};

/** One route pattern; all writes still go through the existing canonical text/media/App owners. */
export default function ChatAttachment() {
  const params = useLocalSearchParams<{
    id: string;
    kind: string;
    thread?: string;
    identity: string;
    device: string;
    epoch: string;
  }>();
  const kind = conversationAttachmentCategory(params.kind);
  const destination: Destination = {
    id: params.id,
    thread: params.thread || null,
    identity: params.identity,
    device: params.device,
    epoch: params.epoch,
  };
  if (
    !kind ||
    Object.values(destination).some((value) => value !== null && typeof value !== 'string') ||
    (kind === 'poll' && destination.thread)
  )
    return <Text accessibilityRole="alert">{t('attachmentTabs.unavailable')}</Text>;
  return (
    <AttachmentRoute
      key={JSON.stringify([kind, destination])}
      kind={kind}
      destination={destination}
    />
  );
}
export function AttachmentRoute({
  kind,
  destination: target,
}: {
  kind: ConversationAttachmentCategory;
  destination: Destination;
}) {
  const dialogs = useDialogs();
  const client = useClient();
  const { store } = client;
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
  const routeClosed = useRef(false);
  const mounted = useRef(true),
    focused = useRef(false),
    foreground = useRef(AppState.currentState === 'active');
  const locationAttempted = useRef(false);
  const [heldLocation, setHeldLocation] = useState(false);
  const working = useRef(false),
    pollAttempted = useRef(false),
    contactAttempted = useRef(false);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState<string | null>(null),
    [leaving, setLeaving] = useState(false);
  const [source, setSource] = useState(false),
    [location, setLocation] = useState<string | null>(null);
  const editorGeneration = useMemo(() => ({ source }), [source]);
  const liveEditor = useRef<object | null>(null);
  useLayoutEffect(() => {
    liveEditor.current = editorGeneration;
    return () => {
      if (liveEditor.current === editorGeneration) liveEditor.current = null;
    };
  }, [editorGeneration]);
  const [contact, setContact] = useState<SharedContact | null>(null),
    [question, setQuestion] = useState(''),
    [options, setOptions] = useState(['', '']);
  const [unknownPoll, setUnknownPoll] = useState(false),
    [heldContact, setHeldContact] = useState(false);
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
      focused.current = true;
      return () => {
        focused.current = false;
      };
    }, []),
  );
  useEffect(() => {
    const listener = AppState.addEventListener('change', (state) => {
      foreground.current = state === 'active';
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
        replyTo: () => channel.controller.getSnapshot(channel.scope).replyTo,
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
  async function createPoll() {
    if (pollAttempted.current) return;
    let args;
    try {
      args = pollArguments(question, options);
    } catch {
      setError(t('attachmentTabs.pollLimit'));
      return;
    }
    if (!current()) return;
    pollAttempted.current = true;
    try {
      await store.request({
        op: 'install_app',
        space_id: target.id,
        app_id: 'poll',
        args_json: JSON.stringify(args),
      });
    } catch {
      if (mounted.current) setUnknownPoll(true);
      throw new Error('poll-unknown');
    }
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
    <ScrollView
      testID={`attachment-${kind}-route`}
      contentInsetAdjustmentBehavior="automatic"
      automaticallyAdjustKeyboardInsets
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
          {kind === 'location' ? (
            <>
              <Text style={{ color: colors.secondary }}>{t('attachmentTabs.staticOnly')}</Text>
              <NativeButton
                label={t('attachmentTabs.chooseLocation')}
                disabled={locked || heldLocation}
                onPress={() => {
                  void perform(async () => {
                    if (locationAttempted.current) return;
                    const value = await chooseStaticLocation(current);
                    if (current() && value) setLocation(value);
                  });
                }}
              />
              {location ? (
                <View style={{ gap: 12 }}>
                  <Text selectable style={{ color: colors.text }}>
                    {location}
                  </Text>
                  <NativeButton
                    secondary
                    label={t('attachmentTabs.preview')}
                    disabled={locked}
                    onPress={() => {
                      if (current()) void Linking.openURL(location).catch(() => {});
                    }}
                  />
                  <NativeButton
                    label={t('attachmentTabs.addToMessage')}
                    disabled={locked}
                    onPress={() => {
                      void perform(async () => {
                        if (!channel) return;
                        if (!locationAttempted.current) {
                          channel.controller.setText(
                            channel.scope,
                            appendAttachmentText(
                              channel.controller.getSnapshot(channel.scope).text,
                              location,
                            ),
                          );
                          locationAttempted.current = true;
                          setHeldLocation(true);
                        }
                        await channel.controller.flush(channel.scope);
                        if (current()) {
                          routeClosed.current = true;
                          setLeaving(true);
                          router.back();
                        }
                      });
                    }}
                  />
                </View>
              ) : null}
            </>
          ) : null}
          {kind === 'contact' ? (
            <>
              <NativeButton
                label={t('attachmentTabs.chooseContact')}
                disabled={locked || heldContact}
                onPress={() => {
                  void perform(async () => {
                    if (contactAttempted.current) return;
                    const value = await chooseSharedContact(current);
                    if (current() && value) setContact(value);
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
          {kind === 'poll' ? (
            <>
              <Field
                label={t('attachmentTabs.question')}
                value={question}
                editable={!locked && !unknownPoll}
                onChangeText={(value) => {
                  if (current() && !working.current && !pollAttempted.current) setQuestion(value);
                }}
              />
              {options.map((option, index) => (
                <View key={index} style={{ gap: 8 }}>
                  <Field
                    label={t('attachmentTabs.option', { number: index + 1 })}
                    value={option}
                    editable={!locked && !unknownPoll}
                    onChangeText={(value) => {
                      if (current() && !working.current && !pollAttempted.current)
                        setOptions(
                          options.map((old, position) => (position === index ? value : old)),
                        );
                    }}
                  />
                  {options.length > 2 ? (
                    <NativeButton
                      secondary
                      label={t('attachmentTabs.removeOption', { number: index + 1 })}
                      disabled={locked || unknownPoll}
                      onPress={() => {
                        if (current() && !working.current && !pollAttempted.current)
                          setOptions(options.filter((_, position) => position !== index));
                      }}
                    />
                  ) : null}
                </View>
              ))}
              <NativeButton
                secondary
                label={t('attachmentTabs.addOption')}
                disabled={locked || unknownPoll || options.length >= 10}
                onPress={() => {
                  if (
                    current() &&
                    !working.current &&
                    !pollAttempted.current &&
                    options.length < 10
                  )
                    setOptions([...options, '']);
                }}
              />
              <NativeButton
                label={t('attachmentTabs.createPoll')}
                disabled={locked || unknownPoll}
                onPress={() => {
                  void perform(createPoll);
                }}
              />
              {unknownPoll ? (
                <Text accessibilityRole="alert" style={{ color: colors.secondary }}>
                  {t('attachmentTabs.pollUnknown')}
                </Text>
              ) : null}
            </>
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
    </ScrollView>
  );
}
