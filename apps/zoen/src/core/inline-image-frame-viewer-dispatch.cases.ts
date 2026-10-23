import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Element } from './inline-image-frame.fixtures.ts';
import { elements, fixture, style, tick } from './inline-image-frame.fixtures.ts';
import { type MediaAlbum } from './media-albums.ts';
import type { TimelineEntry } from './types';

test('actual HTML attachment opens its selected Site version without generic materialization or preview', async () => {
  const f = fixture(false, { info: { name: 'site.html', mime: 'text/html', bytes: '20' } });
  await f.ready();
  const button = elements(f.render()).find((node) => node.type === 'MessagePressable')!;
  assert.ok(button);
  (button.props.onPress as () => void)();
  await tick();
  assert.deepEqual(JSON.parse(JSON.stringify(f.navigationRequests)), [
    { pathname: '/site/[id]', params: { id: 'item', version: '3' } },
  ]);
  assert.equal(f.materialized.length, 0);
  assert.equal(f.legacyPreviews.length, 0);
  f.unmount();
});

test('actual image/video Open preserves the exact single-file preview for forwarded visual messages with gallery capability present', async () => {
  for (const mime of ['image/jpeg', 'video/mp4']) {
    const entry = { id: 'forwarded-message', forwarded: true, voice: null } as TimelineEntry;
    const f = fixture(false, {
      info: { mime },
      galleryCapability: true,
      messageSource: { spaceId: 'qa', entry },
    });
    await f.ready();
    const button = mime.startsWith('image/')
      ? f.parts().frame
      : elements(f.render()).find((node) => node.props.testID === 'message-video-gallery-item')!;
    assert.ok(button, 'Actual visual media Open control');
    (button.props.onPress as () => void)();
    await tick();
    assert.equal(
      f.galleryRequests.length,
      0,
      'Forwarded media must not enter signed-album request helper',
    );
    assert.equal(f.legacyPreviews.length, 1);
    assert.strictEqual(f.legacyPreviews[0], f.file);
    assert.deepEqual(f.materialized.at(-1), { itemId: 'item', version: 3 });
    f.unmount();
  }
});
test('actual ordinary image Open still dispatches the chosen canonical message to the gallery helper', async () => {
  const entry = { id: 'ordinary-message', forwarded: false, voice: null } as TimelineEntry;
  const f = fixture(false, { galleryCapability: true, messageSource: { spaceId: 'qa', entry } });
  await f.ready();
  (f.parts().frame.props.onPress as () => void)();
  await tick();
  assert.equal(f.legacyPreviews.length, 0);
  assert.equal(f.galleryRequests.length, 1);
  assert.equal(f.galleryRequests[0]![1], 'qa');
  assert.equal(f.galleryRequests[0]![2], entry.id);
  assert.strictEqual((f.galleryRequests[0]![3] as TimelineEntry[])[0], entry);
  f.unmount();
});

for (const failure of ['abort', 'background', 'busy', 'unavailable', 'cleanup'] as const) {
  test(`actual image Open through the real gallery helper keeps the signed album after native ${failure}`, async () => {
    const parent = fixture();
    const album: MediaAlbum = {
      id: 'qa-signed-group',
      spaceId: 'qa',
      presentation: 'media',
      entries: [0, 1].map(
        (index) =>
          ({
            id: `member-${index}`,
            author: { id: 'qa', is_me: true },
            delivery: 'Sent',
            deleted: false,
            forwarded: false,
            voice: null,
            kind: {
              Message: {
                text: `Caption ${index}`,
                card: { item_id: index ? 'other' : 'item', kind_id: 'file', version: 3 },
              },
            },
            media: {
              presentation: 'media',
              group: { id: `album_${'a'.repeat(64)}`, index, count: 2 },
            },
          }) as TimelineEntry,
      ),
    };
    const before = parent.renderAlbum(album) as Element;
    const media = before.props.albumContent as { media: Element };
    const leaf = (media.media.props.children as Element[])[0]!.props.children as Element;
    const file = fixture(true, {
      onInlineUnavailable: (leaf.props.albumPart as { onInlineUnavailable(): void })
        .onInlineUnavailable,
      messageSource: { spaceId: 'qa', entry: album.entries[0]! },
      galleryEntries: album.entries,
      galleryCapability: true,
      actualGallery: true,
      tile: (leaf.props.albumPart as { tile: { width: number; height: number } }).tile,
    });
    await file.ready();
    (file.parts().frame.props.onPress as () => void)();
    await tick();
    assert.equal(
      file.nativeIsOpen(),
      true,
      'Real helper queried canonical children before native admission',
    );
    if (failure === 'abort') file.abortGallery();
    else
      file.rejectNative(
        new Error(
          failure === 'background'
            ? 'Synthetic native cancellation'
            : `Synthetic presenter ${failure}`,
        ),
      );
    await tick();
    const after = file.render();
    await tick();
    assert.equal(
      file.galleryListenerCount(),
      0,
      'Rejecting native abort settles cleanup rather than leaving listeners',
    );
    const alert = elements(after).find((node) => node.props.accessibilityRole === 'alert');
    assert.ok(alert, 'A real rejection remains accessible rather than being ignored');
    assert.equal(
      style(alert).position,
      'absolute',
      'Action feedback must overlay the clipped tile rather than flow below it',
    );
    assert.equal(style(alert).bottom, 0);
    assert.equal(style(alert).left, 0);
    assert.equal(style(alert).right, 0);
    assert.equal(style(alert).maxHeight, style(after).height);
    assert.equal(alert.props.numberOfLines, 2);
    assert.ok(
      !Array.isArray(parent.renderAlbum(album)),
      `${failure} must not disqualify canonical inline content`,
    );
    file.unmount();
    parent.unmount();
  });
}
