import type { ReplySelection } from './generated/ReplySelection';
import type { TimelineEntry } from './types';
import { messageText } from './presentation.ts';

/** Half-open UTF-16 offsets in the exact canonical source, never rendered offsets. */
export function selectedMessageQuote(
  entry: TimelineEntry,
  text: string,
  sourceRangesJSON: string | undefined,
): ReplySelection | null {
  if (!entry.text_revision || entry.deleted || !text.trim() || text.length > 4096) return null;
  let value: unknown;
  try {
    value = JSON.parse(sourceRangesJSON ?? '');
  } catch {
    return null;
  }
  if (!Array.isArray(value) || !value.length || value.length > 128) return null;
  const source = messageText(entry);
  const spans: ReplySelection['spans'] = [];
  let previous = 0;
  let extracted = '';
  const boundary = (index: number) =>
    index === 0 ||
    index === source.length ||
    !(
      source.charCodeAt(index - 1) >= 0xd800 &&
      source.charCodeAt(index - 1) <= 0xdbff &&
      source.charCodeAt(index) >= 0xdc00 &&
      source.charCodeAt(index) <= 0xdfff
    );
  for (const span of value) {
    if (!span || typeof span !== 'object') return null;
    const { start_utf16: start, end_utf16: end } = span;
    if (
      !Number.isSafeInteger(start) ||
      !Number.isSafeInteger(end) ||
      start < previous ||
      end <= start ||
      end > source.length ||
      !boundary(start) ||
      !boundary(end)
    )
      return null;
    spans.push({ start_utf16: start, end_utf16: end });
    extracted += source.slice(start, end);
    previous = end;
  }
  if (extracted !== text) return null;
  return { source_id: entry.id, revision: entry.text_revision, spans, text };
}
