import { t, formatLocale } from '../i18n/index.ts';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import * as Sharing from 'expo-sharing';
import { Image } from 'expo-image';
import type { MediaDriver, LocalMediaFile } from './types';
import type { FileDto } from '../core/generated/FileDto';
import type { TimelineEntry } from '../core/types';
import { useClient } from '../core/provider';
import { useTheme } from '../theme';
import { Icon } from '../components/primitives';
import { AudioClip, clipTime } from './AudioClip';
import { AudioWaveform } from './audio-progress';
import { VideoClip } from './VideoClip';
import { previewMediaFile } from './preview-file';
import { formatFileSize } from './file-size';

type Presentation = 'message' | 'document';
type FileAttachmentProps = {
  itemId: string;
  version?: number;
  title?: string;
  voice?: TimelineEntry['voice'];
  expectedHash?: string;
  presentation?: Presentation;
  mine?: boolean;
  onActions?: () => void;
};

/** Version/hash identity owns the player and releases its prior decrypted copy. */
export function FileAttachment(props: FileAttachmentProps) {
  return (
    <FileAttachmentContent
      key={`${props.itemId}:${props.version ?? 'latest'}:${props.expectedHash ?? ''}`}
      {...props}
    />
  );
}

function FileAttachmentContent({
  itemId,
  version,
  title,
  voice,
  expectedHash,
  presentation = 'document',
  mine = false,
  onActions,
}: FileAttachmentProps) {
  const { store, snapshot } = useClient();
  const driver = store.driver.media;
  const { colors, scheme } = useTheme();
  const message = presentation === 'message';
  const ink = message && mine ? colors.onMine : colors.text;
  const [info, setInfo] = useState<FileDto | null>(null);
  const [local, setLocal] = useState<LocalMediaFile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ratio, setRatio] = useState(1);
  const [autoplay, setAutoplay] = useState(false);
  const [transcriptExpanded, setTranscriptExpanded] = useState(false);
  const mounted = useRef(false);
  const generation = useRef(0);
  const owned = useRef<LocalMediaFile | null>(null);
  const actionBusy = useRef(false);
  useEffect(() => {
    mounted.current = true;
    const startedGeneration = generation.current;
    return () => {
      mounted.current = false;
      generation.current = startedGeneration + 1;
      const file = owned.current;
      owned.current = null;
      if (file && driver) void driver.releaseLocalFile(file.uri).catch(() => {});
    };
  }, [driver]);
  // Readiness is canonical. Looking at a thumbnail never restores an old version.
  useEffect(() => {
    let active = true;
    if (driver)
      void driver
        .fileInfo(itemId, version)
        .then((value) => {
          if (active) setInfo(value);
        })
        .catch((reason: unknown) => {
          if (active)
            setError(
              reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'),
            );
        });
    return () => {
      active = false;
    };
  }, [driver, itemId, version, expectedHash, snapshot]);
  const materialize = useCallback(
    async (media: MediaDriver) => {
      const file = await media.materializeFile(itemId, version);
      if (!file) throw new Error(t('media.thisFileIsStillArrivingTryAgainWhen'));
      if (expectedHash && file.info.sha256 !== expectedHash) {
        await media.releaseLocalFile(file.uri);
        throw new Error(t('media.theAudioHasBeenUpdatedOpenTheMessage'));
      }
      return file;
    },
    [itemId, version, expectedHash],
  );

  // Only already-arrived, modest visual files become inline previews. Large files
  // remain an explicit open action; audio never starts or requests permission here.
  const visual = Boolean(info?.mime.startsWith('image/') || info?.mime.startsWith('video/'));
  useEffect(() => {
    if (!driver || !message || !info?.ready || !visual || Number(info.bytes) > 16 * 1048576) return;
    let active = true;
    const expectedGeneration = generation.current;
    void materialize(driver)
      .then(async (file) => {
        if (!active || !mounted.current || expectedGeneration !== generation.current) {
          await driver.releaseLocalFile(file.uri);
          return;
        }
        const previous = owned.current;
        owned.current = file;
        setLocal(file);
        if (previous) await driver.releaseLocalFile(previous.uri);
      })
      .catch((reason: unknown) => {
        if (active && mounted.current)
          setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
      });
    return () => {
      active = false;
    };
  }, [driver, message, info?.ready, info?.bytes, info?.sha256, visual, materialize]);

  async function action(kind: 'open' | 'share') {
    if (!driver || actionBusy.current) return;
    actionBusy.current = true;
    const expectedGeneration = generation.current;
    setBusy(true);
    setError(null);
    let file: LocalMediaFile | null = null;
    try {
      file = await materialize(driver);
      if (!mounted.current || expectedGeneration !== generation.current) return;
      if (kind === 'open' && (voice || (message && visual))) {
        const previous = owned.current;
        owned.current = file;
        setLocal(file);
        file = null;
        if (voice) setAutoplay(true);
        if (previous) await driver.releaseLocalFile(previous.uri);
        return;
      }
      if (kind === 'share') {
        const available = await Sharing.isAvailableAsync();
        if (!mounted.current || expectedGeneration !== generation.current) return;
        if (!available) throw new Error(t('media.sharingIsNotAvailableOnThisDevice'));
        await Sharing.shareAsync(file.uri, {
          mimeType: file.info.mime,
          dialogTitle: file.info.name,
        });
      } else {
        const preview = file;
        file = null;
        await previewMediaFile(driver, preview);
      }
    } catch (reason) {
      if (mounted.current && expectedGeneration === generation.current)
        setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
    } finally {
      try {
        if (file) await driver.releaseLocalFile(file.uri);
      } catch (reason) {
        if (mounted.current && expectedGeneration === generation.current)
          setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
      } finally {
        actionBusy.current = false;
        if (mounted.current) setBusy(false);
      }
    }
  }
  async function previewImage() {
    if (!driver || actionBusy.current) return;
    actionBusy.current = true;
    const expectedGeneration = generation.current;
    setError(null);
    let file: LocalMediaFile | null = null;
    try {
      // A separate lease stays alive for QuickLook even if its message scrolls away.
      file = await materialize(driver);
      if (!mounted.current || expectedGeneration !== generation.current) return;
      const preview = file;
      file = null;
      await previewMediaFile(driver, preview);
    } catch (reason) {
      if (mounted.current && expectedGeneration === generation.current)
        setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
    } finally {
      try {
        if (file) await driver.releaseLocalFile(file.uri);
      } catch (reason) {
        if (mounted.current && expectedGeneration === generation.current)
          setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
      } finally {
        actionBusy.current = false;
      }
    }
  }
  const label = title ?? info?.name ?? t('media.file2');
  const readiness = info
    ? info.ready
      ? formatFileSize(info.bytes, formatLocale)
      : t('media.receivedParts', {
          received: info.chunks_here,
          total: info.chunks,
          count: info.chunks,
        })
    : t('media.loading');
  const image = local?.info.mime.startsWith('image/');
  const video = local?.info.mime.startsWith('video/');
  const outline = scheme === 'dark' ? '#FFFFFF1A' : '#0000001A';
  return (
    <View
      style={{
        width: '100%',
        minWidth: 0,
        gap: message ? 0 : 8,
        padding: message ? 0 : 12,
        borderRadius: message ? 24 : 20,
        borderCurve: 'continuous',
        backgroundColor: message ? 'transparent' : colors.surface,
      }}
    >
      {local && image && message ? (
        <Pressable
          onLongPress={onActions}
          delayLongPress={350}
          accessibilityRole="button"
          accessibilityLabel={t('media.openFileLabel', { name: label })}
          onPress={() => {
            void previewImage();
          }}
          style={({ pressed }) => ({
            borderRadius: 24,
            borderCurve: 'continuous',
            overflow: 'hidden',
            opacity: pressed ? 0.8 : 1,
          })}
        >
          <Image
            cachePolicy="none"
            recyclingKey={`${itemId}:${local.info.sha256}`}
            source={{ uri: local.uri }}
            accessible={false}
            contentFit="cover"
            onLoad={(event) => {
              const { width, height } = event.source;
              if (width > 0 && height > 0) setRatio(Math.max(0.7, Math.min(1.8, width / height)));
            }}
            onError={() => setError(t('media.thisFileCouldNotBeOpened'))}
            style={{ width: '100%', aspectRatio: ratio, maxHeight: 360 }}
          />
          <View
            pointerEvents="none"
            style={{
              position: 'absolute',
              inset: 0,
              borderWidth: 1,
              borderColor: outline,
              borderRadius: 24,
              borderCurve: 'continuous',
            }}
          />
        </Pressable>
      ) : local && video && message ? (
        <VideoClip uri={local.uri} compact />
      ) : voice && local ? (
        <View style={{ paddingHorizontal: message ? 10 : 0, paddingVertical: message ? 5 : 0 }}>
          <AudioClip
            uri={local.uri}
            durationMs={Number(voice.ms)}
            levels={voice.levels}
            compact={message}
            appearance={message ? (mine ? 'mine' : 'received') : 'default'}
            autoPlay={autoplay}
            onLongPress={onActions}
          />
        </View>
      ) : voice && message ? (
        <Pressable
          onLongPress={onActions}
          delayLongPress={350}
          accessibilityRole="button"
          accessibilityLabel={t('media.playAudio')}
          accessibilityState={{ busy, disabled: busy || !driver }}
          disabled={busy || !driver}
          onPress={() => {
            void action('open');
          }}
          style={({ pressed }) => ({
            flexDirection: 'row',
            alignItems: 'center',
            gap: 8,
            paddingHorizontal: 10,
            paddingVertical: 5,
            minHeight: 58,
            opacity: pressed ? 0.6 : 1,
          })}
        >
          <View style={{ width: 44, height: 44, alignItems: 'center', justifyContent: 'center' }}>
            {busy ? (
              <ActivityIndicator color={ink} />
            ) : (
              <Icon
                name={{ ios: 'play.fill', android: 'play_arrow', web: 'play_arrow' }}
                color={ink}
                size={25}
              />
            )}
          </View>
          <AudioWaveform levels={voice.levels} progress={0} ink={ink} compact />
          <Text
            style={{
              color: ink,
              opacity: 0.72,
              minWidth: 44,
              textAlign: 'right',
              fontSize: 12,
              fontVariant: ['tabular-nums'],
            }}
          >
            {clipTime(Number(voice.ms) / 1000)}
          </Text>
        </Pressable>
      ) : (
        <Pressable
          onLongPress={onActions}
          delayLongPress={350}
          accessibilityRole="button"
          accessibilityLabel={t('media.openFileLabel', { name: label })}
          disabled={busy || !driver}
          onPress={() => {
            void action('open');
          }}
          style={({ pressed }) => ({
            flexDirection: 'row',
            gap: 12,
            alignItems: 'center',
            minHeight: visual && message ? 160 : 64,
            padding: message ? 14 : 0,
            opacity: pressed ? 0.65 : 1,
          })}
        >
          {busy ? (
            <ActivityIndicator color={ink} />
          ) : (
            <View
              style={{
                width: 44,
                height: 48,
                borderRadius: 10,
                backgroundColor: message ? 'transparent' : colors.muted,
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <Icon
                name={
                  info?.mime.startsWith('image/')
                    ? { ios: 'photo', android: 'image', web: 'image' }
                    : info?.mime.startsWith('video/')
                      ? { ios: 'video.fill', android: 'videocam', web: 'videocam' }
                      : { ios: 'doc.fill', android: 'description', web: 'description' }
                }
                size={30}
                color={ink}
              />
            </View>
          )}
          <View style={{ flex: 1, minWidth: 0, gap: 5 }}>
            <Text numberOfLines={2} style={{ color: ink, fontSize: 15, fontWeight: '600' }}>
              {label}
            </Text>
            <Text style={{ color: ink, opacity: 0.72, fontSize: 12 }}>{readiness}</Text>
          </View>
        </Pressable>
      )}
      {voice?.transcript ? (
        <View
          style={{ paddingHorizontal: message ? 14 : 0, paddingBottom: message ? 12 : 0, gap: 4 }}
        >
          <Pressable
            accessibilityRole="button"
            aria-expanded={transcriptExpanded}
            accessibilityLabel={t('media.voiceTranscript')}
            onPress={() => setTranscriptExpanded(!transcriptExpanded)}
            style={{
              flexDirection: 'row',
              alignItems: 'center',
              justifyContent: 'space-between',
              minHeight: 44,
            }}
          >
            <Text style={{ color: ink, fontSize: 12, fontWeight: '600' }}>
              {t('media.voiceTranscript')}
            </Text>
            <Icon
              name={
                transcriptExpanded
                  ? { ios: 'chevron.up', android: 'expand_less', web: 'expand_less' }
                  : { ios: 'chevron.down', android: 'expand_more', web: 'expand_more' }
              }
              color={ink}
              size={11}
            />
          </Pressable>
          {transcriptExpanded ? (
            <Text selectable style={{ color: ink, fontSize: 15, lineHeight: 21 }}>
              {voice.transcript}
            </Text>
          ) : null}
        </View>
      ) : null}
      {!message ? (
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={t('media.shareFileLabel', { name: voice ? t('media.audio') : label })}
          disabled={busy || !driver}
          onPress={() => {
            void action('share');
          }}
          style={{ minHeight: 44, justifyContent: 'center' }}
        >
          <Text style={{ color: colors.accent, fontSize: 13, fontWeight: '600' }}>
            {t('media.share')}
          </Text>
        </Pressable>
      ) : null}
      {error ? (
        <Text
          selectable
          accessibilityRole="alert"
          style={{ color: ink, fontSize: 13, lineHeight: 19, padding: message ? 12 : 0 }}
        >
          {error}
        </Text>
      ) : null}
    </View>
  );
}

export function MessageMedia({
  entry,
  presentation = 'message',
  onActions,
}: {
  entry: TimelineEntry;
  presentation?: Presentation;
  onActions?: () => void;
}) {
  if (entry.deleted) return null;
  if (entry.voice)
    return (
      <FileAttachment
        key={`${entry.voice.item_id}:${entry.voice.sha256}`}
        itemId={entry.voice.item_id}
        voice={entry.voice}
        expectedHash={entry.voice.sha256}
        presentation={presentation}
        mine={entry.author.is_me}
        onActions={onActions}
      />
    );
  const card = 'Message' in entry.kind ? entry.kind.Message.card : null;
  return card?.kind_id === 'file' ? (
    <FileAttachment
      itemId={card.item_id}
      version={card.version}
      title={card.title}
      presentation={presentation}
      mine={entry.author.is_me}
      onActions={onActions}
    />
  ) : null;
}
