export type AuthProvider = 'apple' | 'google';
export type AuthClient = 'ios' | 'web';

/** Ephemeral server challenge; nonce is passed unchanged to each official SDK. */
export type AuthChallenge = {
  challenge_id: string;
  nonce: string;
  state: string;
  audience: string;
  expires_at_ms: number;
};

export type ProviderCredential = {
  id_token: string;
  authorization_code?: string;
  state?: string;
};

/** A provider proof identifies an account; it never unlocks or enrolls an E2E device. */
export type AuthVerification = {
  proof_token: string;
  expires_at_ms: number;
  account: { kind: 'unbound' } | { kind: 'bound'; identity_id: string; handle: string };
  profile?: { name?: string };
};

/** Native Apple names are initial profile hints, not signed identity claims. */
export type VerifiedSignIn = AuthVerification & {
  provider: AuthProvider;
  suggested_name?: string;
};

export type AuthService = {
  challenge(
    provider: AuthProvider,
    client: AuthClient,
    signal: AbortSignal,
  ): Promise<AuthChallenge>;
  verify(
    challenge: AuthChallenge,
    credential: ProviderCredential,
    signal: AbortSignal,
  ): Promise<AuthVerification>;
};

export type ProviderConfiguration = {
  google?: { iosClientId?: string; webClientId: string };
  apple?: { iosAudience: string; serviceId?: string; redirectUri?: string };
};

export type SignInButtonsProps = {
  service: AuthService;
  configuration: ProviderConfiguration;
  /** Changes whenever the account/server context changes, invalidating pending sign-in. */
  scope: string;
  disabled?: boolean;
  /** Parent performs automatic, idempotent signed provisioning or routes secure recovery. */
  onVerified(result: VerifiedSignIn): Promise<void>;
};
