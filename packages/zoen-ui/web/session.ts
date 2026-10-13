import { integer, object, publicKey, string } from './boundary.ts';

export function parsePending(json: string) {
  const value = object(JSON.parse(json));
  return {
    device: publicKey(value.device), relay: string(value.relay), hpke_secret: string(value.hpke_secret),
    secret: string(value.secret), code: string(value.code), check: string(value.check), box_id: string(value.box_id),
  };
}
export type PendingLink = ReturnType<typeof parsePending>;

export function parseAccount(json: string) {
  const value = object(JSON.parse(json));
  return {
    identity: publicKey(value.identity), device: publicKey(value.device), cert: string(value.cert),
    name: string(value.name), handle: string(value.handle), relay: string(value.relay), primary: publicKey(value.primary),
    primary_hpke: string(value.primary_hpke), hpke_secret: string(value.hpke_secret), link_secret: string(value.link_secret),
    agreement_secret: string(value.agreement_secret), profile_key: string(value.profile_key),
    profile_key_version: integer(value.profile_key_version), profile_seen_version: integer(value.profile_seen_version),
    profile_fields: string(value.profile_fields),
  };
}
export type LinkedAccount = ReturnType<typeof parseAccount>;

export type StoredSession =
  | { kind: 'empty' }
  | { kind: 'pending'; pending: PendingLink; confirming?: LinkedAccount }
  | { kind: 'linked'; account: LinkedAccount; snapshot: string };

export function parseSession(json: string): StoredSession {
  const value = object(JSON.parse(json));
  switch (value.kind) {
    case 'empty': return { kind: 'empty' };
    case 'pending': {
      const pending = parsePending(JSON.stringify(value.pending));
      if (value.confirming === undefined) return { kind: 'pending', pending };
      return { kind: 'pending', pending, confirming: parseAccount(JSON.stringify(value.confirming)) };
    }
    case 'linked': return { kind: 'linked', account: parseAccount(JSON.stringify(value.account)), snapshot: string(value.snapshot) };
    default: throw new Error('Unrecognized local session.');
  }
}

export function relayUrl(input: string): string {
  const url = new URL(input);
  if (url.username || url.password || url.search || url.hash || url.pathname !== '/') {
    throw new Error('Use the relay origin without a path, password, or query.');
  }
  const local = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (url.protocol !== 'https:' && !(local && url.protocol === 'http:')) {
    throw new Error('A remote relay must use HTTPS.');
  }
  return url.origin;
}

export function socketUrl(relay: string): string {
  const url = new URL(relayUrl(relay));
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.pathname = '/v1/sync';
  return url.toString();
}

export function inviteFromPath(path: string): string | null {
  const match = /^\/join\/([0-9A-HJKMNP-TV-Z]{10})$/i.exec(path);
  return match?.[1]?.toUpperCase() ?? null;
}
