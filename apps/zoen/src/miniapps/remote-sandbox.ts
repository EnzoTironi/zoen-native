import type { McpUiResourceCsp } from '@modelcontextprotocol/ext-apps';
import { t } from '../i18n/index.ts';
import { hostname } from './sandbox.ts';

type ResourcePolicy = { readonly [K in keyof McpUiResourceCsp]?: readonly string[] };

function origins(values: readonly string[] | undefined, websocket = false): string[] {
  if (!values) return [];
  if (!Array.isArray(values) || values.length > 32)
    throw new Error(t('miniapps.invalidMiniappDomain'));
  return [
    ...new Set(
      values.map((value) => {
        if (typeof value !== 'string' || value.length > 512)
          throw new Error(t('miniapps.invalidMiniappDomain'));
        // Wildcards cover subdomains only; an arbitrary CSP expression is never a domain.
        const wildcard = /^(?:https|wss):\/\/\*\./.test(value);
        const parsed = new URL(wildcard ? value.replace('://*.', '://wildcard.') : value);
        const domain = wildcard ? parsed.hostname.replace(/^wildcard\./, '') : parsed.hostname;
        if (
          !(parsed.protocol === 'https:' || (websocket && parsed.protocol === 'wss:')) ||
          !hostname(domain) ||
          /\.(?:localhost|local|internal|test|invalid)$/.test(domain) ||
          parsed.username ||
          parsed.password ||
          parsed.search ||
          parsed.hash ||
          parsed.pathname !== '/' ||
          (parsed.port && parsed.port !== '443') ||
          value.includes("'") ||
          value.includes('"') ||
          value.includes(';') ||
          /\s/.test(value)
        )
          throw new Error(t('miniapps.invalidMiniappDomain'));
        const normalized = wildcard
          ? parsed.origin.replace('://wildcard.', '://*.')
          : parsed.origin;
        if (value !== normalized && value !== `${normalized}/`)
          throw new Error(t('miniapps.invalidMiniappDomain'));
        return normalized;
      }),
    ),
  ];
}

/** Standard MCP Apps CSP metadata; presentation never needs a Zoen-specific manifest. */
export function remoteAppCsp(csp: ResourcePolicy): string {
  const connect = origins(csp.connectDomains, true).join(' ') || "'none'";
  const resources = origins(csp.resourceDomains).join(' ');
  const frames = origins(csp.frameDomains).join(' ') || "'none'";
  const base = origins(csp.baseUriDomains).join(' ') || "'none'";
  return `default-src 'none'; script-src 'unsafe-inline' ${resources}; style-src 'unsafe-inline' ${resources}; img-src data: blob: ${resources}; font-src data: ${resources}; media-src data: blob: ${resources}; connect-src ${connect}; worker-src blob:; frame-src ${frames}; object-src 'none'; base-uri ${base}; form-action 'none'`;
}

export function remoteAppHtml(html: string, csp: ResourcePolicy): string {
  if (typeof html !== 'string' || new TextEncoder().encode(html).length > 8 * 1024 * 1024)
    throw new Error(t('miniapps.thisMiniappSResourceCannotBeOpened'));
  // Precede all provider bytes. Searching for a provider's <head> could insert the
  // policy into an HTML comment instead of enforcing it before scripts execute.
  return `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta http-equiv="Content-Security-Policy" content="${remoteAppCsp(csp)}"><meta name="referrer" content="no-referrer"><script>(()=>{for(const key of ['RTCPeerConnection','webkitRTCPeerConnection']){try{Object.defineProperty(window,key,{value:undefined,writable:false,configurable:false})}catch{}}window.open=()=>null;})();</script>${html}`;
}
