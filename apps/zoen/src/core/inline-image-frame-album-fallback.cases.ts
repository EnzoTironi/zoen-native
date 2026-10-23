import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Element } from './inline-image-frame.fixtures.ts';
import { fixture } from './inline-image-frame.fixtures.ts';
import { type MediaAlbum } from './media-albums.ts';
import { messageCard } from './message-presentation.ts';
import type { TimelineEntry } from './types';

test('actual canonical file metadata releases a visual album to individual rows for generic or oversized members only', async () => {
  for (const [mime, bytes, expected] of [
    ['application/pdf', '1000', 1],
    ['audio/mp4', '1000', 1],
    ['image/png', String(16 * 1048576 + 1), 1],
    ['image/png', '1000', 0],
    ['video/mp4', '1000', 0],
  ] as const) {
    let unavailable = 0;
    const f = fixture(true, {
      info: { mime, bytes },
      onInlineUnavailable: () => {
        unavailable++;
      },
    });
    f.render();
    assert.equal(unavailable, 0, 'Unknown metadata must not invent a file classification');
    await f.ready();
    assert.equal(unavailable, expected, mime);
    f.render();
    assert.equal(
      unavailable,
      expected,
      'Unchanged metadata must not publish another classification',
    );
    f.unmount();
  }
});

test('actual metadata, materialize and image decode failures invoke the production album callback and retain EVERY canonical row', async () => {
  for (const failure of ['fileInfoError', 'materializeError', 'decode'] as const) {
    const parent = fixture();
    const album: MediaAlbum = {
      id: 'qa-signed-group',
      spaceId: 'qa',
      presentation: 'media',
      entries: [0, 1].map(
        (index) =>
          ({
            id: `member-${index}`,
            author: { is_me: true },
            delivery: 'Sent',
            kind: {
              Message: {
                text: `Caption ${index}`,
                card: { item_id: index ? 'other' : 'item', kind_id: 'file', version: 3 },
              },
            },
          }) as TimelineEntry,
      ),
    };
    const first = parent.renderAlbum(album) as Element;
    const media = first.props.albumContent as { media: Element };
    const leaf = (media.media.props.children as Element[])[0]!.props.children as Element;
    const part = leaf.props.albumPart as { onInlineUnavailable(): void };
    const file = fixture(true, {
      onInlineUnavailable: part.onInlineUnavailable,
      ...(failure === 'decode' ? {} : { [failure]: true }),
    });
    await file.ready();
    if (failure === 'decode') {
      const failedImage = file.parts().image;
      (failedImage.props.onError as () => void)();
      file.render();
    }
    const rows = parent.renderAlbum(album) as Element[];
    assert.ok(Array.isArray(rows), `${failure} must release the clipped mosaic`);
    assert.deepEqual(
      rows.map((row) => row.props.entry),
      album.entries,
    );
    assert.ok(rows.every((row) => row.type === 'CanonicalConversationMessage'));
    // A late error from the old version cannot disband a newly rendered album.
    const successor = {
      ...album,
      entries: album.entries.map((entry, index) =>
        index
          ? entry
          : {
              ...entry,
              kind: {
                Message: { text: 'Successor', card: { ...messageCard(entry)!, version: 4 } },
              },
            },
      ),
    };
    assert.ok(!Array.isArray(parent.renderAlbum(successor)));
    part.onInlineUnavailable();
    assert.ok(!Array.isArray(parent.renderAlbum(successor)));
    file.unmount();
    parent.unmount();
  }
});
