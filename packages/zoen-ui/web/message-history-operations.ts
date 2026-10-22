// Whole operations borrowed from the sole Messages owner.
import { historySlice, type HistoryResult } from './history.ts';
import { object, message, boolean, integer, publicKey, bytes, string } from './boundary.ts';
import type { MessageOperations } from './message-operations.ts';
import type { Messages } from './messages.ts';
type MessageHistoryOperationsPort = Pick<
  MessageOperations,
  | 'closed'
  | 'historyResult'
  | 'connection'
  | 'historyRunning'
  | 'session'
  | 'historyTimer'
  | 'restoreHistory'
  | 'generation'
  | 'base'
  | 'aborted'
  | 'rpc'
  | 'enqueue'
  | 'core'
  | 'apply'
  | 'historyWaiters'
  | 'historyError'
  | 'scheduleHistory'
  | 'linkWork'
  | 'scheduleMaintenance'
  | 'notice'
  | 'publish'
  | 'linkRequest'
  | 'resumeOutgoingHistory'
  | 'transport'
>;

export function scheduleHistory(this: MessageHistoryOperationsPort, delay: number): void {
  if (
    this.closed ||
    this.historyResult ||
    this.connection.kind !== 'ready' ||
    this.historyRunning ||
    !this.session.account.link_secret ||
    !this.session.account.hpke_secret
  )
    return;
  clearTimeout(this.historyTimer);
  this.historyTimer = setTimeout(() => {
    this.historyTimer = undefined;
    void this.restoreHistory();
  }, delay);
}

export async function restoreHistory(this: MessageHistoryOperationsPort): Promise<void> {
  if (this.historyRunning || this.closed || this.connection.kind !== 'ready') return;
  this.historyRunning = true;
  const generation = this.generation;
  const valid = () =>
    !this.closed && generation === this.generation && this.connection.kind === 'ready';
  try {
    const result = await historySlice({
      base: this.base(),
      signal: this.aborted.signal,
      valid,
      rpc: (kind, value) => this.rpc(kind, value),
      core: async (operation) => {
        let work: unknown;
        await this.enqueue(
          async () => {
            let response: Record<string, unknown>;
            try {
              response = object(JSON.parse(await this.core.text(operation)));
            } catch (error) {
              throw Object.assign(new Error(message(error)), { soft: true });
            }
            await this.apply(JSON.stringify(response.update), generation);
            work = response.work;
          },
          () => (valid() ? null : 'A conexão mudou durante a restauração.'),
          false,
        );
        return work;
      },
    });
    if (result && valid()) {
      this.historyResult = result;
      for (const waiter of this.historyWaiters) {
        clearTimeout(waiter.timer);
        waiter.resolve(result);
      }
      this.historyWaiters.clear();
    }
  } catch (error) {
    this.historyError = message(error);
  } finally {
    this.historyRunning = false;
    this.scheduleHistory(1_000);
  }
}

export async function historyStatus(this: MessageHistoryOperationsPort): Promise<{
  initial_pending: boolean;
  manifest_received: boolean;
  chunks_received: number;
  chunks_total: number;
  receive_active: boolean;
}> {
  let status: ReturnType<Messages['historyStatus']> extends Promise<infer T> ? T : never;
  await this.enqueue(async () => {
    const value = object(JSON.parse(await this.core.text({ kind: 'history_status' })));
    status = {
      initial_pending: boolean(value.initial_pending),
      manifest_received: boolean(value.manifest_received),
      chunks_received: integer(value.chunks_received),
      chunks_total: integer(value.chunks_total),
      receive_active: this.historyRunning,
    };
  });
  return status!;
}

export function receiveHistory(
  this: MessageHistoryOperationsPort,
  timeoutMs: number,
): Promise<HistoryResult> {
  if (this.closed) return Promise.reject(new Error('Esta sessão está fechada.'));
  if (this.historyResult) return Promise.resolve(this.historyResult);
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 300_000)
    return Promise.reject(new Error('Tempo de restauração inválido.'));
  this.scheduleHistory();
  return new Promise((resolve, reject) => {
    const waiter = {
      resolve,
      reject,
      timer: setTimeout(() => {
        this.historyWaiters.delete(waiter);
        reject(new Error(this.historyError));
      }, timeoutMs),
    };
    this.historyWaiters.add(waiter);
  });
}

export async function linkWork(
  this: MessageHistoryOperationsPort,
  command: unknown,
  generation: number,
): Promise<Record<string, unknown>> {
  let response: Record<string, unknown>;
  try {
    response = object(
      JSON.parse(
        await this.core.text({
          kind: 'link_request',
          request: JSON.stringify(command),
          now: Date.now(),
        }),
      ),
    );
  } catch (error) {
    throw Object.assign(new Error(message(error)), { soft: true });
  }
  if (response.update !== null) await this.apply(JSON.stringify(response.update), generation);
  return object(response.result);
}

/** Retained ciphertext is saved before PUT, and the same bytes survive a cold retry. */
export async function resumeOutgoingHistory(
  this: MessageHistoryOperationsPort,
  generation: number,
): Promise<void> {
  if (this.closed || generation !== this.generation || this.connection.kind !== 'ready') return;
  try {
    let work = await this.linkWork({ op: 'history_send_resume' }, generation);
    while (work.kind === 'upload') {
      if (this.closed || generation !== this.generation || this.connection.kind !== 'ready') return;
      const transfer = publicKey(work.transfer),
        index = integer(work.index),
        sha = publicKey(work.sha256);
      const sealed = bytes(work.bytes);
      if (index > 899 || sealed.length < 16 || sealed.length > (1 << 20) + 16)
        throw new Error('Parte inválida do histórico.');
      const abort = new AbortController();
      const stopped = () => abort.abort(this.aborted.signal.reason);
      this.aborted.signal.addEventListener('abort', stopped, { once: true });
      const timer = setTimeout(
        () => abort.abort(new Error('O envio do histórico demorou demais.')),
        15_000,
      );
      let response: Response;
      try {
        response = await fetch(`${this.base()}/v1/transfer/${transfer}/${index}`, {
          method: 'PUT',
          headers: {
            'x-zoen-device': publicKey(work.device),
            'x-zoen-ts': string(work.timestamp),
            'x-zoen-sig': string(work.signature),
          },
          body: sealed,
          signal: abort.signal,
        }).catch((error) => {
          throw Object.assign(new Error(message(error)), { historyUploadRetry: true });
        });
      } finally {
        clearTimeout(timer);
        this.aborted.signal.removeEventListener('abort', stopped);
      }
      await response.body?.cancel();
      if (!response.ok) {
        const retry = response.status === 429 || response.status >= 500;
        if (!retry)
          await this.linkWork(
            {
              op: 'history_failed',
              transfer,
              message:
                'O servidor recusou o envio do histórico. Confira o dispositivo e tente novamente.',
            },
            generation,
          );
        throw Object.assign(
          new Error(
            retry
              ? 'O histórico aguarda conexão para terminar de enviar.'
              : 'O servidor recusou o envio do histórico. Confira o dispositivo e tente novamente.',
          ),
          { historyUploadRetry: retry, soft: true },
        );
      }
      if (this.closed || generation !== this.generation || this.connection.kind !== 'ready') return;
      work = await this.linkWork(
        { op: 'history_uploaded', transfer, index, sha256: sha },
        generation,
      );
      // Yield the canonical queue between chunks so sync/receipts stay responsive.
      if (work.kind === 'upload') {
        this.scheduleMaintenance(50, generation, true);
        return;
      }
    }
  } catch (error) {
    if (this.closed) return;
    if (
      !(error as { historyUploadRetry?: boolean; soft?: boolean }).historyUploadRetry &&
      !(error as { soft?: boolean }).soft
    )
      throw error;
    this.notice = message(error);
    if ((error as { historyUploadRetry?: boolean }).historyUploadRetry)
      this.scheduleMaintenance(5000, generation, true);
    this.publish();
  }
}

export async function linkDevice(
  this: MessageHistoryOperationsPort,
  code: string,
): Promise<{ device_id: string; check: string }> {
  let linked: { device_id: string; check: string } | undefined;
  await this.enqueue(
    async () => {
      let response: Record<string, unknown>;
      try {
        response = object(
          JSON.parse(await this.core.text({ kind: 'link_prepare', code, now: Date.now() })),
        );
      } catch (error) {
        throw Object.assign(new Error(message(error)), { soft: true });
      }
      const result = object(response.result);
      linked = { device_id: publicKey(result.device_id), check: string(result.check) };
      await this.apply(JSON.stringify(response.update), this.generation);
    },
    () => (this.connection.kind === 'ready' ? null : 'Aguarde a conexão para vincular o aparelho.'),
    false,
  );
  const end = Date.now() + 30_000;
  while (Date.now() < end && !this.closed) {
    const status = object(
      await this.linkRequest({ op: 'link_status', device_id: linked!.device_id }),
    );
    if (status.failure !== null) throw new Error(string(status.failure));
    if (status.delivered === true) return linked!;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    'O vínculo foi preparado e aguarda confirmação do servidor. Tente novamente com o mesmo código.',
  );
}

export async function sendHistory(
  this: MessageHistoryOperationsPort,
  device: string,
): Promise<number> {
  await this.linkRequest({ op: 'history_send', device_id: device });
  await this.enqueue(() => this.resumeOutgoingHistory(this.generation));
  const end = Date.now() + 60_000;
  while (Date.now() < end && !this.closed) {
    const work = object(await this.linkRequest({ op: 'history_send_resume' }));
    if (work.kind === 'complete') return integer(work.messages);
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error('O histórico foi preservado e continuará a ser enviado quando houver conexão.');
}

export async function loadOlder(
  this: MessageHistoryOperationsPort,
  space: string,
  timeoutMs: number,
): Promise<{ loaded: number; more: boolean; primary_offline: boolean; message: string | null }> {
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 300_000)
    throw new Error('Tempo de carregamento inválido.');
  const request = object(await this.linkRequest({ op: 'load_older', space_id: space }));
  if (request.request_id === null)
    return { loaded: 0, more: false, primary_offline: false, message: null };
  const id = string(request.request_id),
    end = Date.now() + timeoutMs;
  while (Date.now() < end && !this.closed) {
    const answer = object(
      await this.linkRequest({ op: 'older_status', space_id: space, request_id: id }),
    );
    if (typeof answer.failure === 'string') throw new Error(answer.failure);
    if (answer.pending === false)
      return {
        loaded: integer(answer.loaded),
        more: boolean(answer.more),
        primary_offline: false,
        message: null,
      };
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return {
    loaded: 0,
    more: true,
    primary_offline: true,
    message: 'Abra o Zoen no aparelho que vinculou este navegador para carregar mensagens antigas.',
  };
}

export async function rpc(
  this: MessageHistoryOperationsPort,
  kind: string,
  value: string,
): Promise<unknown> {
  if (this.closed) throw new Error('Esta sessão está fechada.');
  const generation = this.generation;
  const response = await this.transport.request(kind, value);
  if (this.closed || generation !== this.generation)
    throw new Error('A conexão mudou. Tente de novo.');
  return response;
}
