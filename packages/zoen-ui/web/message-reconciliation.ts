import { CursorReconciliation } from './cursor-reconciliation.ts';
import { object } from './boundary.ts';
import type { MessageOperations } from './message-operations.ts';

type ReconciliationOwner = Pick<
  MessageOperations,
  | 'closed'
  | 'generation'
  | 'connection'
  | 'transport'
  | 'enqueue'
  | 'core'
  | 'apply'
  | 'cursorRepair'
>;

export function createReconciliation(owner: ReconciliationOwner): CursorReconciliation {
  const allowed = (generation: number) =>
    !owner.closed && owner.generation === generation && owner.connection.kind === 'ready';
  return new CursorReconciliation({
    allowed,
    reconnect: () => owner.transport.reconcileTimeout(),
    tick: (clock, generation) =>
      owner.enqueue(async () => {
        if (!owner.cursorRepair.check(generation)) return;
        // The queued callback may have waited behind another write. Check the
        // actual clock again; never borrow extra time from its admission timer.
        const now = owner.cursorRepair.now();
        if (now < clock) throw new Error('The reconciliation clock moved backwards.');
        const result = object(JSON.parse(await owner.core.text({ kind: 'reconcile', now })));
        if (!owner.cursorRepair.check(generation)) return;
        if (result.kind === 'expired') {
          if ('update' in result) throw new Error('Expired reconciliation carried an update.');
          owner.cursorRepair.expire(generation);
          return;
        }
        if (result.kind !== 'update' || !('update' in result))
          throw new Error('Invalid reconciliation result.');
        await owner.apply(JSON.stringify(result.update), generation);
      }),
  });
}
