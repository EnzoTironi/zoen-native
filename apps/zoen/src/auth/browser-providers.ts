import { AuthCancelled, profileName, webAppleRedirect } from './guards.ts';
import type { AuthTicket } from './attempt';
import type { ProviderCredential } from './types';

export type GoogleCredential = { credential?: unknown; state?: unknown };
export type GoogleIdentity = {
  initialize(options: {
    client_id: string;
    nonce: string;
    callback(response: GoogleCredential): void;
    auto_select: boolean;
    button_auto_select: boolean;
    use_fedcm_for_button: boolean;
    ux_mode: 'popup';
  }): void;
  renderButton(
    element: HTMLElement,
    options: {
      type: 'standard';
      text: 'continue_with';
      theme: 'outline' | 'filled_black';
      size: 'large';
      shape: 'rectangular';
      width: number;
      locale: string;
      state: string;
    },
  ): void;
  cancel(): void;
};
type AppleResponse = {
  authorization?: { id_token?: unknown; code?: unknown; state?: unknown };
  user?: { name?: { firstName?: unknown; lastName?: unknown } };
};
type AppleIdentity = {
  init(options: {
    clientId: string;
    scope: string;
    redirectURI: string;
    nonce: string;
    state: string;
    usePopup: true;
  }): void;
  signIn(): Promise<AppleResponse>;
};
type ProviderWindow = Window & {
  google?: { accounts?: { id?: GoogleIdentity } };
  AppleID?: { auth?: AppleIdentity };
};
const GOOGLE_SCRIPT = 'https://accounts.google.com/gsi/client';
const APPLE_SCRIPT =
  'https://appleid.cdn-apple.com/appleauth/static/jsapi/appleid/1/pt_BR/appleid.auth.js';
const loading = new Map<string, Promise<void>>();
let googleOwner: symbol | null = null;
let applePresenting = false;

/** SDKs execute only on the dedicated auth page, never in an embedded miniapp. */
export function browserAuthContext(): void {
  if (typeof window === 'undefined' || window.top !== window.self)
    throw new Error('Abra o Zoen em sua própria aba para entrar.');
  if (!window.isSecureContext) throw new Error('Use a versão HTTPS do Zoen para entrar.');
  if (window.crossOriginIsolated) throw new Error('Abra a tela de entrada do Zoen para continuar.');
}

function officialScript(key: string, source: string): Promise<void> {
  browserAuthContext();
  const previous = loading.get(key);
  if (previous) return previous;
  const promise = new Promise<void>((resolve, reject) => {
    const script = document.createElement('script');
    script.src = source;
    script.async = true;
    script.referrerPolicy = 'strict-origin-when-cross-origin';
    script.dataset.zoenAuth = key;
    let finished = false;
    const finish = (reason?: Error) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      script.onload = null;
      script.onerror = null;
      if (reason) {
        script.remove();
        loading.delete(key);
        reject(reason);
      } else resolve();
    };
    const timer = setTimeout(
      () => finish(new Error('O provedor demorou para carregar. Tente novamente.')),
      15_000,
    );
    script.onload = () => finish();
    script.onerror = () => finish(new Error('Não foi possível carregar o provedor de login.'));
    document.head.append(script);
  });
  loading.set(key, promise);
  return promise;
}

export async function loadGoogleIdentity(): Promise<GoogleIdentity> {
  await officialScript('google', GOOGLE_SCRIPT);
  const sdk = (window as ProviderWindow).google?.accounts?.id;
  if (!sdk || typeof sdk.initialize !== 'function' || typeof sdk.renderButton !== 'function')
    throw new Error('O Google não pôde ser iniciado.');
  return sdk;
}
export async function loadAppleIdentity(): Promise<AppleIdentity> {
  await officialScript('apple', APPLE_SCRIPT);
  const sdk = (window as ProviderWindow).AppleID?.auth;
  if (!sdk || typeof sdk.init !== 'function' || typeof sdk.signIn !== 'function')
    throw new Error('A Apple não pôde ser iniciada.');
  return sdk;
}

export function mountGoogleButton(
  sdk: GoogleIdentity,
  element: HTMLElement,
  ticket: AuthTicket,
  dark: boolean,
  width: number,
  onCredential: (credential: ProviderCredential) => void,
  onError: (reason: Error) => void,
): () => void {
  browserAuthContext();
  if (ticket.provider !== 'google' || ticket.client !== 'web' || ticket.signal.aborted)
    throw new AuthCancelled();
  const owner = Symbol();
  googleOwner = owner;
  let consumed = false;
  sdk.initialize({
    client_id: ticket.challenge.audience,
    nonce: ticket.challenge.nonce,
    auto_select: false,
    button_auto_select: false,
    use_fedcm_for_button: true,
    ux_mode: 'popup',
    callback: (response) => {
      // Only this SDK callback, this live ticket and the rendered button's state are accepted.
      // No generic window/message listener or URL credential ingestion exists here.
      if (googleOwner !== owner || consumed || ticket.signal.aborted) return;
      if (typeof response.credential !== 'string' || response.state !== ticket.challenge.state) {
        onError(new Error('A resposta não pertence a este login.'));
        return;
      }
      consumed = true;
      onCredential({ id_token: response.credential, state: ticket.challenge.state });
    },
  });
  sdk.renderButton(element, {
    type: 'standard',
    text: 'continue_with',
    theme: dark ? 'filled_black' : 'outline',
    size: 'large',
    shape: 'rectangular',
    width: Math.min(400, Math.max(200, Math.floor(width))),
    locale: 'pt-BR',
    state: ticket.challenge.state,
  });
  return () => {
    consumed = true;
    element.replaceChildren();
    if (googleOwner === owner) {
      googleOwner = null;
      sdk.cancel();
    }
  };
}

export async function appleBrowserCredential(
  sdk: AppleIdentity,
  ticket: AuthTicket,
  redirectUri: string,
): Promise<{ credential: ProviderCredential; suggestedName?: string }> {
  browserAuthContext();
  const redirectURI = webAppleRedirect(redirectUri, window.location.origin);
  if (ticket.provider !== 'apple' || ticket.client !== 'web' || ticket.signal.aborted)
    throw new AuthCancelled();
  if (applePresenting) throw new Error('Conclua o login Apple que já está aberto.');
  if (ticket.challenge.expires_at_ms <= Date.now())
    throw new Error('O login expirou. Tente novamente.');
  applePresenting = true;
  try {
    sdk.init({
      clientId: ticket.challenge.audience,
      scope: 'name email',
      redirectURI,
      nonce: ticket.challenge.nonce,
      state: ticket.challenge.state,
      usePopup: true,
    });
    // Called synchronously from the actual button gesture; no fetch/dynamic import before it.
    let response: AppleResponse;
    try {
      response = await sdk.signIn();
    } catch (reason) {
      const code =
        reason && typeof reason === 'object' && 'error' in reason ? reason.error : undefined;
      if (
        code === 'popup_closed_by_user' ||
        code === 'user_cancelled_authorize' ||
        code === 'user_cancelled'
      )
        throw new AuthCancelled();
      throw new Error('Não foi possível continuar com Apple. Tente novamente.');
    }
    if (ticket.signal.aborted) throw new AuthCancelled();
    const auth = response.authorization;
    if (
      !auth ||
      typeof auth.id_token !== 'string' ||
      typeof auth.code !== 'string' ||
      auth.state !== ticket.challenge.state
    )
      throw new Error('A resposta não pertence a este login.');
    const name = response.user?.name;
    const suggestedName = profileName(
      [name?.firstName, name?.lastName].filter((value) => typeof value === 'string').join(' '),
    );
    return {
      credential: { id_token: auth.id_token, authorization_code: auth.code, state: auth.state },
      suggestedName,
    };
  } finally {
    applePresenting = false;
  }
}

export function appleButtonImage(dark: boolean, width: number): string {
  const url = new URL('https://appleid.cdn-apple.com/appleid/button');
  url.search = new URLSearchParams({
    type: 'continue',
    locale: 'pt_BR',
    color: dark ? 'white' : 'black',
    width: String(Math.min(375, Math.max(130, Math.floor(width)))),
    height: '52',
    scale: '2',
    border_radius: '12',
  }).toString();
  return url.href;
}
