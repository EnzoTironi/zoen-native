import { t, formatLocale } from '../i18n/index.ts';
import { useEventListener } from 'expo';
import { useAudioPlayer, useAudioPlayerStatus, setAudioModeAsync } from 'expo-audio';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { Icon, NativeButton } from '../components/primitives';
import { mediaPlayback, type PlaybackOwner } from './playback-owner.ts';
import { useTheme } from '../theme';
import { AudioProgress } from './audio-progress';

export function clipTime(seconds: number) {
  const value = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`;
}

export function pauseAudioPlayback() {
  mediaPlayback.pauseAll();
}

type Props = {
  uri: string;
  durationMs?: number;
  levels?: readonly number[];
  compact?: boolean;
  appearance?: 'mine' | 'received' | 'default';
  autoPlay?: boolean;
  onLongPress?: () => void;
};

/** Each signed URI owns one player; recycled rows cannot retain a previous source's intent. */
export function AudioClip(props: Props) {
  const [retry, setRetry] = useState(0);
  return (
    <AudioClipContent
      key={`${props.uri}:${retry}`}
      {...props}
      onRetry={() => setRetry((value) => value + 1)}
    />
  );
}

function AudioClipContent({
  uri,
  durationMs = 0,
  levels = [],
  compact = false,
  appearance = 'default',
  autoPlay = false,
  onLongPress,
  onRetry,
}: Props & { onRetry: () => void }) {
  const { colors } = useTheme();
  const ink = appearance === 'mine' ? colors.onMine : colors.text;
  const player = useAudioPlayer({ uri }, { updateInterval: 100, allowsExternalPlayback: false });
  const status = useAudioPlayerStatus(player);
  const [failure, setFailure] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [seeking, setSeeking] = useState<number | null>(null);
  const mounted = useRef(false);
  const autoplay = useRef(autoPlay);
  const seekTarget = useRef<number | null>(null);
  const seekRunning = useRef(false);
  const playback = useRef<{ player: typeof player; owner: PlaybackOwner } | null>(null);
  const pause = useCallback(() => player.pause(), [player]);
  useEffect(() => {
    mounted.current = true;
    const owner = mediaPlayback.register(pause, () => player.playing);
    playback.current = { player, owner };
    return () => {
      mounted.current = false;
      seekTarget.current = null;
      if (playback.current?.owner === owner) playback.current = null;
      owner.dispose();
    };
  }, [pause, player]);
  useEventListener(player, 'playbackStatusUpdate', () => {
    const current = playback.current;
    if (mounted.current && current?.player === player) current.owner.observeExplicitPlayback();
  });
  const total =
    Number.isFinite(status.duration) && status.duration > 0
      ? status.duration
      : Math.max(0, durationMs / 1000);
  const current = Math.min(total, Math.max(0, seeking ?? status.currentTime));
  const progress = total > 0 ? current / total : 0;
  const loadFailed = Boolean(status.error) || status.playbackState === 'failed';
  const failureMessage = loadFailed ? t('media.thisAudioCouldNotBePlayed') : failure;
  const report = useCallback((_reason: unknown) => {
    if (mounted.current) setFailure(t('media.thisAudioCouldNotBePlayed'));
  }, []);
  const play = useCallback(async () => {
    const currentOwner = playback.current;
    const owner = currentOwner?.player === player ? currentOwner.owner : null;
    if (!player.isLoaded || !mounted.current || !owner) return;
    const intent = owner.begin();
    if (!intent) return;
    setStarting(true);
    try {
      await setAudioModeAsync({
        playsInSilentMode: true,
        allowsRecording: false,
        shouldPlayInBackground: false,
        interruptionMode: 'doNotMix',
      });
      if (!mounted.current || !owner.isCurrent(intent)) return;
      if (player.duration > 0 && player.currentTime >= player.duration - 0.05)
        await player.seekTo(0);
      if (mounted.current && owner.isCurrent(intent)) player.play();
    } catch (reason) {
      owner.release(intent);
      throw reason;
    } finally {
      if (mounted.current) setStarting(false);
    }
  }, [player]);
  useEffect(() => {
    if (!autoplay.current || !status.isLoaded) return;
    autoplay.current = false;
    void play().catch(report);
  }, [status.isLoaded, play, report]);

  function seek(fraction: number) {
    if (!status.isLoaded || !total) return;
    const seconds = Math.min(total, Math.max(0, fraction * total));
    seekTarget.current = seconds;
    setSeeking(seconds);
    if (seekRunning.current) return;
    seekRunning.current = true;
    void (async () => {
      try {
        while (mounted.current && seekTarget.current !== null) {
          const target = seekTarget.current;
          seekTarget.current = null;
          await player.seekTo(target);
        }
      } catch (reason) {
        report(reason);
      } finally {
        seekRunning.current = false;
        if (mounted.current) setSeeking(null);
      }
    })();
  }
  const rate = status.playbackRate || 1;
  return (
    <View style={{ width: '100%', minWidth: 0, gap: compact ? 2 : 8 }}>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: compact ? 6 : 10 }}>
        <Pressable
          onLongPress={onLongPress}
          delayLongPress={350}
          accessibilityRole="button"
          accessibilityLabel={status.playing ? t('media.pauseAudio') : t('media.playAudio')}
          accessibilityState={{
            disabled: !status.isLoaded || starting || loadFailed,
            busy: starting || status.isBuffering,
          }}
          disabled={!status.isLoaded || starting || loadFailed}
          onPress={() => {
            setFailure(null);
            if (status.playing) pauseAudioPlayback();
            else void play().catch(report);
          }}
          style={({ pressed }) => ({
            width: 44,
            height: 44,
            borderRadius: 22,
            alignItems: 'center',
            justifyContent: 'center',
            opacity: pressed ? 0.6 : 1,
          })}
        >
          {!loadFailed && (!status.isLoaded || starting || status.isBuffering) ? (
            <ActivityIndicator color={ink} size="small" />
          ) : (
            <Icon
              name={
                loadFailed
                  ? {
                      ios: 'exclamationmark.circle',
                      android: 'error_outline',
                      web: 'error_outline',
                    }
                  : status.playing
                    ? { ios: 'pause.fill', android: 'pause', web: 'pause' }
                    : { ios: 'play.fill', android: 'play_arrow', web: 'play_arrow' }
              }
              size={25}
              color={ink}
            />
          )}
        </Pressable>
        <AudioProgress
          levels={levels}
          progress={progress}
          duration={total}
          ink={ink}
          compact={compact}
          disabled={!status.isLoaded}
          onSeek={seek}
        />
        <View style={{ alignItems: 'flex-end', justifyContent: 'center', minWidth: 44 }}>
          <Text style={{ color: ink, opacity: 0.72, fontSize: 12, fontVariant: ['tabular-nums'] }}>
            {clipTime(current > 0 && current < total ? current : total)}
          </Text>
          <Pressable
            onLongPress={onLongPress}
            delayLongPress={350}
            hitSlop={{ top: 6, bottom: 6 }}
            accessibilityRole="button"
            accessibilityLabel={t('media.playbackRateLabel', {
              rate: new Intl.NumberFormat(formatLocale, { maximumFractionDigits: 1 }).format(rate),
            })}
            disabled={!status.isLoaded}
            onPress={() => {
              try {
                player.setPlaybackRate(rate === 1 ? 1.5 : rate === 1.5 ? 2 : 1);
              } catch (reason) {
                report(reason);
              }
            }}
            style={({ pressed }) => ({
              minHeight: 32,
              minWidth: 44,
              alignItems: 'flex-end',
              justifyContent: 'center',
              opacity: pressed ? 0.6 : 1,
            })}
          >
            <Text
              style={{ color: ink, fontSize: 12, fontWeight: '700', fontVariant: ['tabular-nums'] }}
            >
              {new Intl.NumberFormat(formatLocale, { maximumFractionDigits: 1 }).format(rate)}×
            </Text>
          </Pressable>
        </View>
      </View>
      {failureMessage ? (
        <Text accessibilityRole="alert" style={{ color: ink, fontSize: 13 }}>
          {failureMessage}
        </Text>
      ) : null}
      {loadFailed ? (
        <NativeButton secondary label={t('media.retryAudio')} onPress={onRetry} />
      ) : null}
    </View>
  );
}
