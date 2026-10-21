import { useEvent, useEventListener } from 'expo';
import { useEffect, useRef, useState } from 'react';
import { useVideoPlayer, VideoView } from 'expo-video';
import { ActivityIndicator, Text, View } from 'react-native';
import { GestureDetector, useNativeGesture } from 'react-native-gesture-handler';
import { useMessagePan, useMessagePressConsumption } from '../components/message-interaction';
import { useTheme } from '../theme';
import { t } from '../i18n/index.ts';
import { NativeButton } from '../components/primitives';
import { mediaPlayback, type PlaybackOwner } from './playback-owner.ts';

type VideoClipProps = {
  uri: string;
  compact?: boolean;
  tile?: { width: number; height: number };
  onDimensions?: (size: { width: number; height: number }) => void;
  onInlineUnavailable?: () => void;
};
export function VideoClip({
  uri,
  compact = false,
  tile,
  onDimensions,
  onInlineUnavailable,
}: VideoClipProps) {
  const [attempt, setAttempt] = useState(0);
  return (
    <VideoClipContent
      key={`${uri}:${attempt}`}
      uri={uri}
      compact={compact}
      tile={tile}
      onDimensions={onDimensions}
      onInlineUnavailable={onInlineUnavailable}
      onRetry={() => setAttempt((value) => value + 1)}
    />
  );
}

function VideoClipContent({
  uri,
  compact,
  onRetry,
  tile,
  onDimensions,
  onInlineUnavailable,
}: {
  uri: string;
  compact: boolean;
  onRetry: () => void;
  tile?: VideoClipProps['tile'];
  onDimensions?: VideoClipProps['onDimensions'];
  onInlineUnavailable?: VideoClipProps['onInlineUnavailable'];
}) {
  const { colors } = useTheme();
  const messagePan = useMessagePan();
  const consumePress = useMessagePressConsumption();
  // AVKit owns touches within its controls. Only the surrounding message pan
  // waits for this native boundary; the timeline's vertical scroll is unaffected.
  // Do not use disallowInterruption, which would also cancel ancestor scrolling.
  const nativeControls = useNativeGesture({
    enabled: Boolean(messagePan),
    block: messagePan,
    cancelsTouchesInView: false,
    onActivate: consumePress,
  });
  const player = useVideoPlayer({ uri }, (value) => {
    value.allowsExternalPlayback = false;
  });
  const playback = useRef<{ player: typeof player; owner: PlaybackOwner } | null>(null);
  useEffect(() => {
    const owner = mediaPlayback.register(
      () => player.pause(),
      () => player.playing,
    );
    playback.current = { player, owner };
    return () => {
      if (playback.current?.owner === owner) playback.current = null;
      owner.dispose();
    };
  }, [player]);
  useEventListener(player, 'playingChange', ({ isPlaying }) => {
    const current = playback.current;
    if (current?.player === player) current.owner.observe(isPlaying);
  });
  const status = useEvent(player, 'statusChange', { status: player.status, error: undefined });
  useEffect(() => {
    // The ordinary row keeps its visible Retry. A failed mosaic cell must
    // release the bounded tile before that control would fall below its clip.
    if ((status.status === 'error' || status.error) && playback.current?.player === player)
      onInlineUnavailable?.();
  }, [player, status.status, status.error, onInlineUnavailable]);
  const source = useEvent(player, 'sourceLoad', {
    videoSource: { uri },
    duration: player.duration,
    availableAudioTracks: [],
    availableVideoTracks: player.availableVideoTracks,
    availableSubtitleTracks: [],
  });
  const size = source?.availableVideoTracks[0]?.size;
  useEffect(() => {
    if (
      size &&
      Number.isFinite(size.width) &&
      Number.isFinite(size.height) &&
      size.width > 0 &&
      size.height > 0
    )
      onDimensions?.(size);
  }, [size, onDimensions]);
  const ratio = size && size.width > 0 && size.height > 0 ? size.width / size.height : 16 / 9;
  return (
    <View style={{ width: '100%', minWidth: 0, gap: 10 }}>
      <GestureDetector gesture={nativeControls}>
        <View
          collapsable={false}
          onTouchStart={() => {
            consumePress();
            if (playback.current?.player === player) playback.current.owner.refresh();
          }}
          style={{
            borderRadius: tile ? 0 : compact ? 24 : 16,
            borderCurve: 'continuous',
            overflow: 'hidden',
            backgroundColor: '#000000',
          }}
        >
          <VideoView
            player={player}
            style={
              tile
                ? { width: tile.width, height: tile.height }
                : compact
                  ? {
                      width: '100%',
                      aspectRatio: Math.max(0.75, Math.min(1.9, ratio)),
                      maxHeight: 360,
                    }
                  : { width: '100%', height: 320 }
            }
            contentFit={tile ? 'cover' : 'contain'}
            nativeControls
            fullscreenOptions={{ enable: true }}
            playsInline
          />
          {status.status === 'loading' ? (
            <View
              pointerEvents="none"
              style={{
                position: 'absolute',
                inset: 0,
                alignItems: 'center',
                justifyContent: 'center',
              }}
            >
              <ActivityIndicator color="#FFFFFF" />
            </View>
          ) : null}
        </View>
      </GestureDetector>
      {status.error || status.status === 'error' ? (
        <View style={{ gap: 8 }}>
          <Text accessibilityRole="alert" style={{ color: colors.danger, fontSize: 14 }}>
            {t('media.thisVideoCouldNotBePlayed')}
          </Text>
          <NativeButton secondary label={t('media.retryVideo')} onPress={onRetry} />
        </View>
      ) : null}
    </View>
  );
}
