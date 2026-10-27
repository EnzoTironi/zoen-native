import { preparePackage } from './message-markdown-native.fixtures.ts';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
      const pkg = preparePackage(app, directory);
      const source = readFileSync(join(pkg, 'ios/views/EnrichedMarkdownInternalText.m'), 'utf8');
      writeFileSync(
        join(directory, 'admission.inc'),
        method(source, '- (NSNumber *)zoenMessageContextBodyAtPoint:'),
      );
      writeFileSync(
        join(directory, 'admission-helper.inc'),
        method(
          readFileSync(join(pkg, 'ios/utils/ENRMMessageContextAdmission.h'), 'utf8'),
          'static inline NSNumber *ENRMMessageContextBodyAtPoint(',
        ),
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
