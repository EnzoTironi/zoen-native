import { useEvent } from 'expo';
import { useVideoPlayer, VideoView } from 'expo-video';
import { ActivityIndicator, Text, View } from 'react-native';
import { useTheme } from '../theme';

export function VideoClip({ uri, compact = false }: { uri: string; compact?: boolean }) {
  return <VideoClipContent key={uri} uri={uri} compact={compact} />;
}

function VideoClipContent({ uri, compact }: { uri: string; compact: boolean }) {
  const { colors } = useTheme();
  const player = useVideoPlayer({ uri }, (value) => {
    value.allowsExternalPlayback = false;
  });
  const status = useEvent(player, 'statusChange', { status: player.status, error: undefined });
  const source = useEvent(player, 'sourceLoad', {
    videoSource: { uri },
    duration: player.duration,
    availableAudioTracks: [],
    availableVideoTracks: player.availableVideoTracks,
    availableSubtitleTracks: [],
  });
  const size = source?.availableVideoTracks[0]?.size;
  const ratio = size && size.width > 0 && size.height > 0 ? size.width / size.height : 16 / 9;
  return (
    <View style={{ width: '100%', minWidth: 0, gap: 10 }}>
      <View
        style={{
          borderRadius: compact ? 24 : 16,
          borderCurve: 'continuous',
          overflow: 'hidden',
          backgroundColor: '#000000',
        }}
      >
        <VideoView
          player={player}
          style={
            compact
              ? { width: '100%', aspectRatio: Math.max(0.75, Math.min(1.9, ratio)), maxHeight: 360 }
              : { width: '100%', height: 320 }
          }
          contentFit="contain"
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
      {status.error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger, fontSize: 14 }}>
          {status.error.message}
        </Text>
      ) : null}
    </View>
  );
}
