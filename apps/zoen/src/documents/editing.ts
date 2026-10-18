import { locale, t } from '../i18n/index.ts';
import { formatMoneyInput } from '../i18n/money.ts';
import type { PageBlockDto } from '../core/generated/PageBlockDto';
import type { TextSpanDto } from '../core/generated/TextSpanDto';

/** Block operations only describe the state the editor actually observed. */
export function moveBlock(order: readonly string[], id: string, direction: -1 | 1): string[] {
  const index = order.indexOf(id);
  if (index < 0) throw new Error(t('documents.thisBlockIsNoLongerOnThePage'));
  const target = index + direction;
  if (target < 0 || target >= order.length) return [...order];
  const result = [...order];
  [result[index], result[target]] = [result[target]!, result[index]!];
  return result;
}

export function replaceBlock(
  blocks: readonly PageBlockDto[],
  originalId: string | null,
  replacement: readonly PageBlockDto[],
): { order: string[]; changed: PageBlockDto[] } {
  const changed = replacement.map((block, index) => ({
    ...block,
    id: index === 0 && originalId ? originalId : block.id,
  }));
  if (!originalId)
    return {
      order: [...blocks.map((block) => block.id), ...changed.map((block) => block.id)],
      changed,
    };
  if (!blocks.some((block) => block.id === originalId))
    throw new Error(t('documents.thisBlockIsNoLongerOnThePage'));
  return {
    order: blocks.flatMap((block) =>
      block.id === originalId ? changed.map((item) => item.id) : [block.id],
    ),
    changed,
  };
}

/** Markdown cannot encode future/custom marks. Retain their metadata in a text-only edit. */
export function needsLosslessTextEditor(block: PageBlockDto): boolean {
  return block.spans.some((span) => !['b', 'i', 's', 'c', 'a', 'hb'].includes(span.key));
}

/**
 * Treat one input event as one replacement, in the core's UTF-16 coordinates.
 * Existing ranges move with untouched text; inserted text inherits a mark only
 * when inserted inside it. Surrogate pairs are never split at an edit boundary.
 */
export function retargetSpans(
  before: string,
  after: string,
  spans: readonly TextSpanDto[],
): TextSpanDto[] {
  if (before === after) return spans.map((span) => ({ ...span }));
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  if (start > 0 && isHighSurrogate(before.charCodeAt(start - 1))) start--;
  let oldEnd = before.length;
  let newEnd = after.length;
  while (oldEnd > start && newEnd > start && before[oldEnd - 1] === after[newEnd - 1]) {
    oldEnd--;
    newEnd--;
  }
  if (oldEnd < before.length && isLowSurrogate(before.charCodeAt(oldEnd))) {
    oldEnd++;
    newEnd++;
  }
  const delta = newEnd - oldEnd;
  return spans.flatMap((span) => {
    const nextStart =
      span.start < start ? span.start : span.start >= oldEnd ? span.start + delta : start;
    const nextEnd = span.end <= start ? span.end : span.end >= oldEnd ? span.end + delta : newEnd;
    return nextStart < nextEnd ? [{ ...span, start: nextStart, end: nextEnd }] : [];
  });
}

function isHighSurrogate(value: number): boolean {
  return value >= 0xd800 && value <= 0xdbff;
}
function isLowSurrogate(value: number): boolean {
  return value >= 0xdc00 && value <= 0xdfff;
}

/** Financial inputs stay decimal strings all the way to Rust's signed i64. */
export function planAmountInput(cents: string): string {
  return formatMoneyInput(cents, { locale, minorUnitDigits: 2 });
}

export function markdownFilename(title: string): string {
  const safe = title
    .normalize('NFC')
    .replace(/[\\/:*?"<>|\p{Cc}]/gu, '-')
    .replace(/^\.+|[ .]+$/g, '')
    .trim()
    .slice(0, 96);
  return `${safe || t('documents.defaultMarkdownBasename')}.md`;
}

export function validateMarkdown(markdown: string): string {
  if (new TextEncoder().encode(markdown).length > 768 * 1024)
    throw new Error(t('documents.chooseAMarkdownPageUpTo768KB'));
  return markdown;
}
