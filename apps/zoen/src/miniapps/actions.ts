import type { ClientStore } from '../core/store';
import type { Dialogs } from '../components/dialog-types';

/** Confirmation belongs to the host and replays the same immutable tool arguments. */
export async function callMiniapp(
  store: ClientStore,
  dialogs: Dialogs,
  item: string,
  tool: string,
  args: string,
  assertActive?: () => void,
) {
  const initial = store.getSnapshot();
  const scope = `${initial.mode}:${initial.snapshot?.me?.id ?? ''}`;
  const assertScope = () => {
    assertActive?.();
    const current = store.getSnapshot();
    if (`${current.mode}:${current.snapshot?.me?.id ?? ''}` !== scope)
      throw new Error('A conta mudou. Abra o miniapp novamente.');
  };
  assertScope();
  const command = {
    op: 'app_call_tool' as const,
    item_id: item,
    tool,
    args_json: args,
    confirmed: false,
  };
  let result = await store.request(command);
  assertScope();
  if (result.status === 'NeedsConfirmation') {
    const accepted = await dialogs.confirm({
      title: result.confirm_title ?? 'Confirmar ação do miniapp?',
      message: result.confirm_detail ?? result.message,
      confirmLabel: 'Confirmar ação',
    });
    if (!accepted) return null;
    assertScope();
    result = await store.request({ ...command, confirmed: true });
  }
  assertScope();
  if (result.status === 'NeedsConfirmation')
    throw new Error('A permissão mudou. Revise a ação novamente.');
  if (result.status === 'Denied')
    throw new Error(result.message || 'O miniapp não tem permissão para esta ação.');
  const payload: unknown = JSON.parse(result.result_json);
  if (payload && typeof payload === 'object' && 'isError' in payload && payload.isError === true)
    throw new Error(result.message || 'O miniapp não conseguiu concluir esta ação.');
  return result;
}
