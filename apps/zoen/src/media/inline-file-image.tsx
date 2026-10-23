import { Image } from 'expo-image';
import { View } from 'react-native';
import { MessagePressable as Pressable } from '../components/message-interaction';
import { t } from '../i18n';
import type { MediaTile } from './file-attachment-types';
import type { LocalMediaFile } from './types';

/** Verified private URI only; decoded geometry and taps keep the originating lease. */
export function InlineFileImage({
  local,
  label,
  mine,
  tile,
  frame,
  outline,
  onActions,
  onOpen,
  isCurrent,
  onDecoded,
  onFailure,
}: {
  local: LocalMediaFile;
  label: string;
  mine: boolean;
  tile?: MediaTile;
  frame: { width: number; height: number };
  outline: string;
  onActions?: () => void;
  onOpen(): void;
  isCurrent(): boolean;
  onDecoded(width: number, height: number): void;
  onFailure(): void;
}) {
  return (
    <Pressable
      onLongPress={() => {
        if (isCurrent()) onActions?.();
      }}
      delayLongPress={350}
      accessibilityRole="button"
      accessibilityLabel={t('media.openFileLabel', { name: label })}
      onPress={() => {
        if (isCurrent()) onOpen();
      }}
      style={({ pressed }) => ({
        alignSelf: mine ? 'flex-end' : 'flex-start',
        width: tile?.width ?? (frame.width || '100%'),
        height: tile?.height ?? (frame.height || undefined),
        aspectRatio: tile || frame.width ? undefined : 1,
        borderRadius: tile ? 0 : 24,
        borderCurve: 'continuous',
        overflow: 'hidden',
        opacity: pressed ? 0.8 : 1,
      })}
    >
      <Image
        cachePolicy="none"
        recyclingKey={`${local.itemId}:${local.info.sha256}`}
        source={{ uri: local.uri }}
        accessible={false}
        contentFit={tile ? 'cover' : 'contain'}
        onLoad={(event) => {
          if (!isCurrent() || event.source.url !== local.uri) return;
          const { width, height } = event.source;
          if (Number.isFinite(width) && Number.isFinite(height) && width > 0 && height > 0)
            onDecoded(width, height);
        }}
        onError={() => {
          if (isCurrent()) onFailure();
        }}
        style={{ position: 'absolute', inset: 0 }}
      />
      <View
        pointerEvents="none"
        style={{
          position: 'absolute',
          inset: 0,
          borderWidth: tile ? 0 : 1,
          borderColor: outline,
          borderRadius: tile ? 0 : 24,
          borderCurve: 'continuous',
        }}
      />
    </Pressable>
  );
}
