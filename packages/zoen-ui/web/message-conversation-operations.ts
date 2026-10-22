// Whole operations borrowed from the sole Messages owner.
import { type MessageCore } from './message-operations.ts';
import { plain, type Invite } from './message-model.ts';
import { message, object } from './boundary.ts';
import type { MessageOperations } from './message-operations.ts';
type MessageConversationOperationsPort = Pick<
  MessageOperations,
  | 'closed'
  | 'connection'
  | 'conversations'
  | 'generation'
  | 'enqueue'
  | 'notice'
  | 'core'
  | 'apply'
  | 'transport'
  | 'session'
  | 'rpc'
  | 'membershipRequest'
>;

export function act(
  this: MessageConversationOperationsPort,
  space: string,
  operation: Parameters<MessageCore['text']>[0],
  allowOffline: boolean,
): Promise<void> {
  if (this.closed)
    return Promise.reject(new Error('Recarregue o Zoen para abrir esta sessão de novo.'));
  if (!allowOffline && this.connection.kind !== 'ready')
    return Promise.reject(new Error('Sem conexão agora. Tente de novo em instantes.'));
  if (!this.conversations.find((conversation) => conversation.id === space)?.can_send) {
    return Promise.reject(new Error('Esta conversa ainda está sendo preparada no seu celular.'));
  }
  const generation = this.generation;
  return this.enqueue(
    async () => {
      if (!this.conversations.find((conversation) => conversation.id === space)?.can_send)
        throw Object.assign(new Error('Esta conversa ainda está sendo preparada no seu celular.'), {
          soft: true,
        });
      this.notice = null;
      let update: string;
      try {
        update = await this.core.text(operation);
      } catch (error) {
        throw Object.assign(new Error(plain(message(error))), { soft: true });
      }
      await this.apply(update, generation);
    },
    () =>
      !allowOffline && (generation !== this.generation || this.connection.kind !== 'ready')
        ? 'A conexão mudou. Tente de novo.'
        : null,
    false,
  );
}

export function markRead(this: MessageConversationOperationsPort, space: string): Promise<void> {
  if (this.closed || this.connection.kind !== 'ready') return Promise.resolve();
  const conversation = this.conversations.find((c) => c.id === space);
  if (!conversation?.unread) return Promise.resolve();
  const generation = this.generation;
  return this.enqueue(
    async () => {
      let update: string;
      try {
        update = await this.core.text({ kind: 'mark_read', space });
      } catch {
        throw Object.assign(new Error('Não deu para marcar a conversa como lida. Tente de novo.'), {
          soft: true,
        });
      }
      await this.apply(update, generation);
    },
    () =>
      generation !== this.generation || this.connection.kind !== 'ready'
        ? 'A conexão mudou. Tente de novo.'
        : null,
    false,
  );
}

export async function typing(
  this: MessageConversationOperationsPort,
  space: string,
  typing: boolean,
): Promise<void> {
  if (this.closed || this.connection.kind !== 'ready') return;
  const generation = this.generation;
  const frame = await this.core.bytes({ kind: 'typing_frame', space, typing });
  if (
    frame.length &&
    !this.closed &&
    this.connection.kind === 'ready' &&
    generation === this.generation
  )
    this.transport.send(frame);
}

export async function unlinkDevice(
  this: MessageConversationOperationsPort,
  device: string,
): Promise<void> {
  if (this.closed) throw new Error('Esta sessão está fechada.');
  if (device === this.session.account.device)
    throw new Error('Use Sair para fechar este navegador.');
  const response = object(await this.rpc('unlink', device));
  if (response.kind !== 'done')
    throw new Error('O servidor não confirmou a remoção deste aparelho.');
  await this.membershipRequest({ op: 'refresh_devices' });
}

export function join(
  this: MessageConversationOperationsPort,
  { invite, code }: { invite: Invite; code: string },
): Promise<void> {
  if (this.closed) return Promise.reject(new Error('The browser session is closed.'));
  if (this.connection.kind !== 'ready')
    return Promise.reject(new Error('The relay is reconnecting. Try again when connected.'));
  const generation = this.generation;
  return this.enqueue(
    async () => {
      this.notice = null;
      const update = await this.core.text({
        kind: 'queue_join',
        space: invite.space,
        role: invite.role,
        code,
        now: Date.now(),
      });
      await this.apply(update, generation);
    },
    () =>
      generation !== this.generation || this.connection.kind !== 'ready'
        ? 'The connection changed. Try again when connected.'
        : null,
  );
}
