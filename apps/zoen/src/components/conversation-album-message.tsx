import { useLayoutEffect, useRef, useState } from 'react';
import { View, useWindowDimensions } from 'react-native';
import type { MediaAlbum } from '../core/media-albums';
import { albumDelivery } from '../core/media-albums';
import { messageCard } from '../core/message-presentation';
import { messageText } from '../core/presentation';
import type { DecodedMessageImage } from '../media/inline-image-frame';
import { mediaMosaicLayout } from '../media/mosaic-layout';
import { useTheme } from '../theme';
import { ConversationMessage, type ConversationMessageProps } from './conversation-message';

/** One visual album; every cell still uses its canonical message's interaction,
 * preview/copy lease, caption, reply/thread target and menu callbacks. */
export function ConversationAlbumMessage({
  album,
  ...message
}: ConversationMessageProps & { album: MediaAlbum }) {
  const { width, scale } = useWindowDimensions();
  const { presentation: p } = useTheme();
  const [decoded, setDecoded] = useState<Record<string, DecodedMessageImage>>({});
  const [unavailable, setUnavailable] = useState<ReadonlySet<string>>(new Set());
  const current = useRef<MediaAlbum | null>(album);
  useLayoutEffect(() => {
    current.current = album;
    return () => {
      current.current = null;
    };
  }, [album]);
  const gutter = !message.entry.author.is_me && message.groupChat ? 44 : 0;
  const available = Math.max(0, width - 32 - gutter);
  const limit = Math.min(available, Math.max(160, Math.min(294, available * 0.87)));
  const sizes = album.entries.map((entry): readonly [number, number] => {
    const image = decoded[entry.id];
    const card = messageCard(entry);
    return image && image.itemId === card?.item_id && image.version === card.version
      ? [image.width, image.height]
      : [1, 1];
  });
  const key = (entry: MediaAlbum['entries'][number]) =>
    JSON.stringify([entry.id, messageCard(entry)?.item_id, messageCard(entry)?.version]);
  const layout = album.entries.some((entry) => unavailable.has(key(entry)))
    ? null
    : mediaMosaicLayout(limit, scale, sizes);
  const cannotInline = (entry: MediaAlbum['entries'][number]) => {
    if (!current.current?.entries.some((value) => key(value) === key(entry))) return;
    setUnavailable((previous) =>
      previous.has(key(entry)) ? previous : new Set([...previous, key(entry)]),
    );
  };
  const receive = (id: string, image: DecodedMessageImage) => {
    const entry = current.current?.entries.find((value) => value.id === id);
    const card = entry && messageCard(entry);
    if (
      !card ||
      image.itemId !== card.item_id ||
      image.version !== card.version ||
      ![image.width, image.height].every((value) => Number.isFinite(value) && value > 0)
    )
      return;
    setDecoded((previous) => {
      const value = previous[id];
      return value?.sha256 === image.sha256 &&
        value.version === image.version &&
        value.width === image.width &&
        value.height === image.height
        ? previous
        : { ...previous, [id]: image };
    });
  };
  if (!layout)
    return album.entries.map((entry, index) => (
      <ConversationMessage
        key={entry.id}
        {...message}
        entry={entry}
        previous={album.entries[index - 1] ?? message.previous}
        next={album.entries[index + 1] ?? message.next}
      />
    ));
  const captions = album.entries.filter((entry) => messageText(entry).trim());
  return (
    <ConversationMessage
      {...message}
      albumContent={{
        width: layout.width,
        delivery: albumDelivery(album.entries),
        media: (
          <View
            testID={`message-album-${album.entries[0]!.id}`}
            style={{
              width: layout.width,
              height: layout.height,
              borderRadius: 22,
              borderCurve: 'continuous',
              overflow: 'hidden',
              backgroundColor: message.entry.author.is_me
                ? p.chat.outgoing.fill
                : p.chat.incoming.fill,
            }}
          >
            {album.entries.map((entry, index) => {
              const frame = layout.items[index]!;
              return (
                <View
                  key={entry.id}
                  style={{
                    position: 'absolute',
                    left: frame.x,
                    top: frame.y,
                    width: frame.width,
                    height: frame.height,
                    overflow: 'hidden',
                  }}
                >
                  <ConversationMessage
                    {...message}
                    entry={entry}
                    albumPart={{
                      tile: { width: frame.width, height: frame.height },
                      onDecoded: (image) => receive(entry.id, image),
                      onInlineUnavailable: () => cannotInline(entry),
                    }}
                  />
                </View>
              );
            })}
          </View>
        ),
        captions: captions.length ? (
          <View
            style={{
              marginTop: 6,
              borderRadius: 22,
              borderCurve: 'continuous',
              overflow: 'hidden',
            }}
          >
            {captions.map((entry) => (
              <ConversationMessage
                key={entry.id}
                {...message}
                entry={entry}
                albumPart={{ captionOnly: true }}
              />
            ))}
          </View>
        ) : null,
      }}
    />
  );
}
