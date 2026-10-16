import { array, boolean, integer, object, string } from './boundary.ts';
import type { PageDto, PageBlockDto } from './live-page.tsx';
function block(input: unknown): PageBlockDto {
  const value = object(input);
  const text = string(value.text);
  return { id: string(value.id), kind: string(value.kind), level: integer(value.level), indent: integer(value.indent), number: integer(value.number), checked: boolean(value.checked), lang: string(value.lang), url: string(value.url), alt: string(value.alt), text,
    spans: array(value.spans).map((input) => { const span = object(input); const start = integer(span.start); const end = integer(span.end); if (start < 0 || end < start || end > text.length) throw Error('Invalid page formatting range.'); return { start, end, key: string(span.key), value: string(span.value) }; }) };
}
/** Worker results remain validated data, never DOM or executable markup. */
export function parsePage(input: unknown): PageDto {
  const value = object(input);
  const blocks = array(value.blocks).map(block);
  if (new Set(blocks.map((b) => b.id)).size !== blocks.length) throw Error('Duplicate page block.');
  return { item_id: string(value.item_id), space_id: string(value.space_id), title: string(value.title), path: string(value.path), version: integer(value.version), ready: boolean(value.ready), unsaved: boolean(value.unsaved), pending_sync: boolean(value.pending_sync), save_error: value.save_error === null ? null : string(value.save_error), can_edit: boolean(value.can_edit), content_hash: string(value.content_hash), edit_context: string(value.edit_context), blocks };
}
