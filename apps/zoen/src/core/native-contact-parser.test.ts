import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { contactVCard, sharedContact } from '../media/attachment-content.ts';

// Real Apple Contacts/CryptoKit and production parser; no fake RFC parser.
test(
  'actual vCard parser and native session boundary reject malformed, mismatched, revoked or retired source reads',
  { skip: process.platform !== 'darwin', timeout: 90000 },
  () => {
    const directory = mkdtempSync(join(tmpdir(), 'zoen-contact-parser-'));
    try {
      const sdk = execFileSync('xcrun', ['--sdk', 'macosx', '--show-sdk-path'], {
        encoding: 'utf8',
        timeout: 10000,
      }).trim();
      const source = fileURLToPath(
        new URL('../../modules/zoen-core/ios/ZoenContactPreview.swift', import.meta.url),
      );
      const variants = [
        {
          name: 'parser',
          sources: [
            fileURLToPath(
              new URL('../../modules/zoen-core/tests/ContactPreviewTests.swift', import.meta.url),
            ),
          ],
          marker: /Contact parser PASS/,
        },
        {
          name: 'session',
          sources: [
            fileURLToPath(
              new URL(
                '../../modules/zoen-core/ios/ZoenSession+ContactPreview.swift',
                import.meta.url,
              ),
            ),
            fileURLToPath(
              new URL('../../modules/zoen-core/tests/ContactSessionTests.swift', import.meta.url),
            ),
          ],
          marker: /Contact session PASS/,
        },
      ];
      for (const variant of variants) {
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
            '-module-cache-path',
            join(directory, 'module-cache'),
            '-sdk',
            sdk,
            source,
            ...variant.sources,
            '-o',
            binary,
          ],
          { encoding: 'utf8', timeout: 45000 },
        );
        assert.match(
          execFileSync(binary, [], { encoding: 'utf8', timeout: 10000 }),
          variant.marker,
        );
        if (variant.name === 'parser') {
          const reviewed = sharedContact('Contato ' + 'é'.repeat(65) + ', Silva; C\\D', [
            '+123;456',
            '+55 11 1234',
          ]);
          const parsed: unknown = JSON.parse(
            execFileSync(binary, ['stdin'], {
              input: contactVCard(reviewed),
              encoding: 'utf8',
              timeout: 10000,
            }),
          );
          assert.ok(typeof parsed === 'object' && parsed && 'name' in parsed && 'phones' in parsed);
          assert.equal(
            parsed.name,
            reviewed.name,
            'actual reviewed writer UTF-8 folding/escaping survives the Apple parser',
          );
          assert.deepEqual(
            (parsed.phones as { value: string }[]).map((phone) => phone.value),
            reviewed.phones,
          );
        }
      }
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
