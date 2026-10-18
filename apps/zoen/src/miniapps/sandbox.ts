import { t } from '../i18n/index.ts';
import { sha256 } from '@noble/hashes/sha2.js';
import type { AppResourceDto } from '../core/generated/AppResourceDto';

export type MiniappManifest = {
  id: string;
  name: string;
  capabilities: { id: string; purpose: string }[];
  allowedDomains: string[];
  networkPurpose: string;
  linkDomains: string[];
  sha256: string;
  bytes: number;
};
export function hostname(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.length <= 253 &&
    /^(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(value)
  );
}
export function verifyMiniappResource(
  resource: AppResourceDto,
  appId: string,
): MiniappManifest | null {
  if (
    resource.mime_type !== 'text/html;profile=mcp-app' ||
    new TextEncoder().encode(resource.text).length > 8 * 1024 * 1024
  )
    throw new Error(t('miniapps.thisMiniappSResourceCannotBeOpened'));
  if (!resource.manifest_json) return null; // Only core-bundled legacy views have no manifest.
  const value: unknown = JSON.parse(resource.manifest_json);
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
    return { id: capability.id, purpose: capability.purpose };
  });
  if (new Set(capabilities.map((capability) => capability.id)).size !== capabilities.length)
    throw new Error(t('miniapps.theManifestHasDuplicatePermissions'));
  const bytes = new TextEncoder().encode(resource.text);
  const digest = Array.from(sha256(bytes), (byte) => byte.toString(16).padStart(2, '0')).join('');
  if (bytes.length !== input.bytes || digest !== input.sha256)
    throw new Error(t('miniapps.theMiniappDoesNotMatchTheVerifiedPackage'));
  return {
    id: appId,
    name: input.name,
    capabilities,
    allowedDomains: input.allowedDomains as string[],
    linkDomains: (input.linkDomains as string[] | undefined) ?? [],
    networkPurpose: typeof input.networkPurpose === 'string' ? input.networkPurpose : '',
    bytes: bytes.length,
    sha256: digest,
  };
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

export function safeMiniappLink(input: unknown, manifest: MiniappManifest | null): string {
  if (typeof input !== 'string' || input.length > 4096) throw new Error(t('miniapps.invalidLink'));
  const url = new URL(input);
  if (
    !['https:', 'mailto:'].includes(url.protocol) ||
    url.username ||
    url.password ||
    (manifest && url.protocol === 'https:' && !manifest.linkDomains.includes(url.hostname))
  )
    throw new Error(t('miniapps.thisMiniappDidNotDeclarePermissionToOpen'));
  return url.href;
}
