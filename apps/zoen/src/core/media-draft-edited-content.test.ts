import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { contentValue } from '../media/media-draft-contract.ts';

test(
  'native edited content preserves canonical Photos ownership through asset replacement',
  { skip: process.platform !== 'darwin', timeout: 60_000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-edited-gallery-'));
    try {
      const binary = join(directory, 'probe');
      const native = fileURLToPath(new URL('../../modules/zoen-core/', import.meta.url));
      execFileSync(
        'xcrun',
        [
          'swiftc',
          '-swift-version',
          '6',
          '-strict-concurrency=complete',
          '-warnings-as-errors',
          '-module-cache-path',
          join(directory, 'module-cache'),
          join(native, 'ios/ZoenMediaDraftEditedContent.swift'),
          join(native, 'tests/MediaDraftEditedContentTests.swift'),
          '-o',
          binary,
        ],
        { timeout: 45_000 },
      );
      const original = {
        kind: 'attachments',
        reply_to: 'thread-child',
        options: { presentation: 'media', grouping: 'grouped' },
        items: [
          { asset_id: 'document', mutation_id: 'send-document', caption: 'Document' },
          { asset_id: 'photo-a', mutation_id: 'send-a', caption: '  Foto 🪁\nlegenda  ' },
          { asset_id: 'photo-b', mutation_id: 'send-b', caption: 'Neighbor' },
        ],
        gallery: {
          forced_caption: false,
          entries: [
            { asset_id: 'photo-a', photo_id: 'ph://source-a' },
            { asset_id: 'photo-b', photo_id: 'ph://source-b' },
          ],
        },
      };
      const replace = (
        value: unknown,
        asset = 'photo-a',
        mutation = 'send-a',
        replacement = 'edited-a',
      ) =>
        JSON.parse(
          execFileSync(binary, [asset, mutation, replacement, 'send-edited'], {
            input: JSON.stringify(value),
            encoding: 'utf8',
          }),
        ) as unknown;
      contentValue(original);
      const result = contentValue(replace(original)).content;
      assert.equal(result?.kind, 'attachments');
      if (result?.kind !== 'attachments') assert.fail();
      assert.deepEqual(result, {
        ...original,
        items: original.items.map((item, index) =>
          index === 1 ? { ...item, asset_id: 'edited-a', mutation_id: 'send-edited' } : item,
        ),
        gallery: {
          ...original.gallery,
          entries: [
            { asset_id: 'edited-a', photo_id: 'ph://source-a' },
            original.gallery.entries[1],
          ],
        },
      });
      const { gallery: _gallery, ...ordinary } = original;
      assert.equal(contentValue(replace(ordinary)).content?.kind, 'attachments');
      const prefix = contentValue(
        replace(original, 'document', 'send-document', 'edited-doc'),
      ).content;
      assert.equal(prefix?.kind, 'attachments');
      if (prefix?.kind !== 'attachments') assert.fail();
      assert.deepEqual(prefix.gallery, original.gallery);
      assert.equal(replace(original, 'photo-a', 'retired-intent'), null);
      assert.equal(replace(original, 'photo-a', 'send-a', 'photo-b'), null);
      assert.equal(
        replace({
          ...original,
          gallery: {
            ...original.gallery,
            entries: [...original.gallery.entries].reverse(),
          },
        }),
        null,
      );
      // Reusing a source asset elsewhere does not transfer ownership of that row.
      const repeated = {
        ...original,
        items: original.items.map((item, index) =>
          index === 0 ? { ...item, asset_id: 'photo-a' } : item,
        ),
      };
      const exact = contentValue(replace(repeated)).content;
      assert.equal(exact?.kind, 'attachments');
      if (exact?.kind !== 'attachments') assert.fail();
      assert.equal(exact.items[0]!.asset_id, 'photo-a');
      assert.equal(exact.items[1]!.asset_id, 'edited-a');
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
