import type { PageBlockDto } from '../core/generated/PageBlockDto';
import { t } from '../i18n/index.ts';

// Legacy projections without complete metadata and future kinds remain closed.
export function canDuplicateBlock(block: PageBlockDto): boolean {
  if (block.kind === 'heading')
    return (
      typeof block.setext === 'boolean' &&
      Number.isInteger(block.level) &&
      block.level >= 1 &&
      block.level <= (block.setext ? 2 : 6)
    );
  if (block.kind === 'image') return typeof block.title === 'string';
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
