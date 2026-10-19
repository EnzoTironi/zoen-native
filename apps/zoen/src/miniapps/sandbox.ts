import { t } from '../i18n/index.ts';
import type { AppResourceDto } from '../core/generated/AppResourceDto';

export type MiniappManifest = Readonly<{
  id: string;
  name: string;
  capabilities: readonly Readonly<{ id: string; purpose: string }>[];
  allowedDomains: readonly string[];
  networkPurpose: string;
  linkDomains: readonly string[];
  sha256: string;
  bytes: number;
}>;
export function hostname(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.length <= 253 &&
    /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(value)
  );
}
/** One immutable, fully checked resource. It grants no device or network permission. */
export type VerifiedMiniappResource = Readonly<{
  resource: Readonly<AppResourceDto>;
  manifest: MiniappManifest;
}>;
const MAX_RESOURCE_BYTES = 8 * 1024 * 1024;

/** Match TextEncoder's USVString semantics; valid Unicode and newlines are unchanged. */
function resourceText(text: string): string {
  return text.replace(/[\uD800-\uDBFF][\uDC00-\uDFFF]|[\uD800-\uDFFF]/g, (value) =>
    value.length === 2 ? value : '\uFFFD',
  );
}

async function resourceDigest(text: string, bytes: Uint8Array<ArrayBuffer>): Promise<string> {
  if (process.env.EXPO_OS === 'android' || process.env.EXPO_OS === 'ios') {
    // SDK58 digest() can synchronously hash despite returning a Promise. This
    // string API actually dispatches Android hashing to Dispatchers.Default.
    const { digestStringAsync, CryptoDigestAlgorithm, CryptoEncoding } =
      await import('expo-crypto');
    return digestStringAsync(CryptoDigestAlgorithm.SHA256, text, { encoding: CryptoEncoding.HEX });
  }
  if (!globalThis.crypto?.subtle)
    throw new Error(t('miniapps.theMiniappPackageCouldNotBeVerified'));
  const digest = await globalThis.crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export async function verifyMiniappResource(
  resource: AppResourceDto,
  appId: string,
): Promise<VerifiedMiniappResource> {
  const { uri, mime_type, text, manifest_json, prefers_border } = resource;
  if (
    mime_type !== 'text/html;profile=mcp-app' ||
    typeof text !== 'string' ||
    text.length > MAX_RESOURCE_BYTES
  )
    throw new Error(t('miniapps.thisMiniappSResourceCannotBeOpened'));
  const pinned = Object.freeze({
    uri,
    mime_type,
    text: resourceText(text),
    manifest_json,
    prefers_border,
  });
  const bytes = new TextEncoder().encode(pinned.text);
  if (bytes.length > MAX_RESOURCE_BYTES)
    throw new Error(t('miniapps.thisMiniappSResourceCannotBeOpened'));
  if (!manifest_json) throw new Error(t('miniapps.theMiniappManifestIsInvalid'));
  const value: unknown = JSON.parse(manifest_json);
  if (!value || typeof value !== 'object' || Array.isArray(value))
    throw new Error(t('miniapps.theMiniappManifestIsInvalid'));
  const input = value as Record<string, unknown>;
  if (
    input.id !== appId ||
    typeof input.name !== 'string' ||
    !Array.isArray(input.capabilities) ||
    !Array.isArray(input.allowedDomains) ||
    !input.allowedDomains.every(hostname) ||
    !Array.isArray(input.linkDomains ?? []) ||
    !((input.linkDomains as unknown[] | undefined) ?? []).every(hostname) ||
    typeof input.sha256 !== 'string' ||
    !/^[a-f0-9]{64}$/.test(input.sha256) ||
    !Number.isSafeInteger(input.bytes)
  )
    throw new Error(t('miniapps.theMiniappManifestDoesNotMatchThisResource'));
  const capabilities = input.capabilities.map((capability: unknown) => {
    if (
      !capability ||
      typeof capability !== 'object' ||
      !('id' in capability) ||
      !('purpose' in capability) ||
      typeof capability.id !== 'string' ||
      !/^[a-zA-Z0-9.:-]{1,64}$/.test(capability.id) ||
      typeof capability.purpose !== 'string' ||
      !capability.purpose.trim() ||
      capability.purpose.length > 1000
    )
      throw new Error(t('miniapps.aPermissionInThisMiniappDoesNotDeclare'));
    return Object.freeze({ id: capability.id, purpose: capability.purpose });
  });
  if (new Set(capabilities.map((capability) => capability.id)).size !== capabilities.length)
    throw new Error(t('miniapps.theManifestHasDuplicatePermissions'));
  if (bytes.length !== input.bytes)
    throw new Error(t('miniapps.theMiniappDoesNotMatchTheVerifiedPackage'));
  // Capture policy strings and arrays before the native/WebCrypto suspension.
  const manifest: MiniappManifest = Object.freeze({
    id: appId,
    name: input.name,
    capabilities: Object.freeze(capabilities),
    allowedDomains: Object.freeze([...(input.allowedDomains as string[])]),
    linkDomains: Object.freeze([...((input.linkDomains as string[] | undefined) ?? [])]),
    networkPurpose: typeof input.networkPurpose === 'string' ? input.networkPurpose : '',
    bytes: bytes.length,
    sha256: input.sha256,
  });
  const digest = await resourceDigest(pinned.text, bytes);
  if (digest !== manifest.sha256)
    throw new Error(t('miniapps.theMiniappDoesNotMatchTheVerifiedPackage'));
  return Object.freeze({ resource: pinned, manifest });
}
export function miniappCsp(allowed: readonly string[]): string {
  if (!allowed.every(hostname)) throw new Error(t('miniapps.invalidMiniappDomain'));
  const network = allowed.map((host) => `https://${host}`).join(' ');
  return `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob: ${network}; media-src data:; font-src data:; connect-src ${network || "'none'"}; worker-src blob:; child-src blob:; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`;
}
export function sandboxHtml(html: string, allowed: readonly string[]): string {
  const security = `<meta http-equiv="Content-Security-Policy" content="${miniappCsp(allowed)}"><meta name="referrer" content="no-referrer"><script>${NETWORK_HARDENING}</script>`;
  return /<head(?:\s[^>]*)?>/i.test(html)
    ? html.replace(/<head(?:\s[^>]*)?>/i, (match) => `${match}${security}`)
    : `${security}${html}`;
}
const NETWORK_HARDENING = `(()=>{for(const key of ['RTCPeerConnection','webkitRTCPeerConnection']){try{Object.defineProperty(window,key,{value:undefined,writable:false,configurable:false})}catch{}}window.open=()=>null;})();`;

export function safeMiniappLink(input: unknown, manifest: MiniappManifest): string {
  if (typeof input !== 'string' || input.length > 4096) throw new Error(t('miniapps.invalidLink'));
  const url = new URL(input);
  if (
    !['https:', 'mailto:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    (url.protocol === 'https:' && !manifest.linkDomains.includes(url.hostname))
  )
    throw new Error(t('miniapps.thisMiniappDidNotDeclarePermissionToOpen'));
  return url.href;
}
