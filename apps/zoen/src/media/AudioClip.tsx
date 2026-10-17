import { useAudioPlayer, useAudioPlayerStatus, setAudioModeAsync } from 'expo-audio';
import { useCallback, useEffect, useState } from 'react';
import { Pressable, Text, View } from 'react-native';
import { Icon } from '../components/primitives';
import { useTheme } from '../theme';

export function clipTime(seconds: number) {
  const value = Math.max(0, Math.floor(seconds));
  return `${Math.floor(value / 60)}:${String(value % 60).padStart(2, '0')}`;
}

let stopPrevious: (() => void) | null = null;

export function pauseAudioPlayback() {
  stopPrevious?.();
  stopPrevious = null;
}

export function AudioClip({
  uri,
  durationMs = 0,
  levels = [],
}: {
  uri: string;
  durationMs?: number;
  levels?: number[];
}) {
  const { colors } = useTheme();
  const player = useAudioPlayer({ uri }, { updateInterval: 150, allowsExternalPlayback: false });
  const status = useAudioPlayerStatus(player);
  const [failure, setFailure] = useState<string | null>(null);
  const pause = useCallback(() => player.pause(), [player]);
  useEffect(() => {
    return () => {
      if (stopPrevious === pause) stopPrevious = null;
    };
  }, [pause]);
  const total = status.duration || durationMs / 1000;
  const progress = total ? Math.min(1, status.currentTime / total) : 0;
  const bars = levels.length
    ? levels.filter((_, index) => index % Math.max(1, Math.ceil(levels.length / 40)) === 0)
    : Array.from({ length: 28 }, () => 0.2);
  async function toggle() {
    if (status.playing) player.pause();
    else {
      stopPrevious?.();
      stopPrevious = pause;
      await setAudioModeAsync({
        playsInSilentMode: true,
        allowsRecording: false,
        shouldPlayInBackground: false,
        interruptionMode: 'doNotMix',
      });
      if (status.didJustFinish || (total && status.currentTime >= total - 0.05))
        await player.seekTo(0);
      player.play();
    }
  }
  return (
    <View style={{ minWidth: 210, gap: 8 }}>
      <View style={{ flexDirection: 'row', alignItems: 'center', gap: 12 }}>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel={status.playing ? 'Pausar áudio' : 'Reproduzir áudio'}
          onPress={() => {
            setFailure(null);
            void toggle().catch((reason: unknown) =>
              setFailure(
                reason instanceof Error
                  ? reason.message
                  : 'Não foi possível reproduzir este áudio.',
              ),
            );
          }}
          style={{
            minWidth: 44,
            minHeight: 44,
            borderRadius: 22,
            backgroundColor: colors.muted,
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <Icon
            name={
              status.playing
                ? { ios: 'pause.fill', android: 'pause', web: 'pause' }
                : { ios: 'play.fill', android: 'play_arrow', web: 'play_arrow' }
            }
            color={colors.accent}
          />
        </Pressable>
        <View
          accessibilityElementsHidden
          style={{
            flex: 1,
            height: 35,
            flexDirection: 'row',
            gap: 2,
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          {bars.map((level, index) => (
            <View
              key={index}
              style={{
                width: 3,
                height: 4 + Math.max(0, Math.min(1, level)) * 28,
                borderRadius: 2,
                backgroundColor: index / bars.length <= progress ? colors.accent : colors.tertiary,
              }}
            />
          ))}
        </View>
        <Text style={{ color: colors.secondary, fontSize: 12, fontVariant: ['tabular-nums'] }}>
          {clipTime(status.playing ? status.currentTime : total)}
        </Text>
      </View>
      {total > 0 ? (
        <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Voltar 10 segundos"
            onPress={() => {
              void player.seekTo(Math.max(0, status.currentTime - 10));
            }}
            style={{ minHeight: 36, paddingHorizontal: 10, justifyContent: 'center' }}
          >
            <Text style={{ color: colors.secondary, fontSize: 12 }}>↶ 10 s</Text>
          </Pressable>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel={`Velocidade ${status.playbackRate || 1}. Alterar velocidade`}
            onPress={() => {
              player.setPlaybackRate(
                (status.playbackRate || 1) === 1 ? 1.5 : status.playbackRate === 1.5 ? 2 : 1,
              );
            }}
            style={{ minHeight: 36, paddingHorizontal: 10, justifyContent: 'center' }}
          >
            <Text style={{ color: colors.secondary, fontSize: 12 }}>
              {status.playbackRate || 1}×
            </Text>
          </Pressable>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Avançar 10 segundos"
            onPress={() => {
              void player.seekTo(Math.min(total, status.currentTime + 10));
            }}
            style={{ minHeight: 36, paddingHorizontal: 10, justifyContent: 'center' }}
          >
            <Text style={{ color: colors.secondary, fontSize: 12 }}>10 s ↷</Text>
          </Pressable>
        </View>
      ) : null}
      {failure ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger, fontSize: 13 }}>
          {failure}
        </Text>
      ) : null}
    </View>
  );
}
