import { ContactAttachment } from './contact-attachment';
import { isContactFileMime } from './contact-file';
import { InlineFileImage } from './inline-file-image';
import { materializeObservedFile } from './observed-file';
import * as Sharing from 'expo-sharing';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { Icon } from '../components/primitives';
import type { FileDto } from '../core/generated/FileDto';
import { useClient } from '../core/provider';
import { formatLocale, t } from '../i18n/index.ts';
import { useTheme } from '../theme';
import { AudioClip, AudioClipControls, clipTime } from './AudioClip';
import { VideoClip } from './VideoClip';
import { AudioWaveform } from './audio-progress';
import { previewConversationMedia } from './conversation-media-preview';
import type { FileAttachmentProps } from './file-attachment-types';
import { formatFileSize } from './file-size';
import { inlineImageFrame } from './inline-image-frame';
import { useReceivedMessageAudio } from './message-audio-provider';
import { previewMediaFile } from './preview-file';
import type { LocalMediaFile, MediaDriver } from './types';
import { VoiceTranscript } from './voice-transcript';
import { openSiteFile } from '../sites/open-file';

function FileAttachmentContent({
  itemId,
  version,
  title,
  voice,
  expectedHash,
  presentation = 'document',
  renderAsFile = false,
  mine = false,
  onActions,
  onImageDecoded,
  messageSource,
  galleryEntries,
  tile,
  onInlineUnavailable,
  isCurrent,
}: FileAttachmentProps) {
  const { store, snapshot, mode } = useClient();
  const driver = store.driver.media;
  const { colors, scheme } = useTheme();
  const message = presentation === 'message';
  const account = snapshot?.account;
  const received = useReceivedMessageAudio(
    messageSource?.spaceId ?? '',
    message && voice ? (messageSource?.entry ?? null) : null,
    mode === 'account' && account
      ? {
          identityId: account.identity_id,
          deviceId: account.device_id,
          epoch: store.sessionEpoch,
        }
      : null,
  );
  // Voice sits on its message bubble; generic file fallbacks sit on the sky.
  const ink = message && mine && voice ? colors.onMine : colors.text;
  const [info, setInfo] = useState<FileDto | null>(null);
  const [local, setLocal] = useState<LocalMediaFile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Presentation failures do not change whether the canonical file can render
  // inline. Only metadata, inline materialization or decoding can release a tile.
  const [inlineFailed, setInlineFailed] = useState(false);
  const [ratio, setRatio] = useState(1);
  const [availableWidth, setAvailableWidth] = useState(0);
  const [autoplay, setAutoplay] = useState(false);
  const mounted = useRef(false);
  const generation = useRef(0);
  const owned = useRef<LocalMediaFile | null>(null);
  const actionBusy = useRef(false);
  const current = useCallback(() => mounted.current && (isCurrent?.() ?? true), [isCurrent]);
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
    if (driver && !received && current())
      void driver
        .fileInfo(itemId, version)
        .then((value) => {
          if (active && current()) setInfo(value);
        })
        .catch((reason: unknown) => {
          if (active && current()) {
            setInlineFailed(true);
            setError(
              reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'),
            );
          }
        });
    return () => {
      active = false;
    };
  }, [driver, itemId, version, expectedHash, snapshot, received, current]);
  const materialize = useCallback(
    (media: MediaDriver) =>
      materializeObservedFile(media, { itemId, version, expectedHash }, current),
    [itemId, version, expectedHash, current],
  );

  // Only already-arrived, modest visual files become inline previews. Large files
  // remain an explicit open action; audio never starts or requests permission here.
  const visual =
    !renderAsFile && Boolean(info?.mime.startsWith('image/') || info?.mime.startsWith('video/'));
  useEffect(() => {
    // A signed media group can contain a generic/large file. Retain its ordinary
    // canonical row rather than squeezing a file action into a visual mosaic.
    if (current() && (inlineFailed || (info && (!visual || Number(info.bytes) > 16 * 1048576))))
      onInlineUnavailable?.();
  }, [info, visual, inlineFailed, onInlineUnavailable, current]);
  useEffect(() => {
    if (
      !current() ||
      !driver ||
      !message ||
      !info?.ready ||
      !visual ||
      Number(info.bytes) > 16 * 1048576
    )
      return;
    let active = true;
    const expectedGeneration = generation.current;
    void materialize(driver)
      .then(async (file) => {
        if (!active || !current() || expectedGeneration !== generation.current) {
          await driver.releaseLocalFile(file.uri);
          return;
        }
        const previous = owned.current;
        owned.current = file;
        setLocal(file);
        if (previous) await driver.releaseLocalFile(previous.uri);
      })
      .catch((reason: unknown) => {
        if (active && current() && expectedGeneration === generation.current) {
          setInlineFailed(true);
          setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
        }
      });
    return () => {
      active = false;
    };
  }, [driver, message, info?.ready, info?.bytes, info?.sha256, visual, materialize, current]);

  async function action(kind: 'open' | 'share') {
    if (!driver || !current() || actionBusy.current) return;
    if (kind === 'open' && openSiteFile(info, itemId, version)) return;
    actionBusy.current = true;
    const expectedGeneration = generation.current;
    setBusy(true);
    setError(null);
    let file: LocalMediaFile | null = null;
    try {
      file = await materialize(driver);
      if (!current() || expectedGeneration !== generation.current) return;
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
        if (!current() || expectedGeneration !== generation.current) return;
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
      if (current() && expectedGeneration === generation.current)
        setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
    } finally {
      try {
        if (file) await driver.releaseLocalFile(file.uri);
      } catch (reason) {
        if (current() && expectedGeneration === generation.current)
          setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
      } finally {
        actionBusy.current = false;
        if (current()) setBusy(false);
      }
    }
  }
  async function previewImage() {
    if (!driver || !current() || actionBusy.current) return;
    actionBusy.current = true;
    const expectedGeneration = generation.current;
    setError(null);
    let file: LocalMediaFile | null = null;
    try {
      if (
        messageSource &&
        !messageSource.entry.forwarded &&
        driver.previewConversationMedia &&
        visual &&
        !voice
      ) {
        await previewConversationMedia(
          store,
          messageSource.spaceId,
          messageSource.entry.id,
          galleryEntries ?? [messageSource.entry],
        );
        return;
      }
      // A separate native-viewer lease outlives the virtualized message row.
      file = await materialize(driver);
      if (!current() || expectedGeneration !== generation.current) return;
      const preview = file;
      file = null;
      await previewMediaFile(driver, preview);
    } catch (reason) {
      if (current() && expectedGeneration === generation.current)
        setError(reason instanceof Error ? reason.message : t('media.thisFileCouldNotBeOpened'));
    } finally {
      try {
        if (file) await driver.releaseLocalFile(file.uri);
      } catch (reason) {
        if (current() && expectedGeneration === generation.current)
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
  const image = !renderAsFile && local?.info.mime.startsWith('image/');
  const video = !renderAsFile && local?.info.mime.startsWith('video/');
  const imageFrame = inlineImageFrame(availableWidth, ratio);
  const outline = scheme === 'dark' ? '#FFFFFF1A' : '#0000001A';
  if (info && isContactFileMime(info.mime) && driver?.readContact && !voice)
    return (
      <ContactAttachment
        media={driver}
        source={{ itemId, version, expectedHash: expectedHash ?? info.sha256 }}
        spaceId={messageSource?.spaceId}
        readiness={readiness}
        message={message}
        onActions={onActions}
        isCurrent={current}
      />
    );
  return (
    <View
      onLayout={(event) => {
        const width = event.nativeEvent.layout.width;
        if (Number.isFinite(width) && width > 0) setAvailableWidth(width);
      }}
      style={{
        width: '100%',
        height: tile?.height,
        minWidth: 0,
        gap: message ? 0 : 8,
        padding: message ? 0 : 12,
        borderRadius: message ? 24 : 20,
        borderCurve: 'continuous',
        backgroundColor: message ? 'transparent' : colors.surface,
      }}
    >
      {voice && received ? (
        <View style={{ paddingHorizontal: 10, paddingVertical: 5 }}>
          <AudioClipControls
            playback={received}
            levels={voice.levels}
            compact
            appearance={mine ? 'mine' : 'received'}
            onLongPress={onActions}
          />
        </View>
      ) : local && image && message ? (
        <InlineFileImage
          local={local}
          label={label}
          mine={mine}
          tile={tile}
          frame={imageFrame}
          outline={outline}
          onActions={onActions}
          onOpen={() => {
            void previewImage();
          }}
          isCurrent={() => current() && owned.current === local}
          onDecoded={(width, height) => {
            setRatio(width / height);
            onImageDecoded?.({
              itemId: local.itemId,
              version: local.version,
              sha256: local.info.sha256,
              width,
              height,
            });
          }}
          onFailure={() => {
            setInlineFailed(true);
            setError(t('media.thisFileCouldNotBeOpened'));
          }}
        />
      ) : local && video && message ? (
        <View>
          <VideoClip
            uri={local.uri}
            compact
            tile={tile}
            onInlineUnavailable={onInlineUnavailable}
            onDimensions={
              tile
                ? (size) => {
                    if (!current() || owned.current !== local) return;
                    onImageDecoded?.({
                      itemId: local.itemId,
                      version: local.version,
                      sha256: local.info.sha256,
                      ...size,
                    });
                  }
                : undefined
            }
          />
          <Pressable
            testID={`message-video-gallery-${itemId}`}
            accessibilityRole="button"
            accessibilityLabel={t('media.openFileLabel', { name: label })}
            onPress={() => {
              void previewImage();
            }}
            style={{
              position: 'absolute',
              top: 6,
              right: 6,
              width: 44,
              height: 44,
              alignItems: 'center',
              justifyContent: 'center',
              borderRadius: 22,
              backgroundColor: '#00000060',
            }}
          >
            <Icon
              name={{
                ios: 'arrow.up.left.and.arrow.down.right',
                android: 'fullscreen',
                web: 'fullscreen',
              }}
              color="#FFFFFF"
              size={20}
            />
          </Pressable>
        </View>
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
            gap: 11,
            paddingHorizontal: 10,
            paddingVertical: 5,
            minHeight: 54,
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
            <Text style={{ color: colors.secondary, fontSize: 12 }}>{readiness}</Text>
          </View>
        </Pressable>
      )}
      {voice?.transcript ? (
        <VoiceTranscript
          text={voice.transcript}
          ink={ink}
          message={message}
          onLongPress={onActions}
        />
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
          numberOfLines={tile ? 2 : undefined}
          style={{
            color: tile ? '#FFFFFF' : ink,
            fontSize: 13,
            lineHeight: 19,
            padding: message ? 12 : 0,
            ...(tile
              ? {
                  position: 'absolute',
                  left: 0,
                  right: 0,
                  bottom: 0,
                  maxHeight: tile.height,
                  backgroundColor: '#000000B3',
                }
              : {}),
          }}
        >
          {error}
        </Text>
      ) : null}
    </View>
  );
}
export default FileAttachmentContent;
