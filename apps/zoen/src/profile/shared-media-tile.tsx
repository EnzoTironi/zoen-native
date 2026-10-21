import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ActivityIndicator, Pressable, Text, View } from 'react-native';
import { Image } from 'expo-image';
import { Icon } from '../components/primitives';
import { useClient } from '../core/provider';
import type { ItemDetail } from '../core/types';
import type { LocalMediaFile, MediaDriver } from '../media/types';
import { previewMediaFile } from '../media/preview-file';
import { t } from '../i18n';
import { useTheme } from '../theme';

export async function materializeSharedFile(
  driver: MediaDriver,
  item: ItemDetail,
): Promise<LocalMediaFile | null> {
  const file = await driver.materializeFile(item.id, item.version);
  if (!file) return null;
  if (
    !item.file ||
    file.itemId !== item.id ||
    file.version !== item.version ||
    file.info.sha256 !== item.file.sha256 ||
    file.info.mime !== item.file.mime ||
    file.info.bytes !== item.file.bytes
  ) {
    await driver.releaseLocalFile(file.uri);
    throw new Error(t('media.thisFileCouldNotBeOpened'));
  }
  return file;
}
/** A visible grid cell owns a verified disposable thumbnail copy, never a durable media cache. */
export function SharedMediaTile({
  item,
  owner,
  isCurrent,
}: {
  item: ItemDetail;
  owner: string;
  isCurrent: () => boolean;
}) {
  const { store } = useClient();
  const driver = store.driver.media;
  const { presentation: p } = useTheme();
  const key = `${owner}:${item.id}:${item.version}:${item.file?.sha256 ?? ''}:${item.file?.mime ?? ''}:${item.file?.bytes ?? ''}`;
  const ready = Boolean(item.file?.ready);
  const latestItem = useRef(item);
  useLayoutEffect(() => {
    latestItem.current = item;
  });
  const [preview, setPreview] = useState<{ key: string; uri: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  const opening = useRef(false);
  const image = item.file?.mime.startsWith('image/');
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    if (!driver || !image || !ready || Number(latestItem.current.file?.bytes ?? 0) > 16 * 1048576)
      return;
    let active = true;
    let owned: LocalMediaFile | null = null;
    void materializeSharedFile(driver, latestItem.current)
      .then(async (file) => {
        if (!file) return;
        if (!active || !isCurrent()) {
          await driver.releaseLocalFile(file.uri);
          return;
        }
        owned = file;
        setPreview({ key, uri: file.uri });
      })
      .catch(() => {});
    return () => {
      active = false;
      if (owned) void driver.releaseLocalFile(owned.uri).catch(() => {});
    };
  }, [driver, image, key, ready, isCurrent]);
  async function open() {
    if (!driver || opening.current || !mounted.current || !isCurrent()) return;
    opening.current = true;
    setBusy(true);
    setError(null);
    try {
      const file = await materializeSharedFile(driver, item);
      if (!file) throw new Error(t('media.thisFileIsStillArrivingTryAgainWhen'));
      if (!mounted.current || !isCurrent()) {
        await driver.releaseLocalFile(file.uri);
        return;
      }
      // The existing native preview retains/releases its separate immutable-version copy.
      await previewMediaFile(driver, file);
    } catch (failure) {
      if (mounted.current && isCurrent())
        setError(failure instanceof Error ? failure.message : t('media.thisFileCouldNotBeOpened'));
    } finally {
      opening.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <Pressable
      testID={`shared-media-${item.id}`}
      accessibilityRole="button"
      accessibilityLabel={t('peerInfo.openItem', { name: item.file?.name ?? item.title })}
      accessibilityState={{ busy, disabled: !driver }}
      disabled={!driver}
      onPress={() => {
        void open();
      }}
      style={({ pressed }) => ({
        flex: 1,
        aspectRatio: 1,
        margin: 0.5,
        overflow: 'hidden',
        backgroundColor: p.list.block,
        opacity: pressed ? 0.75 : 1,
        alignItems: 'center',
        justifyContent: 'center',
      })}
    >
      {preview?.key === key ? (
        <Image
          cachePolicy="none"
          recyclingKey={key}
          source={{ uri: preview.uri }}
          contentFit="cover"
          accessible={false}
          style={{ position: 'absolute', inset: 0 }}
        />
      ) : (
        <Icon
          name={
            image
              ? { ios: 'photo', android: 'image', web: 'image' }
              : { ios: 'play.rectangle.fill', android: 'movie', web: 'movie' }
          }
          size={28}
          color={p.list.secondary}
        />
      )}
      {busy ? <ActivityIndicator color={p.list.accent} /> : null}
      {error ? (
        <Text
          accessibilityRole="alert"
          numberOfLines={3}
          style={{ color: p.destructive, fontSize: 12, padding: 6 }}
        >
          {error}
        </Text>
      ) : null}
      {!image ? (
        <View style={{ position: 'absolute', left: 6, right: 6, bottom: 5 }}>
          <Text numberOfLines={1} style={{ color: p.list.text, fontSize: 11 }}>
            {item.file?.name ?? item.title}
          </Text>
        </View>
      ) : null}
    </Pressable>
  );
}
