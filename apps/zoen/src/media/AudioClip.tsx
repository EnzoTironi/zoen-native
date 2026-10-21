import { t, formatLocale } from '../i18n/index.ts';
import { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { Icon, NativeButton } from '../components/primitives';
import { mediaPlayback } from './playback-owner.ts';
import { useAudioClipPlayback, type AudioClipPlayback } from './audio-clip-playback.ts';
import { useTheme } from '../theme';
import { AudioProgress } from './audio-progress';
import { telegramAudio } from '../core/telegram-audio-presentation';

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
  const playback = useAudioClipPlayback({ uri, durationMs });
  const status = useSyncExternalStore(
    playback.subscribe,
    playback.getSnapshot,
    playback.getSnapshot,
  );
  const autoplay = useRef(autoPlay);
  useEffect(() => {
    if (!autoplay.current || !status.loaded) return;
    autoplay.current = false;
    void playback.playFrom().catch(() => {});
  }, [playback, status.loaded]);
  return (
    <AudioClipControls
      playback={playback}
      levels={levels}
      compact={compact}
      appearance={appearance}
      onLongPress={onLongPress}
      onRetry={onRetry}
    />
  );
}

/** Controls only: compact and detailed presentations never allocate another player. */
export function AudioClipControls({
  playback,
  levels = [],
  compact = false,
  appearance = 'default',
  onLongPress,
  onRetry,
}: {
  playback: AudioClipPlayback;
  levels?: readonly number[];
  compact?: boolean;
  appearance?: Props['appearance'];
  onLongPress?: () => void;
  onRetry?: () => void;
}) {
  const { presentation: p } = useTheme();
  const bubble = appearance === 'mine' ? p.chat.outgoing : p.chat.incoming;
  const ink = appearance === 'default' ? p.chat.input.text : bubble.text;
  const controlFill = appearance === 'default' ? p.chat.input.action : bubble.accent;
  const controlInk =
    appearance === 'mine' && p.profile === 'night' ? bubble.fill : p.chat.input.actionText;
  const status = useSyncExternalStore(
    playback.subscribe,
    playback.getSnapshot,
    playback.getSnapshot,
  );
  const total = status.durationSeconds;
  const current = status.currentSeconds;
  const progress = total > 0 ? current / total : 0;
  const rate = status.rate;
  const loadFailed = status.loadFailed;
  const failureMessage = status.error ? t('media.thisAudioCouldNotBePlayed') : null;
  return (
    <View style={{ width: '100%', minWidth: 0, gap: compact ? 2 : 8 }}>
      <View
        testID="audio-playback-controls"
        style={{
          flexDirection: 'row',
          alignItems: 'center',
          gap: compact ? telegramAudio.voiceControlArea - telegramAudio.voiceControl : 10,
        }}
      >
        <Pressable
          onLongPress={onLongPress}
          delayLongPress={350}
          accessibilityRole="button"
          accessibilityLabel={
            status.playing || status.busy ? t('media.pauseAudio') : t('media.playAudio')
          }
          accessibilityState={{
            disabled: !status.loaded || loadFailed,
            busy: status.busy || status.buffering,
          }}
          disabled={!status.loaded || loadFailed}
          onPress={() => {
            if (status.playing || status.busy) playback.pause();
            else void playback.playFrom().catch(() => {});
          }}
          style={({ pressed }) => ({
            width: telegramAudio.voiceControl,
            height: telegramAudio.voiceControl,
            borderRadius: telegramAudio.voiceControl / 2,
            backgroundColor: compact ? controlFill : 'transparent',
            alignItems: 'center',
            justifyContent: 'center',
            opacity: pressed ? 0.6 : 1,
          })}
        >
          {!loadFailed && (!status.loaded || status.busy || status.buffering) ? (
            <ActivityIndicator color={compact ? controlInk : ink} size="small" />
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
              color={compact ? controlInk : ink}
            />
          )}
        </Pressable>
        <View
          style={{
            flex: 1,
            minWidth: 0,
            minHeight: compact ? 44 : 60,
            justifyContent: 'center',
          }}
        >
          <AudioProgress
            levels={levels}
            progress={progress}
            duration={total}
            ink={ink}
            compact={compact}
            disabled={!status.loaded}
            onSeek={(fraction) => {
              void playback.seekSeconds(fraction * total).catch(() => {});
            }}
          />
          <Text
            pointerEvents="none"
            style={{
              position: compact ? 'absolute' : undefined,
              bottom: compact ? 2 : undefined,
              left: compact ? 0 : undefined,
              color: ink,
              opacity: 0.72,
              fontSize: compact ? 13 : 12,
              lineHeight: compact ? 16 : undefined,
              fontVariant: ['tabular-nums'],
            }}
          >
            {clipTime(current > 0 && current < total ? current : total)}
          </Text>
        </View>
        <View style={{ alignItems: 'flex-end', justifyContent: 'center', minWidth: 44 }}>
          <Pressable
            onLongPress={onLongPress}
            delayLongPress={350}
            hitSlop={{ top: 6, bottom: 6 }}
            accessibilityRole="button"
            accessibilityLabel={t('media.playbackRateLabel', {
              rate: new Intl.NumberFormat(formatLocale, { maximumFractionDigits: 1 }).format(rate),
            })}
            disabled={!status.loaded}
            onPress={() => {
              playback.setRate(rate === 1 ? 1.5 : rate === 1.5 ? 2 : 1);
            }}
            style={({ pressed }) => ({
              minHeight: 44,
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
        <NativeButton secondary label={t('media.retryAudio')} onPress={onRetry ?? playback.retry} />
      ) : null}
    </View>
  );
}
