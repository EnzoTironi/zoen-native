import test from 'node:test';
import assert from 'node:assert/strict';
import {
  moveBlock,
  replaceBlock,
  retargetSpans,
  planAmountInput,
  markdownFilename,
} from './editing.ts';
import { parseMoneyInput } from '../core/presentation.ts';
import type { PageBlockDto } from '../core/generated/PageBlockDto.ts';

function block(id: string): PageBlockDto {
  return {
    id,
    kind: 'paragraph',
    level: 0,
    indent: 0,
    number: 0,
    checked: false,
    lang: '',
    url: '',
    alt: '',
    text: id,
    spans: [],
  };
}
test('replacement keeps the edited stable ID, inserted IDs, and untouched order', () => {
  const observed = [block('first'), block('old'), block('last')];
  const parsed = [block('new-1'), block('new-2')];
  assert.deepEqual(replaceBlock(observed, 'old', parsed).order, ['first', 'old', 'new-2', 'last']);
  assert.equal(parsed[0]!.id, 'new-1');
  assert.deepEqual(replaceBlock(observed, 'old', []).order, ['first', 'last']);
  assert.deepEqual(
    moveBlock(
      observed.map((b) => b.id),
      'old',
      -1,
    ),
    ['old', 'first', 'last'],
  );
  assert.throws(() => replaceBlock(observed, 'missing', []));
});
test('text fallback moves custom marks without splitting emoji or losing values', () => {
  const mark = { start: 2, end: 8, key: 'future', value: 'opaque' };
  assert.deepEqual(retargetSpans('A 🪴abcZ', 'A 🌷abcZ', [mark]), [mark]);
  assert.deepEqual(retargetSpans('abcdef', 'abXYZcdef', [{ ...mark, start: 1, end: 5 }]), [
    { ...mark, start: 1, end: 8 },
  ]);
  assert.deepEqual(retargetSpans('abcdef', 'abef', [{ ...mark, start: 2, end: 4 }]), []);
  assert.deepEqual(retargetSpans('abcdef', 'Xabcdef', [{ ...mark, start: 0, end: 6 }]), [
    { ...mark, start: 1, end: 7 },
  ]);
});
test('plan costs round-trip exact signed i64 domain without Number coercion', () => {
  const maximum = '9223372036854775807';
  assert.equal(planAmountInput(maximum), '92233720368547758,07');
  assert.equal(parseMoneyInput(planAmountInput(maximum)), maximum);
  assert.equal(parseMoneyInput('9.007.199.254.740,99'), '900719925474099');
  assert.throws(() => parseMoneyInput('92233720368547758,08'));
  for (const invalid of ['-1,00', '1.2', '1,001', 'NaN', '1e5'])
    assert.throws(() => parseMoneyInput(invalid));
});
test('export filenames cannot escape their cache directory', () => {
  assert.equal(markdownFilename('../../Meu: diário\u0000'), '-..-Meu- diário-.md');
  assert.equal(markdownFilename('...'), 'pagina.md');
});
