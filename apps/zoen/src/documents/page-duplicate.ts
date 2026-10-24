import type { PageBlockDto } from '../core/generated/PageBlockDto';
import { t } from '../i18n/index.ts';

// Image title and heading setext syntax are not projected by PageBlockDto.
// Do not silently invent defaults when duplicating those kinds or future ones.
export function canDuplicateBlock(block: PageBlockDto): boolean {
  return ['paragraph', 'bullet', 'numbered', 'task', 'quote', 'code', 'raw', 'divider'].includes(
    block.kind,
  );
}

/** Capture one observed edit before awaiting Core's opaque mutation/block identifier. */
export function duplicateBlockChange(order: readonly string[], block: PageBlockDto) {
  const before = [...order];
  const index = before.indexOf(block.id);
  if (index < 0 || !canDuplicateBlock(block))
    throw new Error(t('documents.thisBlockIsNoLongerOnThePage'));
  const original = { ...block, spans: block.spans.map((span) => ({ ...span })) };
  return (id: string): { order: string[]; changed: PageBlockDto[] } => {
    if (!/^[A-Za-z0-9_-]{1,128}$/.test(id) || before.includes(id))
      throw new Error(t('documents.theDraftCouldNotBeUpdated'));
    return {
      order: [...before.slice(0, index + 1), id, ...before.slice(index + 1)],
      changed: [{ ...original, id, spans: original.spans.map((span) => ({ ...span })) }],
    };
  };
}
