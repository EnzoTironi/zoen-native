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
        let result = Dictionary(uniqueKeysWithValues: ["grid", "albums", "preview", "selected"].map {
          ($0, ["fullScreen": ZoenAttachmentGalleryLayout.prefersFullScreen(screen: $0),
                 "pager": ZoenAttachmentGalleryLayout.usesPager(screen: $0)])
        })
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
      assert.equal(result.grid.fullScreen, false);
      assert.equal(result.albums.fullScreen, false);
      assert.equal(result.preview.fullScreen, true);
      assert.equal(result.preview.pager, true);
      assert.equal(result.selected.fullScreen, true, 'ordered selection review fills the screen');
      assert.equal(result.selected.pager, true, 'review retains aspect-fit media paging');
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
