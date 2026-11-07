import type { DecideOutcome } from '../core/generated/DecideOutcome';
import type { useRequestDecisionLease } from './use-request-decision-lease';

export type DecisionTicket = NonNullable<ReturnType<typeof useRequestDecisionLease>['ticket']>;
export type DecisionAttempt = Readonly<{ ticket: DecisionTicket }>;

/** Only controls presentation concurrency. Core owns signed decisions and their receipts. */
export async function submitRequestDecision({
  ticket,
  flight,
  confirm,
  submit,
  pending,
  accepted,
  failed,
}: {
  ticket: DecisionTicket | null;
  flight: { current: DecisionAttempt | null };
  confirm?: () => Promise<boolean>;
  submit(): Promise<DecideOutcome>;
  pending(ticket: DecisionTicket | null): void;
  accepted(outcome: DecideOutcome): void;
  failed(reason: unknown): void;
}) {
  if (
    !ticket ||
    !ticket.lease.accepts(ticket) ||
    (flight.current && flight.current.ticket.lease.continues(flight.current.ticket))
  )
    return;
  const attempt = { ticket };
  flight.current = attempt;
  try {
    if (confirm && !(await confirm())) return;
    if (!ticket.lease.accepts(ticket)) return;
    pending(ticket);
    const outcome = await submit();
    if (ticket.lease.continues(ticket)) accepted(outcome);
  } catch (reason) {
    if (ticket.lease.continues(ticket)) failed(reason);
  } finally {
    if (flight.current === attempt) {
      flight.current = null;
      pending(null);
    }
  }
}
