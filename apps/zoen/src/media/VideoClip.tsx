import { useEvent } from 'expo';
import { useVideoPlayer, VideoView } from 'expo-video';
import { Text, View } from 'react-native';
import { useTheme } from '../theme';

export function VideoClip({ uri }: { uri: string }) {
  const { colors } = useTheme();
  const player = useVideoPlayer({ uri }, (value) => {
    value.allowsExternalPlayback = false;
  });
  const status = useEvent(player, 'statusChange', { status: player.status, error: undefined });
  return (
    <View style={{ gap: 10 }}>
      <VideoView
        player={player}
        style={{ width: '100%', height: 320, borderRadius: 16 }}
        contentFit="contain"
        nativeControls
        fullscreenOptions={{ enable: true }}
        playsInline
      />
      {status.error ? (
        <Text accessibilityRole="alert" style={{ color: colors.danger, fontSize: 14 }}>
          {status.error.message}
        </Text>
      ) : null}
    </View>
  );
}
