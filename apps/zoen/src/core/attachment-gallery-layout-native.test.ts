import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

test(
  'actual native selected-photo review keeps the full media pager instead of a clipped message summary',
  { skip: process.platform !== 'darwin', timeout: 150000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-gallery-layout-'));
    try {
      const main = join(directory, 'main.swift');
      writeFileSync(
        main,
        `import Foundation
        import CoreGraphics
        let screens = Dictionary(uniqueKeysWithValues: ["grid", "albums", "preview", "selected"].map {
          ($0, ["fullScreen": ZoenAttachmentGalleryLayout.prefersFullScreen(screen: $0),
                 "pager": ZoenAttachmentGalleryLayout.usesPager(screen: $0)])
        })
        func rect(_ value: CGRect) -> [Double] {
          [value.minX, value.minY, value.width, value.height].map { Double($0) }
        }
        let viewport = CGSize(width: 390, height: 844)
        let withoutKeyboard = ZoenAttachmentGalleryLayout.mediaFrame(size: viewport, keyboardHeight: 0)
        let withKeyboard = ZoenAttachmentGalleryLayout.mediaFrame(size: viewport, keyboardHeight: 300)
        let media: [[Double]] = [rect(withoutKeyboard), rect(withKeyboard),
            rect(ZoenAttachmentGalleryLayout.mediaFrame(size: viewport, keyboardHeight: -1)),
            rect(ZoenAttachmentGalleryLayout.mediaFrame(size: viewport, keyboardHeight: 900)),
            rect(ZoenAttachmentGalleryLayout.mediaFrame(size: viewport, keyboardHeight: .nan))]
        let fit: [[Double]] = [
            rect(ZoenAttachmentGalleryLayout.aspectFit(dimensions: CGSize(width: 1600, height: 900), viewport: withoutKeyboard)),
            rect(ZoenAttachmentGalleryLayout.aspectFit(dimensions: CGSize(width: 900, height: 1600), viewport: withoutKeyboard)),
            rect(ZoenAttachmentGalleryLayout.aspectFit(dimensions: CGSize(width: 1600, height: 900), viewport: withKeyboard)),
            rect(ZoenAttachmentGalleryLayout.aspectFit(dimensions: .zero, viewport: withoutKeyboard)),
            rect(ZoenAttachmentGalleryLayout.aspectFit(dimensions: viewport, viewport: CGRect(x: 0, y: 0, width: CGFloat.infinity, height: 1)))]
        let orders: [[String]] = [
            ZoenAttachmentGalleryLayout.reordered(["a", "b", "c"], from: 0, to: 2) ?? [],
            ZoenAttachmentGalleryLayout.reordered(["a", "b", "c"], from: 2, to: 0) ?? [],
            ZoenAttachmentGalleryLayout.reordered(["a", "b", "c"], from: 1, to: 1) ?? []]
        let invalidReorders: [Bool] = [
            ZoenAttachmentGalleryLayout.reordered(["a", "a"], from: 0, to: 1) == nil,
            ZoenAttachmentGalleryLayout.reordered(["a"], from: -1, to: 0) == nil,
            ZoenAttachmentGalleryLayout.reordered(["a"], from: 0, to: 1) == nil,
            ZoenAttachmentGalleryLayout.reordered((0..<11).map(String.init), from: 0, to: 1) == nil]
        let result: [String: Any] = ["screens": screens, "media": media, "fit": fit,
          "orders": orders, "invalidReorders": invalidReorders]
        print(String(decoding: try JSONSerialization.data(withJSONObject: result), as: UTF8.self))
        `,
      );
      const binary = join(directory, 'gallery-layout');
      const source = fileURLToPath(
        new URL('../../modules/zoen-core/ios/ZoenAttachmentGalleryLayout.swift', import.meta.url),
      );
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-j',
          '1',
          '-module-cache-path',
          join(directory, 'modules'),
          source,
          main,
          '-o',
          binary,
        ],
        { encoding: 'utf8', timeout: 120000 },
      );
      const result = JSON.parse(execFileSync(binary, [], { encoding: 'utf8', timeout: 10000 }));
      assert.equal(result.screens.grid.fullScreen, false);
      assert.equal(result.screens.albums.fullScreen, false);
      assert.equal(result.screens.preview.fullScreen, true);
      assert.equal(result.screens.preview.pager, true);
      assert.equal(
        result.screens.selected.fullScreen,
        true,
        'ordered selection review fills the screen',
      );
      assert.equal(result.screens.selected.pager, true, 'review retains aspect-fit media paging');
      assert.deepEqual(result.media, [
        [0, 0, 390, 844],
        [0, -150, 390, 844],
        [0, 0, 390, 844],
        [0, -422, 390, 844],
        [0, 0, 0, 0],
      ]);
      assert.deepEqual(result.fit[0], [0, 312.3125, 390, 219.375]);
      assert.deepEqual(result.fit[1], [0, (844 - (390 * 1600) / 900) / 2, 390, (390 * 1600) / 900]);
      assert.deepEqual(result.fit[2], [0, 162.3125, 390, 219.375]);
      assert.deepEqual(result.fit.slice(3), [
        [0, 0, 0, 0],
        [0, 0, 0, 0],
      ]);
      assert.deepEqual(result.orders, [
        ['b', 'c', 'a'],
        ['c', 'a', 'b'],
        ['a', 'b', 'c'],
      ]);
      assert.deepEqual(result.invalidReorders, [true, true, true, true]);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
