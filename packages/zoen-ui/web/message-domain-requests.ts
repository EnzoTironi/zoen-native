// Whole operations borrowed from the sole Messages owner.
import { object, message, array, string } from './boundary.ts';
import { parseChunk } from './message-file-model.ts';
import type { MessageOperations } from './message-operations.ts';
type MessageDomainRequestsPort = Pick<
  MessageOperations,
  | 'enqueue'
  | 'core'
  | 'generation'
  | 'apply'
  | 'connection'
  | 'resumeDocumentUploads'
  | 'notice'
  | 'scheduleMaintenance'
  | 'publish'
  | 'openFile'
  | 'closed'
  | 'upload'
>;

export async function domainRequest(
  this: MessageDomainRequestsPort,
  command: unknown,
  domain:
    | 'document_request'
    | 'agent_request'
    | 'app_request'
    | 'profile_request'
    | 'membership_request'
    | 'link_request',
): Promise<unknown> {
  let result: unknown;
  await this.enqueue(
    async () => {
      let response: Record<string, unknown>;
      const request = JSON.stringify(command);
      const operation = () => ({ kind: domain, request, now: Date.now() });
      try {
        response = object(JSON.parse(await this.core.text(operation())));
      } catch (error) {
        throw Object.assign(new Error(message(error)), { soft: true });
      }
      const generation = this.generation;
      if (response.update !== null) await this.apply(JSON.stringify(response.update), generation);
      // The saved snapshot now owns every outgoing byte. An interrupted PUT cannot
      // publish an incomplete document or erase the person's local draft.
      if (array(response.uploads).length && this.connection.kind === 'ready') {
        try {
          await this.resumeDocumentUploads(generation);
        } catch (error) {
          if (!(error as { documentUploadRetry?: boolean }).documentUploadRetry) throw error;
          this.notice =
            'O documento foi salvo neste navegador e aguarda conexão para terminar de enviar.';
          this.scheduleMaintenance(5000, generation, true);
          this.publish();
        }
      }
      const downloads = array(response.downloads).map(parseChunk);
      if (downloads.length) {
        let complete = true;
        for (const reference of downloads) {
          let bytes: Uint8Array;
          try {
            if (reference.bytes > 32 << 20) throw new Error('Conteúdo grande demais.');
            const blob = await this.openFile(
              {
                name: 'page',
                path: '',
                mime: 'application/vnd.zoen.page',
                bytes: reference.bytes,
                sha256: reference.sha256,
                chunks: [reference],
                thumb: null,
                voice: null,
              },
              false,
              true,
            );
            bytes = new Uint8Array(await blob.arrayBuffer());
          } catch {
            complete = false;
            continue;
          }
          const update = await this.core.text({
            kind: 'document_cache_blob',
            reference: JSON.stringify(reference),
            bytes,
          });
          await this.apply(update, generation);
        }
        if (complete) {
          // Only reads can request missing payloads. Mutations refuse an incomplete
          // page before changing it, so fetching never replays a non-idempotent write.
          try {
            response = object(JSON.parse(await this.core.text(operation())));
          } catch (error) {
            throw Object.assign(new Error(message(error)), { soft: true });
          }
          if (response.update !== null)
            await this.apply(JSON.stringify(response.update), generation);
        } else if (response.result === null) {
          throw Object.assign(
            new Error('O documento ainda está chegando. Tente de novo quando estiver conectado.'),
            { soft: true },
          );
        }
      }
      result = response.result;
    },
    () => (this.closed ? 'Esta sessão está fechada.' : null),
    false,
  );
  return result;
}

export async function resumeDocumentUploads(
  this: MessageDomainRequestsPort,
  generation: number,
): Promise<void> {
  const blobs = array(JSON.parse(await this.core.text({ kind: 'document_uploads' }))).map(string);
  for (const blob of blobs) {
    if (this.closed || this.connection.kind !== 'ready' || generation !== this.generation) return;
    const sealed = await this.core.bytes({ kind: 'local_chunk', blob });
    if (!sealed.length) throw new Error('O conteúdo local do documento está indisponível.');
    try {
      await this.upload(blob, sealed);
    } catch (error) {
      throw Object.assign(
        error instanceof Error ? error : new Error('Sem conexão para enviar o documento.'),
        { documentUploadRetry: true },
      );
    }
    const update = await this.core.text({
      kind: 'document_uploaded',
      blobs: JSON.stringify([blob]),
    });
    await this.apply(update, generation);
  }
}

export async function integrityRequest(
  this: MessageDomainRequestsPort,
  command: unknown,
): Promise<unknown> {
  let result: unknown;
  // This is a local, nonpersistent read. Reconnecting the same account does
  // not invalidate its log; stopping this Messages instance does.
  await this.enqueue(
    async () => {
      try {
        const response = await this.core.text({
          kind: 'integrity_request',
          request: JSON.stringify(command),
        });
        if (this.closed) throw new Error('Esta sessão foi encerrada. Abra o relatório novamente.');
        result = JSON.parse(response);
      } catch (error) {
        throw Object.assign(new Error(message(error)), { soft: true });
      }
    },
    undefined,
    false,
  );
  return result;
}
