import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';

function method(source: string, marker: string) {
  const start = source.indexOf(marker);
  assert.ok(start >= 0, `Actual native method missing: ${marker}`);
  const open = source.indexOf('{', start);
  let depth = 1,
    end = open + 1;
  while (end < source.length && depth) {
    if (source[end] === '{') depth++;
    if (source[end] === '}') depth--;
    end++;
  }
  assert.equal(depth, 0);
  return source.slice(start, end);
}

test(
  'actual patched renderer admits body and protects all existing interactive attribute kinds',
  { skip: process.platform !== 'darwin', timeout: 60000 },
  () => {
    const app = fileURLToPath(new URL('../../', import.meta.url));
    const directory = mkdtempSync(join(tmpdir(), 'zoen-context-admission-'));
    try {
      const relative = 'node_modules/react-native-enriched-markdown/ios/EnrichedMarkdownText.mm';
      mkdirSync(join(directory, relative, '..'), { recursive: true });
      cpSync(join(app, relative), join(directory, relative));
      const marker = '- (NSNumber *)zoenMessageContextBodyAtPoint:';
      if (!readFileSync(join(directory, relative), 'utf8').includes(marker)) {
        execFileSync(
          'patch',
          [
            '--batch',
            '--forward',
            '-p1',
            '-i',
            join(
              app,
              'patches/react-native-enriched-markdown+1.1.1+008+message-context-admission.patch',
            ),
          ],
          { cwd: directory },
        );
      }
      writeFileSync(
        join(directory, 'admission.inc'),
        method(readFileSync(join(directory, relative), 'utf8'), marker),
      );
      writeFileSync(
        join(directory, 'classifier.inc'),
        method(
          readFileSync(
            join(app, 'node_modules/react-native-enriched-markdown/ios/utils/LinkTapUtils.m'),
            'utf8',
          ),
          'BOOL isPointOnInteractiveElement(',
        ),
      );
      execFileSync(
        'xcrun',
        [
          '--sdk',
          'macosx',
          'clang',
          '-fobjc-arc',
          '-I' + directory,
          join(app, 'modules/zoen-core/tests/MessageContext/RendererAdmissionTests.m'),
          '-framework',
          'Foundation',
          '-framework',
          'CoreGraphics',
          '-o',
          join(directory, 'admission'),
        ],
        { timeout: 30000, encoding: 'utf8' },
      );
      assert.match(
        execFileSync(join(directory, 'admission'), { encoding: 'utf8' }),
        /Actual renderer admission \+ interactive attributes PASS/,
      );
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  },
);
