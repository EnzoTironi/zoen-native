import { Text, View } from 'react-native';
import { NativeButton } from '../components/primitives';
import { t } from '../i18n';
import { AttachmentApproval, supportsMediaApproval } from './attachment-approval';
import type { AttachmentComposerField } from './attachment-composer';
import { MAX_ATTACHMENT_ITEMS, type AttachmentDraftState } from './attachment-draft';
import { AttachmentDraftStrip } from './attachment-draft-strip';
import { isLocationFileMime } from './location-file';
import { mediaPlayback } from './playback-owner';
import { copyMediaReply, sameMediaReply } from './media-reply';
import { renderPollDraftReview } from './poll-draft-review';

import type { useDurableAttachmentController } from './use-durable-attachment-controller';
export function renderDurableAttachment(
  context: ReturnType<typeof useDurableAttachmentController>,
) {
  const {
    runtime,
    state,
    content,
    own,
    reviewing,
    channel,
    options,
    current,
    failure,
    send,
    pick,
    photos,
    canReview,
    openApproval,
    review,
    publish,
    remove,
    cancel,
    colors,
    snapshot,
    store,
    closeApproval,
    edit,
    reorder,
    setRestoreEpoch,
  } = context;
  if (state?.content?.kind === 'poll')
    return {
      active: true,
      busy: true,
      field: undefined,
      pick,
      canAdd: false,
      review: renderPollDraftReview(context, state.content),
    };
  const presentation = runtime.approval;
  // A native-review writer freeze is not a pending Send.
  const retry = Boolean(state?.frozen && !state.reviewing);
  const restoring = !!content?.items.some((item) => !own?.previews.has(item.asset_id));
  const busy = Boolean(
    runtime.submitting ||
    reviewing.mutationId ||
    presentation?.closing ||
    own?.picking ||
    own?.clearing ||
    state?.sending ||
    restoring ||
    (state?.phase === 'loading' && !state.error),
  );
  const scope = channel
    ? {
        mode: 'account' as const,
        identityId: channel.scope.expected_identity_id,
        deviceId: channel.scope.expected_device_id,
        spaceId: channel.scope.space_id,
        replyTo: content?.reply_to ?? channel.scope.thread_root_id,
        ...(content?.reply_quote ? { replyQuote: content.reply_quote } : {}),
        ...(content?.shared_reply ? { sharedReply: content.shared_reply } : {}),
        ...(content?.reply_intent ? { replyIntent: content.reply_intent } : {}),
        thread: Boolean(channel.scope.thread_root_id),
      }
    : null;
  const items =
    content?.items.map((item) => {
      const asset =
        own?.previews.get(item.asset_id) ??
        state?.value?.assets.find((asset) => asset.id === item.asset_id);
      return {
        uri: own?.previews.get(item.asset_id)?.uri ?? '',
        name: asset?.name ?? '',
        mime: asset?.mime ?? 'application/octet-stream',
        size: asset?.size ?? 0,
        sha256: state?.value?.assets.find((asset) => asset.id === item.asset_id)?.sha256,
        mutationId: item.mutation_id,
        caption: item.caption,
        attempted: retry,
        accepted: null,
      };
    }) ?? [];
  const selectedIndex = Math.max(
    0,
    content?.items.findIndex((item) => item.asset_id === own?.selected) ?? 0,
  );
  const selected = items[selectedIndex];
  const field: AttachmentComposerField | undefined =
    selected && content && scope
      ? {
          mutationId: selected.mutationId,
          scope,
          targetFrozen: true,
          value: selected.caption,
          position: selectedIndex + 1,
          total: items.length,
          editable:
            !options.disabled &&
            !options.suspended &&
            !reviewing.mutationId &&
            !state?.frozen &&
            !restoring,
          sending: busy,
          remaining: items.length,
          retry,
          onCancelReply:
            !busy &&
            !presentation &&
            !state?.frozen &&
            !options.disabled &&
            !options.suspended &&
            (content.reply_quote ||
              content.shared_reply ||
              (content.reply_to && !channel?.scope.thread_root_id))
              ? () => {
                  if (
                    !channel ||
                    !current() ||
                    runtime.picking ||
                    runtime.submitting ||
                    runtime.approval
                  )
                    return null;
                  const latest = channel.getSnapshot();
                  const next = latest.content;
                  if (
                    latest.frozen ||
                    latest.sending ||
                    next?.kind !== 'attachments' ||
                    next.reply_to !== content.reply_to ||
                    !sameMediaReply(next, content)
                  )
                    return null;
                  const previous = { reply_to: next.reply_to, ...copyMediaReply(next) };
                  void channel
                    .save({
                      ...next,
                      reply_to: null,
                      reply_quote: undefined,
                      shared_reply: undefined,
                      reply_intent: undefined,
                    })
                    .catch(failure);
                  return previous;
                }
              : undefined,
          onChange: (caption) => {
            if (
              !channel ||
              !current() ||
              // Photos anchors to this editable native field. While its overlay
              // owns interaction, even a retained callback cannot change the draft.
              runtime.picking ||
              state?.frozen ||
              runtime.approval !== presentation ||
              presentation?.closing
            )
              return;
            const next = channel.getSnapshot().content;
            if (next?.kind !== 'attachments') return;
            void channel
              .save({
                ...next,
                ...(next.gallery ? { gallery: { ...next.gallery, forced_caption: false } } : {}),
                items: next.items.map((item) =>
                  item.mutation_id === selected.mutationId ? { ...item, caption } : item,
                ),
              })
              .catch(failure);
          },
          onSend: (finalCaption) => {
            void send(
              content,
              presentation,
              finalCaption === undefined
                ? undefined
                : { mutationId: selected.mutationId, text: finalCaption },
            );
          },
        }
      : undefined;
  const reviewFailed = !own?.error && !state?.error && Boolean(reviewing.error);
  const error =
    own?.error ??
    state?.error?.message ??
    (reviewFailed ? t('media.attachmentPreviewFailed') : null);
  const strip: AttachmentDraftState | null =
    scope && content
      ? {
          scope,
          items,
          targetFrozen: true,
          phase: state?.sending ? 'sending' : own?.picking || restoring ? 'picking' : 'ready',
          error: null,
        }
      : null;
  return {
    active: Boolean(content?.items.length),
    busy,
    field,
    pick,
    canAdd: Boolean(
      !reviewing.mutationId &&
      state?.phase === 'ready' &&
      !state.frozen &&
      (!state.content || state.content.kind === 'attachments') &&
      items.length < MAX_ATTACHMENT_ITEMS,
    ),
    review: (
      <>
        {!(strip && field && presentation && own?.approval === presentation) ? photos.view : null}
        {strip || (!state?.content && error) ? (
          <View style={{ gap: 8 }}>
            {strip ? (
              <AttachmentDraftStrip
                state={strip}
                selectedId={selected?.mutationId ?? null}
                busy={Boolean(
                  reviewing.mutationId ||
                  own?.picking ||
                  own?.clearing ||
                  state?.sending ||
                  (restoring && !error),
                )}
                error={error}
                onPreview={
                  canReview
                    ? (id) => {
                        const item = items.find((item) => item.mutationId === id);
                        if (
                          supportsMediaApproval &&
                          item &&
                          (/^(image|video)\//.test(item.mime) || isLocationFileMime(item.mime))
                        )
                          openApproval(id);
                        else void review.open(id);
                      }
                    : undefined
                }
                onSelect={(id) =>
                  publish({
                    selected:
                      content?.items.find((item) => item.mutation_id === id)?.asset_id ?? null,
                  })
                }
                onRemove={(id) => {
                  const item = content?.items.find((item) => item.mutation_id === id);
                  if (item) void remove(item.asset_id);
                }}
                onCancel={() => {
                  void cancel();
                }}
              />
            ) : (
              <Text accessibilityRole="alert" style={{ color: colors.danger }}>
                {error}
              </Text>
            )}
            {strip && field && presentation && own?.approval === presentation ? (
              <AttachmentApproval
                grouped={content?.options?.grouping === 'grouped'}
                libraryPicker={photos.view}
                testID="attachment-approval"
                state={strip}
                field={field}
                selectedId={selected?.mutationId ?? null}
                recipient={
                  snapshot?.conversations.find((chat) => chat.id === channel?.scope.space_id)
                    ?.title ?? t('media.attachmentConversation')
                }
                busy={busy}
                editing={Boolean(reviewing.mutationId)}
                suspended={Boolean(own?.picking || reviewing.mutationId)}
                canAdd={!state?.frozen && !busy && items.length < MAX_ATTACHMENT_ITEMS}
                canEdit={Boolean(
                  canReview &&
                  store.driver.media?.drafts?.edit &&
                  !state?.frozen &&
                  selected &&
                  /^(image\/(jpeg|png|heic|heif)|video\/(mp4|quicktime|x-m4v))$/.test(
                    selected.mime,
                  ),
                )}
                error={error}
                isCurrent={() =>
                  Boolean(
                    current() &&
                    runtime.foreground &&
                    runtime.approval === presentation &&
                    !presentation.closing &&
                    !runtime.picking &&
                    !review.getSnapshot().mutationId &&
                    !channel?.getSnapshot().sending,
                  )
                }
                onClose={() => {
                  void closeApproval();
                }}
                onSelect={(id) => {
                  if (
                    !current() ||
                    runtime.approval !== presentation ||
                    presentation.closing ||
                    busy
                  )
                    return;
                  mediaPlayback.pauseAll();
                  publish({
                    selected:
                      content?.items.find((item) => item.mutation_id === id)?.asset_id ?? null,
                  });
                }}
                onEdit={(finalCaption) => {
                  if (runtime.approval === presentation && !presentation.closing && selected)
                    void edit(selected.mutationId, presentation, finalCaption);
                }}
                onReorder={(ids) => {
                  if (runtime.approval === presentation && !presentation.closing) void reorder(ids);
                }}
                onAdd={(kind) => {
                  if (runtime.approval === presentation && !presentation.closing) void pick(kind);
                }}
                onRemove={(id) => {
                  if (runtime.approval !== presentation || presentation.closing) return;
                  const item = content?.items.find((item) => item.mutation_id === id);
                  if (item) void remove(item.asset_id);
                }}
                onRetry={() => {
                  if (runtime.approval !== presentation || presentation.closing) return;
                  publish({ error: null });
                  void channel
                    ?.retry()
                    .then(() => {
                      if (!runtime.closed) setRestoreEpoch((value) => value + 1);
                    })
                    .catch(failure);
                }}
              />
            ) : null}
            {error && channel ? (
              <NativeButton
                testID="retry-media-draft"
                secondary
                label={t('chat.tryAgain2')}
                onPress={() => {
                  if (reviewFailed && selected) {
                    void review.open(selected.mutationId);
                    return;
                  }
                  publish({ error: null });
                  void channel
                    .retry()
                    .then(() => {
                      if (!runtime.closed) setRestoreEpoch((value) => value + 1);
                    })
                    .catch(failure);
                }}
              />
            ) : null}
          </View>
        ) : null}
      </>
    ),
  };
}
