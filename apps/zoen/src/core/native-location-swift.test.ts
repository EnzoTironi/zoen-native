import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

for (const variant of [
  {
    name: 'draft-source',
    sources: ['ios/Location/ZoenDraftLocationSource.swift', 'tests/DraftLocationSourceTests.swift'],
    marker: /Draft location held-copy\/hash PASS/,
  },
  { name: 'parser', sources: ['tests/LocationPreviewTests.swift'], marker: /Location parser PASS/ },
  {
    name: 'session',
    sources: [
      'ios/ZoenSession+LocationPreview.swift',
      'ios/ZoenSession+LocationSearch.swift',
      'tests/LocationSessionTests.swift',
    ],
    marker: /Location session PASS/,
  },
  {
    name: 'search',
    sources: [
      'ios/Location/ZoenLocationLifetime.swift',
      'ios/Location/ZoenLocationMapConfiguration.swift',
      'ios/Location/ZoenLocationSearch.swift',
      'ios/Location/ZoenMapKitSearch.swift',
      'tests/LocationSearchTests.swift',
    ],
    marker: /Location search PASS/,
  },
]) {
  test(
    `actual production static-location ${variant.name} Swift contract`,
    { skip: process.platform !== 'darwin', timeout: 60000 },
    () => {
      const directory = mkdtempSync(join(tmpdir(), 'zoen-location-native-'));
      const root = new URL('../../modules/zoen-core/', import.meta.url);
      try {
        const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
          encoding: 'utf8',
          timeout: 10000,
        }).trim();
        const binary = join(directory, variant.name);
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
            '-target',
            'arm64-apple-macosx26.0',
            '-module-cache-path',
            join(directory, 'cache'),
            '-sdk',
            sdk,
            fileURLToPath(new URL('ios/Location/ZoenStaticLocation.swift', root)),
            ...variant.sources.map((path) => fileURLToPath(new URL(path, root))),
            '-o',
            binary,
          ],
          { encoding: 'utf8', timeout: 45000 },
        );
        assert.match(
          execFileSync(binary, [], { encoding: 'utf8', timeout: 10000 }),
          variant.marker,
        );
      } finally {
        rmSync(directory, { recursive: true, force: true });
      }
    },
  );
}

test(
  'MapKit Expo adapter parses as actual iOS Swift source',
  { skip: process.platform !== 'darwin', timeout: 15000 },
  () => {
    const root = new URL('../../modules/zoen-core/ios/', import.meta.url);
    execFileSync(
      'xcrun',
      [
        'swiftc',
        '-frontend',
        '-parse',
        ...[
          'Location/ZoenLocationMap.swift',
          'Location/ZoenDraftLocationGallery.swift',
          'ZoenAttachmentGalleryAssets.swift',
          'Location/ZoenLocationModule.swift',
          'ZoenCoreModule.swift',
        ].map((path) => fileURLToPath(new URL(path, root))),
      ],
      { encoding: 'utf8', timeout: 10000 },
    );
  },
);
