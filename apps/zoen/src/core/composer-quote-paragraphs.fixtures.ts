import assert from 'node:assert/strict';
type Range = { type: number; start: number; length: number };
type Result = {
  plain: string;
  markdown: string;
  ranges: Range[];
  available: boolean;
  reparsedPlain: string;
  reparsedRanges: Range[];
  reparsedBlocks: number[][];
};
export function assertComposerQuoteRoundTrips(
  run: (markdown: string, options?: Record<string, unknown>) => Result,
) {
  const quoteDraft = 'QA758 itálico riscado\ncitação\n*link* café🦊 literal *x* [a](b) `z`.';
  const quoteStart = quoteDraft.indexOf('citação');
  const quoteOptions = {
    edit: { start: 0, delete: 0, text: quoteDraft },
    toggles: [
      { type: 1, start: quoteDraft.indexOf('itálico'), length: 7 },
      { type: 3, start: quoteDraft.indexOf('riscado'), length: 7 },
    ],
    blockAvailability: { start: quoteStart, length: 7 },
    toggleQuotes: [[quoteStart, 7]],
  };
  const quotedDraft = run('', quoteOptions);
  assert.equal(quotedDraft.available, true, JSON.stringify(quotedDraft));
  assert.equal(quotedDraft.reparsedPlain, quoteDraft);
  assert.deepEqual(quotedDraft.reparsedRanges, quotedDraft.ranges);
  assert.equal(quotedDraft.reparsedBlocks.length, 1);
  assert.deepEqual(quotedDraft.reparsedBlocks[0]?.slice(1), [quoteStart, 7]);
  for (const text of [
    'a\nq\nb',
    'a\nq\n\nb',
    'q\nb',
    'a\nq\n',
    'a\nq',
    'q\n\nb',
    'a\nq\nr\nb',
    'a\r\nq\r\nb',
  ]) {
    const start = text.indexOf('q');
    for (const length of [1, text.includes('r') ? 3 : 1]) {
      const value = run('', {
        edit: { start: 0, delete: 0, text },
        blockAvailability: { start, length },
        toggleQuotes: [[start, length]],
      });
      assert.equal(value.available, true, JSON.stringify(value));
      assert.equal(value.reparsedPlain, text);
      assert.equal(run(value.markdown).markdown, value.markdown);
    }
  }
  for (const type of [0, 1, 3, 5, 6]) {
    const crossing = run('', {
      edit: { start: 0, delete: 0, text: 'before\nquote\nafter' },
      toggles: [{ type, start: 0, length: 18 }],
      blockAvailability: { start: 7, length: 5 },
      toggleQuotes: [[7, 5]],
    });
    assert.equal(crossing.available, true, JSON.stringify(crossing));
    assert.equal(crossing.reparsedPlain, 'before\nquote\nafter');
    for (let i = 0; i < crossing.plain.length; i++) {
      if ('\r\n'.includes(crossing.plain[i]!)) continue;
      assert.ok(
        crossing.reparsedRanges.some(
          (range) => range.type === type && range.start <= i && i < range.start + range.length,
        ),
      );
    }
  }
  const linkedQuote = run('[before\nquote\nafter](https://example.test)', {
    blockAvailability: { start: 7, length: 5 },
    toggleQuotes: [[7, 5]],
  });
  assert.equal(linkedQuote.available, true, JSON.stringify(linkedQuote));
  assert.equal(linkedQuote.reparsedPlain, 'before\nquote\nafter');
  const quotedCanonical = run('', { ...quoteOptions, toggleQuotes: [] }).markdown;
  const onlyQuote = { toggleQuotes: [[quoteStart, 7]] };
  assert.equal(run(quotedCanonical, { ...onlyQuote, undo: true }).markdown, quotedCanonical);
  assert.equal(
    run(quotedCanonical, { ...onlyQuote, undo: true, redo: true }).markdown,
    quotedDraft.markdown,
  );
  const removedQuote = run('', {
    ...quoteOptions,
    toggleQuotes: [
      [quoteStart, 7],
      [quoteStart, 7],
    ],
  });
  assert.equal(removedQuote.reparsedPlain, quoteDraft);
  assert.deepEqual(removedQuote.reparsedBlocks, []);
  assert.equal(run('', { ...quoteOptions, undo: true }).markdown, '');
  assert.equal(run('', { ...quoteOptions, undo: true, redo: true }).markdown, quotedDraft.markdown);
}
