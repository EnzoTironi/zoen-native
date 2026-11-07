import { useEffect, useRef, useState } from 'react';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import type { InviteDto } from '../core/generated/InviteDto';
import { useMemberActions } from './use-member-actions';
import { createSpaceInvite, discloseSpaceInvite, type InviteFlight } from './space-invite-actions';

export function useSpaceInvite(space: string) {
  const client = useClient(),
    { store } = client;
  const member = useMemberActions(space);
  const owner = accountScope(store, client);
  const scope = accountScopeKey(owner);
  const { session, state } = member;
  const context = state.context;
  const ticket = session.ticket(state);
  const review =
    ticket && context ? `${ticket.scope}:${ticket.generation}:${context.revision}` : '';
  const [opening, setOpening] = useState<{ review: string; key: object } | null>(null);
  const active = useRef<object | null>(null);
  const flight = useRef<InviteFlight | null>(null);
  const [result, setResult] = useState<{ key: object; value: InviteDto } | null>(null);
  const [busy, setBusy] = useState<object | null>(null);
  const [error, setError] = useState<object | null>(null);
  const [ttl, setTtl] = useState(604800);
  const [uses, setUses] = useState(50);
  useEffect(() => {
    if (
      member.available &&
      owner &&
      scope === accountScopeKey(accountScope(store)) &&
      state.scope &&
      !state.selected
    )
      session.open(owner.identity);
  }, [member.available, owner, scope, session, state.scope, state.selected, store]);
  const permitted =
    !!context &&
    context.member_id === context.identity_id &&
    context.state === 'Active' &&
    context.can_manage_members &&
    context.delivery_ready;
  const open = opening !== null && opening.review === review && permitted;
  const current = () =>
    !!opening &&
    active.current === opening.key &&
    opening.review === review &&
    permitted &&
    session.canOpenProfile(ticket) &&
    session.getSnapshot().context?.revision === context?.revision;
  const close = () => {
    active.current = null;
    setOpening(null);
  };
  return {
    open,
    permitted,
    available: member.available,
    ready: state.phase === 'ready',
    readFailed: state.phase === 'failed',
    loading: state.phase === 'loading',
    title: context?.space_title ?? '',
    ttl,
    uses,
    setTtl,
    setUses,
    close,
    busy: !!opening && busy === opening.key,
    error: !!opening && error === opening.key,
    result: open && result?.key === opening.key ? result.value : null,
    show: () => {
      if (!permitted || !review || scope !== accountScopeKey(accountScope(store))) return;
      const key = {};
      active.current = key;
      setOpening({ review, key });
      setResult(null);
      setError(null);
      setTtl(604800);
      setUses(50);
    },
    retry: () => session.refresh(session.ticket()),
    create: () => {
      if (!opening || !context) return;
      void createSpaceInvite({
        store,
        context,
        ttl,
        uses,
        key: opening.key,
        flight,
        current,
        pending: (value) => {
          setBusy(value ? opening.key : null);
          if (value) setError(null);
        },
        created: (value) => setResult({ key: opening.key, value }),
        failed: () => setError(opening.key),
      });
    },
    disclose: (action: (link: string) => Promise<unknown>, completed?: () => void) => {
      if (!current() || !opening || !context || result?.key !== opening.key) return;
      void discloseSpaceInvite({
        store,
        context,
        key: opening.key,
        flight,
        current,
        pending: (value) => {
          setBusy(value ? opening.key : null);
          if (value) setError(null);
        },
        disclose: async () => {
          await action(result.value.link);
          if (current()) completed?.();
        },
        failed: () => setError(opening.key),
      });
    },
  };
}
