import { useRef, useState } from 'react';
import { useClient } from '../core/provider';
import { accountScope, accountScopeKey } from '../core/account-scope';
import type { InboxFolders } from '../core/generated/InboxFolders';
import type { InboxFoldersInput } from '../core/generated/InboxFoldersInput';
import { t } from '../i18n/index.ts';
import { cloneFolders } from './folders';

type Draft = { owner: string; revision: string; configuration: InboxFolders };
type Intent = { owner: string; input: Extract<InboxFoldersInput, { kind: 'put' }> };
export function useFolderEditor() {
  const client = useClient(),
    { store, snapshot } = client;
  const scope = accountScope(store),
    owner = accountScopeKey(scope);
  const current = snapshot?.inbox_folders;
  const [draft, setDraft] = useState<Draft | null>(null);
  const [intent, setIntent] = useState<Intent | null>(null);
  const [pendingOwner, setPendingOwner] = useState('');
  const busy = useRef(false);
  const intentRef = useRef<Intent | null>(null);
  const [error, setError] = useState<{ owner: string; message: string } | null>(null);
  const base =
    draft?.owner === owner
      ? draft
      : current
        ? { owner, revision: current.revision, configuration: current.configuration }
        : null;
  const activeIntent = intent?.owner === owner ? intent : null;
  const pending = pendingOwner === owner && !!owner;
  const blocked = !scope || !base || pending || !!activeIntent;
  const change = (configuration: InboxFolders) => {
    if (!blocked && !busy.current && intentRef.current?.owner !== owner && base)
      setDraft({ ...base, configuration });
  };
  const discard = () => {
    if (busy.current) return;
    setDraft(null);
    setIntent(null);
    intentRef.current = null;
    setError(null);
    void store.refresh().catch(() => {});
  };
  const save = async () => {
    if (busy.current || !scope || !base || accountScopeKey(accountScope(store)) !== owner) return;
    busy.current = true;
    setPendingOwner(owner);
    setError(null);
    try {
      let request = intentRef.current?.owner === owner ? intentRef.current : null;
      if (!request) {
        const id = await store.driver.request({ op: 'new_mutation_id' });
        if (accountScopeKey(accountScope(store)) !== owner) return;
        request = {
          owner,
          input: {
            kind: 'put',
            scope: { expected_identity_id: scope.identity, expected_device_id: scope.device },
            operation_id: id,
            observed_revision: base.revision,
            configuration: cloneFolders(base.configuration),
          },
        };
        intentRef.current = request;
        setIntent(request);
      }
      if (accountScopeKey(accountScope(store)) !== owner) return;
      await store.request({ op: 'inbox_folders', input: request.input });
      if (accountScopeKey(accountScope(store)) === owner) {
        setDraft(null);
        setIntent(null);
        intentRef.current = null;
      }
    } catch (failure) {
      if (accountScopeKey(accountScope(store)) === owner)
        setError({
          owner,
          message: failure instanceof Error ? failure.message : t('inboxActions.changed'),
        });
    } finally {
      busy.current = false;
      setPendingOwner('');
    }
  };
  const newId = async () => {
    if (blocked || busy.current) return null;
    busy.current = true;
    setPendingOwner(owner);
    try {
      const id = await store.driver.request({ op: 'new_mutation_id' });
      return accountScopeKey(accountScope(store)) === owner ? id : null;
    } catch {
      if (accountScopeKey(accountScope(store)) === owner)
        setError({ owner, message: t('inboxActions.changed') });
      return null;
    } finally {
      busy.current = false;
      setPendingOwner('');
    }
  };
  return {
    owner,
    base,
    blocked,
    pending,
    change,
    save,
    discard,
    newId,
    chats: snapshot?.conversations ?? [],
    error: error?.owner === owner ? error.message : null,
    canSave: !!activeIntent || draft?.owner === owner,
  };
}
