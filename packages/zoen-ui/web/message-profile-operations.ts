// Whole operations borrowed from the sole Messages owner.
import { object, message, string, boolean } from './boundary.ts';
import { plain } from './message-model.ts';
import type { MessageOperations } from './message-operations.ts';
type MessageProfileOperationsPort = Pick<
  MessageOperations,
  | 'generation'
  | 'enqueue'
  | 'core'
  | 'apply'
  | 'connection'
  | 'resumeDocumentUploads'
  | 'notice'
  | 'scheduleMaintenance'
  | 'publish'
  | 'closed'
  | 'base'
  | 'aborted'
  | 'handleChanges'
  | 'rpc'
  | 'session'
>;

export async function saveProfile(
  this: MessageProfileOperationsPort,
  request: { name: string; bio: string; photo: 'keep' | 'remove' | 'set'; mime?: string },
  bytes: Uint8Array,
): Promise<void> {
  if (bytes.length > 5 * 1024 * 1024) throw new Error('A foto precisa ter até 5 MB.');
  const generation = this.generation;
  await this.enqueue(
    async () => {
      let response: Record<string, unknown>;
      try {
        response = object(
          JSON.parse(
            await this.core.text({
              kind: 'profile_save',
              request: JSON.stringify(request),
              bytes,
              now: Date.now(),
            }),
          ),
        );
      } catch (error) {
        throw Object.assign(new Error(plain(message(error))), { soft: true });
      }
      await this.apply(JSON.stringify(response.update), generation);
      if (this.connection.kind === 'ready') {
        try {
          await this.resumeDocumentUploads(generation);
        } catch (error) {
          if (!(error as { documentUploadRetry?: boolean }).documentUploadRetry) throw error;
          this.notice = 'Seu perfil foi salvo e aguarda conexão para enviar a foto.';
          this.scheduleMaintenance(5000, generation, true);
          this.publish();
        }
      }
    },
    () => (this.closed || generation !== this.generation ? 'Esta sessão está fechada.' : null),
    false,
  );
}

export async function profilePhoto(
  this: MessageProfileOperationsPort,
  identity: string,
  expected: string,
): Promise<{ bytes: Uint8Array; mime: string; sha256: string }> {
  if (!/^[a-f0-9]{64}$/.test(identity) || !/^[a-f0-9]{64}$/.test(expected))
    throw new Error('Foto inválida.');
  let result: { bytes: Uint8Array; mime: string; sha256: string } | undefined;
  const generation = this.generation;
  await this.enqueue(
    async () => {
      const read = async () => {
        const framed = await this.core
          .bytes({ kind: 'profile_photo_framed', identity, expected })
          .catch((error) => {
            throw Object.assign(new Error(plain(message(error))), { soft: true });
          });
        if (framed.length < 4 || framed.length > 5 * 1024 * 1024 + 4100)
          throw new Error('Foto inválida.');
        const length = new DataView(framed.buffer, framed.byteOffset, framed.byteLength).getUint32(
          0,
          true,
        );
        if (length > 4096 || length > framed.length - 4) throw new Error('Foto inválida.');
        const header = object(JSON.parse(new TextDecoder().decode(framed.subarray(4, 4 + length))));
        if (
          header.identity_id !== identity ||
          header.sha256 !== expected ||
          !['image/jpeg', 'image/png', 'image/heic', 'image/webp'].includes(string(header.mime))
        )
          throw new Error('Esta foto não corresponde ao perfil.');
        return { header, bytes: framed.slice(4 + length) };
      };
      let photo = await read();
      if (!boolean(photo.header.cached)) {
        const blob = string(photo.header.blob);
        if (!/^[a-f0-9]{64}$/.test(blob)) throw new Error('Foto inválida.');
        const response = await fetch(`${this.base()}/v1/blobs/${blob}`, {
          signal: this.aborted.signal,
        });
        if (!response.ok || !response.body) {
          await response.body?.cancel();
          throw Object.assign(new Error('A foto ainda está chegando. Tente novamente.'), {
            soft: true,
          });
        }
        const reader = response.body.getReader();
        const chunks: Uint8Array[] = [];
        let size = 0;
        try {
          for (;;) {
            const { done, value } = await reader.read();
            if (done) break;
            if (value.length > 5 * 1024 * 1024 + 40 - size) throw new Error('Foto grande demais.');
            size += value.length;
            chunks.push(value);
          }
        } finally {
          await reader.cancel().catch(() => {});
        }
        if (this.closed || generation !== this.generation)
          throw new Error('Esta sessão está fechada.');
        const sealed = new Uint8Array(size);
        let offset = 0;
        for (const chunk of chunks) {
          sealed.set(chunk, offset);
          offset += chunk.length;
        }
        const update = await this.core.text({
          kind: 'profile_photo_received',
          identity,
          expected,
          sealed,
        });
        await this.apply(update, generation);
        photo = await read();
      }
      if (!boolean(photo.header.cached) || !photo.bytes.length)
        throw Object.assign(new Error('A foto ainda não está disponível.'), { soft: true });
      result = { bytes: photo.bytes, mime: string(photo.header.mime), sha256: expected };
    },
    () => (this.closed || generation !== this.generation ? 'Esta sessão está fechada.' : null),
    false,
  );
  if (!result) throw new Error('A foto ainda não está disponível.');
  return result;
}

export function updateHandle(this: MessageProfileOperationsPort, handle: string): Promise<string> {
  const generation = this.generation;
  const task = this.handleChanges.then(async () => {
    const active = () =>
      !this.closed && this.connection.kind === 'ready' && generation === this.generation;
    if (!active()) throw new Error('Conecte ao servidor para mudar seu nome de usuário.');
    const profile = object(JSON.parse(await this.core.text({ kind: 'handle_profile', handle })));
    const normalized = string(profile.handle);
    let response: Record<string, unknown>;
    try {
      response = object(await this.rpc('register', JSON.stringify(profile)));
    } catch (error) {
      if (/handle_taken/.test(message(error)))
        throw new Error('Este nome de usuário já está em uso.');
      throw error;
    }
    if (response.kind !== 'registered')
      throw new Error('O servidor não confirmou seu nome de usuário.');
    await this.enqueue(
      async () => {
        const update = await this.core.text({
          kind: 'handle_confirmed',
          profile: JSON.stringify(response.profile),
          expected: normalized,
        });
        this.session.account.handle = normalized;
        await this.apply(update, generation);
      },
      () => (active() ? null : 'A conta ou conexão mudou. Tente novamente.'),
    );
    return normalized;
  });
  this.handleChanges = task.then(
    () => {},
    () => {},
  );
  return task;
}

export async function directoryProfiles(
  this: MessageProfileOperationsPort,
  profiles: unknown[],
): Promise<unknown> {
  let result: unknown;
  const generation = this.generation;
  await this.enqueue(
    async () => {
      const response = object(
        JSON.parse(
          await this.core.text({ kind: 'directory_profiles', profiles: JSON.stringify(profiles) }),
        ),
      );
      await this.apply(JSON.stringify(response.update), generation);
      result = response.result;
    },
    () => (generation !== this.generation ? 'A conexão mudou. Tente de novo.' : null),
    false,
  );
  return result;
}
