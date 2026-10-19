import test from 'node:test';
import assert from 'node:assert/strict';
import type { ClientStore } from '../core/store';
import type { Dialogs } from '../components/dialog-types';
import { callMiniapp } from './actions.ts';

test('closing a frame during irreversible confirmation cannot replay the mutation', async () => {
  const commands: unknown[] = [];
  let active = true;
  const store = {
    getSnapshot: () => ({ mode: 'native', snapshot: { me: { id: 'owner' } } }),
    request: async (command: unknown) => {
      commands.push(command);
      return { status: 'NeedsConfirmation', message: 'Confirmar?' };
    },
  } as unknown as ClientStore;
  const dialogs = {
    confirm: async () => {
      active = false;
      return true;
    },
  } as unknown as Dialogs;
  await assert.rejects(
    callMiniapp(store, dialogs, 'it_one', 'delete', '{"cost":9223372036854775807}', () => {
      if (!active) throw new Error('closed');
    }),
    /closed/,
  );
  assert.equal(commands.length, 1);
});
