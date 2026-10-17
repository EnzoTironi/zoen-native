import { t, formatLocale } from '../i18n/index.ts';
import { useAudioPlayer, useAudioPlayerStatus, setAudioModeAsync } from 'expo-audio';
import { useCallback, useEffect, useRef, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { Icon } from '../components/primitives';
import { useTheme } from '../theme';
import { AudioProgress } from './audio-progress';

export function clipTime(seconds: number) {
  const value = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`;
}

let stopPrevious: (() => void) | null = null;
let playbackIntent = 0;
export function pauseAudioPlayback() {
  ++playbackIntent;
  stopPrevious?.();
  stopPrevious = null;
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
  return <AudioClipContent key={props.uri} {...props} />;
}

function AudioClipContent({
  uri,
  durationMs = 0,
  levels = [],
  compact = false,
  appearance = 'default',
  autoPlay = false,
  onLongPress,
}: Props) {
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
  const pause = useCallback(() => player.pause(), [player]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      seekTarget.current = null;
      if (stopPrevious === pause) {
        ++playbackIntent;
        stopPrevious = null;
      }
    };
  }, [pause]);
  const total =
    Number.isFinite(status.duration) && status.duration > 0
      ? status.duration
      : Math.max(0, durationMs / 1000);
  const current = Math.min(total, Math.max(0, seeking ?? status.currentTime));
  const progress = total > 0 ? current / total : 0;
  const report = useCallback((reason: unknown) => {
    if (mounted.current)
      setFailure(reason instanceof Error ? reason.message : t('media.thisAudioCouldNotBePlayed'));
  }, []);
  const play = useCallback(async () => {
    if (!player.isLoaded || !mounted.current) return;
    const intent = ++playbackIntent;
    stopPrevious?.();
    stopPrevious = pause;
    setStarting(true);
    try {
      await setAudioModeAsync({
        playsInSilentMode: true,
        allowsRecording: false,
        shouldPlayInBackground: false,
        interruptionMode: 'doNotMix',
      });
      if (!mounted.current || playbackIntent !== intent || stopPrevious !== pause) return;
      if (player.duration > 0 && player.currentTime >= player.duration - 0.05)
        await player.seekTo(0);
      if (mounted.current && playbackIntent === intent && stopPrevious === pause) player.play();
    } finally {
      if (mounted.current) setStarting(false);
    }
  }, [player, pause]);
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
            disabled: !status.isLoaded || starting,
            busy: starting || status.isBuffering,
          }}
          disabled={!status.isLoaded || starting}
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
          {!status.isLoaded || starting || status.isBuffering ? (
            <ActivityIndicator color={ink} size="small" />
          ) : (
            <Icon
              name={
                status.playing
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
      {failure ? (
        <Text selectable accessibilityRole="alert" style={{ color: ink, fontSize: 13 }}>
          {failure}
        </Text>
      ) : null}
    </View>
  );
}
