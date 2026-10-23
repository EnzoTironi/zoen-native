import assert from 'node:assert/strict';
import { test } from 'node:test';
import { formatFileSize } from '../media/file-size.ts';
import { contrastRatio, resolveSkyTheme, skyBackdrops } from '../theme/sky.ts';
import { DEFAULT_THEME_CUSTOMIZATION } from '../theme/telegram.ts';
import { elements, fixture, near, style } from './inline-image-frame.fixtures.ts';

test('actual inline image resolves one bounded frame and both image and rounded mask fill it without cropping', async () => {
  for (const mine of [true, false]) {
    const f = fixture(mine);
    await f.ready();
    f.layout(294);
    for (const [width, height, expectedWidth, expectedHeight] of [
      [700, 1000, 252, 360],
      [600, 1000, 216, 360],
      [1000, 1000, 294, 294],
      [2000, 1000, 294, 147],
      [100, 1000, 36, 360],
    ]) {
      f.load(width!, height!);
      const { frame, image, mask } = f.parts();
      near(style(frame).width, expectedWidth!);
      near(style(frame).height, expectedHeight!);
      near(Number(style(frame).width) / Number(style(frame).height), width! / height!);
      assert.equal(style(frame).alignSelf, mine ? 'flex-end' : 'flex-start');
      assert.equal(style(frame).overflow, 'hidden');
      assert.equal(style(frame).borderRadius, style(mask).borderRadius);
      assert.equal(style(image).position, 'absolute');
      assert.equal(style(image).inset, 0);
      assert.equal(style(mask).inset, 0);
      assert.equal(style(image).aspectRatio, undefined);
      assert.equal(style(image).maxHeight, undefined);
      assert.equal(image.props.contentFit, 'contain');
    }
    assert.equal(f.decoded.at(-1)!.sha256, f.file.info.sha256);
    assert.equal(f.decoded.at(-1)!.itemId, 'item');
    assert.equal(f.decoded.at(-1)!.version, 3);
    f.unmount();
  }
});

test('actual inline image recomputes both dimensions after available width changes and ignores invalid or stale load events', async () => {
  const f = fixture();
  await f.ready();
  f.layout(294);
  f.load(2000, 1000);
  near(style(f.parts().frame).height, 147);
  f.layout(144);
  near(style(f.parts().frame).width, 144);
  near(style(f.parts().frame).height, 72);
  f.layout(294);
  near(style(f.parts().frame).width, 294);
  f.load(700, 1000);
  const accepted = f.decoded.length;
  f.load(1000, 1, 'file:///synthetic/old-source.jpg');
  f.load(0, 1000);
  f.load(Number.NaN, 1000);
  assert.equal(f.decoded.length, accepted);
  near(style(f.parts().frame).width, 252);
  f.layout(Number.NaN);
  near(style(f.parts().frame).width, 252);
  const late = f.parts().image.props.onLoad as (event: unknown) => void;
  f.unmount();
  late({ source: { url: f.file.uri, width: 1000, height: 1 } });
  assert.equal(f.decoded.length, accepted);
});

test('generic files and oversized image fallbacks use resolved sky ink independently of outgoing bubble color', async () => {
  for (const phase of ['night', 'day'] as const) {
    for (const outgoing of ['#FFFFFF', '#000000']) {
      const theme = resolveSkyTheme(phase, new Date('2026-10-18T12:00:00'), {
        ...DEFAULT_THEME_CUSTOMIZATION,
        outgoing,
      });
      for (const mine of [true, false]) {
        for (const mime of ['image/png', 'application/pdf']) {
          const f = fixture(mine, { theme, info: { name: 'large-file', mime, bytes: '52533657' } });
          await f.ready();
          const tree = f.render();
          assert.equal(
            elements(tree).some((node) => node.type === 'Image'),
            false,
          );
          const icon = elements(tree).find((node) => node.type === 'Icon')!;
          const [name, size] = elements(tree).filter((node) => node.type === 'Text');
          assert.equal(size!.props.children, formatFileSize(f.file.info.bytes, 'en-US'));
          assert.equal(style(tree).backgroundColor, 'transparent');
          assert.equal(icon.props.color, theme.colors.text);
          assert.equal(style(name!).color, theme.colors.text);
          assert.equal(style(size!).color, theme.colors.secondary);
          assert.equal(
            style(size!).opacity,
            undefined,
            'The resolved secondary ink already accounts for the sky',
          );
          for (const background of skyBackdrops(theme.sky)) {
            assert.ok(contrastRatio(style(name!).color as string, background) >= 4.5);
            assert.ok(contrastRatio(style(size!).color as string, background) >= 4.5);
          }
          f.unmount();
        }
      }
    }
  }
});

test('outgoing voice fallback retains its resolved bubble ink', async () => {
  const theme = resolveSkyTheme('night', new Date('2026-10-18T12:00:00'), {
    ...DEFAULT_THEME_CUSTOMIZATION,
    outgoing: '#FFFFFF',
  });
  const f = fixture(true, {
    theme,
    info: { mime: 'audio/aac' },
    voice: { ms: '1000', levels: [0.2] },
  });
  await f.ready();
  const icon = elements(f.render()).find((node) => node.type === 'Icon')!;
  assert.equal(icon.props.color, theme.colors.onMine);
  f.unmount();
});
