import { AuthCancelled, guardCredentialContext, profileName } from './guards.ts';
import type {
  AuthChallenge,
  AuthClient,
  AuthProvider,
  AuthService,
  ProviderCredential,
  VerifiedSignIn,
} from './types';

export type AuthTicket = Readonly<{
  provider: AuthProvider;
  client: AuthClient;
  challenge: AuthChallenge;
  signal: AbortSignal;
}>;
/** Each ticket has an unforgeable in-process owner and exactly one verification attempt. */
export class AuthAttempt {
  private active = true;
  private verifying: symbol | null = null;
  private readonly tickets = new Map<
    AuthTicket,
    { controller: AbortController; consumed: boolean }
  >();
  private readonly pending = new Set<AbortController>();
  private readonly service: AuthService;
  constructor(service: AuthService) {
    this.service = service;
  }
  activate(): void {
    this.active = true;
  }
  async prepare(provider: AuthProvider, client: AuthClient): Promise<AuthTicket> {
    this.check();
    const controller = new AbortController();
    this.pending.add(controller);
    try {
      const challenge = await this.service.challenge(provider, client, controller.signal);
      this.check(controller.signal);
      const ticket = Object.freeze({ provider, client, challenge, signal: controller.signal });
      this.tickets.set(ticket, { controller, consumed: false });
      return ticket;
    } finally {
      this.pending.delete(controller);
    }
  }
  async complete(
    ticket: AuthTicket,
    credential: ProviderCredential,
    suggestedName?: string,
  ): Promise<VerifiedSignIn> {
    this.check(ticket.signal);
    const owner = this.tickets.get(ticket);
    if (!owner || owner.consumed || this.verifying) throw new AuthCancelled();
    owner.consumed = true;
    const operation = Symbol();
    this.verifying = operation;
    try {
      guardCredentialContext(
        ticket.provider,
        ticket.challenge,
        credential,
        ticket.provider === 'apple' || ticket.client === 'web',
      );
      const result = await this.service.verify(
        ticket.challenge,
        { ...credential, state: credential.state ?? ticket.challenge.state },
        ticket.signal,
      );
      this.check(ticket.signal);
      const hint = profileName(suggestedName);
      return { ...result, provider: ticket.provider, ...(hint ? { suggested_name: hint } : {}) };
    } finally {
      if (this.verifying === operation) this.verifying = null;
      this.release(ticket);
    }
  }
  release(ticket: AuthTicket): void {
    this.tickets.get(ticket)?.controller.abort();
    this.tickets.delete(ticket);
  }
  dispose(): void {
    this.active = false;
    this.verifying = null;
    for (const pending of this.pending) pending.abort();
    this.pending.clear();
    for (const ticket of this.tickets.keys()) this.release(ticket);
  }
  private check(signal?: AbortSignal): void {
    if (!this.active || signal?.aborted) throw new AuthCancelled();
  }
}
