import { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, Text, View } from 'react-native';
import { Image } from 'expo-image';
import { createVideoPlayer, type VideoThumbnail } from 'expo-video';
import * as Haptics from 'expo-haptics';
import { ComposerAction } from '../components/composer-action';
import { Icon } from '../components/primitives';
import { formatLocale, t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import type { AttachmentDraftItem, AttachmentDraftState } from './attachment-draft';
import { sensitiveCacheWork } from './cache-work';

// Extract small native images serially; a selection never opens ten video decoders together.
let thumbnailQueue: Promise<unknown> = Promise.resolve();
function DraftThumbnail({ item }: { item: AttachmentDraftItem }) {
  const { colors, scheme } = useTheme();
  const [thumbnail, setThumbnail] = useState<VideoThumbnail | null>(null);
  const video = item.mime.startsWith('video/');
  const photo = item.mime.startsWith('image/');
  useEffect(() => {
    if (!video || item.accepted) return;
    let live = true;
    const task = sensitiveCacheWork.run(() => {
      const work = thumbnailQueue.then(async () => {
        if (!live) return null;
        const player = createVideoPlayer({ uri: item.uri, useCaching: false });
        try {
          const images = await player.generateThumbnailsAsync(0, { maxWidth: 192, maxHeight: 192 });
          return live ? (images[0] ?? null) : null;
        } finally {
          player.release();
        }
      });
      thumbnailQueue = work.catch(() => {});
      return work;
    });
    void task
      .then((image) => {
        if (live) setThumbnail(image);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [item.uri, item.accepted, video]);
  return (
    <View style={{ flex: 1, alignItems: 'center', justifyContent: 'center', overflow: 'hidden' }}>
      {!item.accepted && (photo || thumbnail) ? (
        <Image
          source={photo ? { uri: item.uri } : thumbnail}
          cachePolicy="none"
          recyclingKey={item.mutationId}
          contentFit="cover"
          style={{ position: 'absolute', inset: 0 }}
        />
      ) : (
        <Icon
          name={
            item.accepted
              ? { ios: 'checkmark', android: 'check', web: 'check' }
              : video
                ? { ios: 'video', android: 'videocam', web: 'videocam' }
                : { ios: 'doc.fill', android: 'description', web: 'description' }
          }
          size={24}
          color={colors.secondary}
        />
      )}
      {video && thumbnail && !item.accepted ? (
        <View style={{ position: 'absolute', bottom: 5, left: 6 }}>
          <Icon
            name={{ ios: 'video.fill', android: 'videocam', web: 'videocam' }}
            color="#fff"
            size={15}
          />
        </View>
      ) : null}
      {photo || thumbnail ? (
        <View
          pointerEvents="none"
          style={{
            position: 'absolute',
            inset: 0,
            borderRadius: 14,
            borderWidth: 1,
            borderColor: scheme === 'dark' ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.1)',
          }}
        />
      ) : null}
    </View>
  );
}

export function AttachmentDraftStrip({
  state,
  selectedId,
  busy,
  error,
  onSelect,
  onRemove,
  onCancel,
}: {
  state: AttachmentDraftState;
  selectedId: string | null;
  busy: boolean;
  error: string | null;
  onSelect: (id: string) => void;
  onRemove: (id: string) => void;
  onCancel: () => void;
}) {
  const { colors } = useTheme();
  const selected = state.items.find((item) => item.mutationId === selectedId);
  const accepted = state.items.filter((item) => item.accepted).length;
  const sending = state.phase === 'sending';
  return (
    <View testID="attachment-draft-strip" style={{ gap: 6 }}>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
        {busy ? <ActivityIndicator color={colors.secondary} size="small" /> : null}
        <Text
          accessibilityLiveRegion="polite"
          style={{ flex: 1, color: colors.secondary, fontSize: 12 }}
        >
          {sending
            ? t('media.attachmentSending', {
                position: Math.min(accepted + 1, state.items.length),
                total: state.items.length,
              })
            : state.phase === 'picking'
              ? t('media.attachmentPreparing')
              : t('media.attachmentDraftCount', { count: state.items.length })}
        </Text>
      </View>
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        keyboardShouldPersistTaps="handled"
        contentContainerStyle={{ gap: 8, paddingBottom: 2 }}
      >
        {state.items.map((item, index) => (
          <Pressable
            key={item.mutationId}
            testID={`attachment-draft-${index}`}
            accessibilityRole="button"
            accessibilityLabel={`${t('media.attachmentPosition', { position: index + 1, total: state.items.length, name: item.name })}${item.accepted ? `, ${t('media.attachmentSent')}` : ''}`}
            accessibilityHint={t('media.attachmentSelect', { name: item.name })}
            accessibilityState={{ selected: selectedId === item.mutationId, disabled: busy }}
            disabled={busy}
            onPress={() => {
              if (selectedId !== item.mutationId && process.env.EXPO_OS === 'ios')
                void Haptics.selectionAsync().catch(() => {});
              onSelect(item.mutationId);
            }}
            style={({ pressed }) => ({
              width: 88,
              height: 88,
              padding: 3,
              borderWidth: 2,
              borderColor: selectedId === item.mutationId ? colors.accent : 'transparent',
              borderRadius: 19,
              opacity: pressed ? 0.7 : 1,
            })}
          >
            <View
              style={{
                flex: 1,
                backgroundColor: colors.surface,
                borderRadius: 14,
                overflow: 'hidden',
              }}
            >
              <DraftThumbnail item={item} />
              {!item.mime.startsWith('image/') && !item.mime.startsWith('video/') ? (
                <Text
                  numberOfLines={1}
                  style={{ color: colors.text, fontSize: 10, padding: 5, textAlign: 'center' }}
                >
                  {item.name}
                </Text>
              ) : null}
              {item.caption && !item.accepted ? (
                <View
                  pointerEvents="none"
                  style={{
                    position: 'absolute',
                    right: 4,
                    bottom: 4,
                    borderRadius: 8,
                    padding: 3,
                    backgroundColor: colors.background,
                  }}
                >
                  <Icon
                    name={{ ios: 'text.bubble.fill', android: 'chat', web: 'chat' }}
                    color={colors.text}
                    size={12}
                  />
                </View>
              ) : null}
            </View>
          </Pressable>
        ))}
      </ScrollView>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 8 }}>
        <View style={{ flex: 1, minWidth: 0 }}>
          <Text numberOfLines={1} style={{ color: colors.secondary, fontSize: 12 }}>
            {selected?.name ?? t('media.attachmentDraftHint')}
          </Text>
          <Text style={{ color: colors.tertiary, fontSize: 11 }}>
            {selected
              ? selected.accepted
                ? t('media.attachmentSent')
                : `${new Intl.NumberFormat(formatLocale, { maximumFractionDigits: 1 }).format(selected.size / 1048576)} MB`
              : ''}
          </Text>
        </View>
        {selected && (!selected.attempted || selected.accepted) ? (
          <ComposerAction
            name="xmark"
            size={44}
            label={t('media.attachmentRemove', { name: selected.name })}
            disabled={busy}
            onPress={() => onRemove(selected.mutationId)}
            testID="remove-selected-attachment"
          />
        ) : null}
        <ComposerAction
          name="trash"
          size={44}
          label={t('media.attachmentsCancel')}
          disabled={busy}
          onPress={onCancel}
          testID="discard-attachment-draft"
        />
      </View>
      {error ? (
        <Text
          accessibilityRole="alert"
          selectable
          style={{ color: colors.danger, fontSize: 12, lineHeight: 17 }}
        >
          {error}
        </Text>
      ) : null}
      {selected?.attempted && !selected.accepted && !sending ? (
        <Text style={{ color: colors.secondary, fontSize: 12, lineHeight: 17 }}>
          {t('media.attachmentRetryLocked')}
        </Text>
      ) : null}
    </View>
  );
}
