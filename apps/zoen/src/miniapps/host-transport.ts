import { parseRuntimePackage, type RuntimePackage } from './runtime-package.ts';
import { runtimeSupportScript } from './vendor/runtime-support.ts';
const MAX_MESSAGE_LENGTH = 1_500_000;
const MAX_DOCUMENT_LENGTH = 9 * 1024 * 1024;
const validNonce = (value: string) => /^[a-f0-9-]{36}$/i.test(value);
const scriptValue = (value: unknown) =>
  (JSON.stringify(value) ?? 'null')
    .replace(/</g, '\\u003c')
    .replace(/\u2028/g, '\\u2028')
    .replace(/\u2029/g, '\\u2029');

/** A trusted parent Window, not a replacement for the app's postMessage transport. */
export function nativeMiniappHtml(
  html: string,
  title: string,
  nonce: string,
  runtime?: RuntimePackage,
): string {
  if (!validNonce(nonce) || html.length > MAX_DOCUMENT_LENGTH || title.length > 1024)
    throw new Error('Invalid app document');
  const runtimeSetup = runtime
    ? `${runtimeSupportScript};const packageHost=__zoenRuntimeSupport.createRuntimeHost(()=>frame.contentWindow,${scriptValue(parseRuntimePackage(runtime))},()=>{if(!active)throw Error('App runtime retired')});`
    : 'const packageHost=null;';
  // The app's own CSP stays inside srcdoc. An outer CSP would also be inherited
  // by that document and could silently override its declared resource policy.
  return `<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,viewport-fit=cover"><meta name="referrer" content="no-referrer"><style>html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}iframe{display:block;width:100%;height:100%;border:0;background:transparent}</style></head><body><iframe id="miniapp" sandbox="allow-scripts" allow="camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'" referrerpolicy="no-referrer"></iframe><script>(function(){
const frame=document.getElementById('miniapp');
const nonce=${scriptValue(nonce)};
let active=true;
${runtimeSetup}
const rpc=value=>value&&typeof value==='object'&&!Array.isArray(value)&&value.jsonrpc==='2.0';
const receive=event=>{
  if(packageHost?.receive(event))return;
  if(!active||event.source!==frame.contentWindow||event.origin!=='null'||!rpc(event.data))return;
  try{
    const message=JSON.stringify(event.data);
    if(message.length>${MAX_MESSAGE_LENGTH})return;
    window.ReactNativeWebView.postMessage(JSON.stringify({nonce,message}));
  }catch{}
};
const dispose=expected=>{
  if(!active||expected!==nonce)return false;
  active=false;
  packageHost?.dispose();
  window.removeEventListener('message',receive);
  frame.srcdoc='';
  frame.remove();
  return true;
};
window.__zoenHostTransport=Object.freeze({
  title:(expected,title)=>{
    if(!active||expected!==nonce||typeof title!=='string'||title.length>1024)return false;
    frame.title=title;
    return true;
  },
  send:(expected,message)=>{
    if(!active||expected!==nonce||!rpc(message))return false;
    try{
      if(JSON.stringify(message).length>${MAX_MESSAGE_LENGTH})return false;
      frame.contentWindow.postMessage(message,'*');
      return true;
    }catch{return false;}
  },
  dispose
});
window.addEventListener('message',receive);
window.addEventListener('pagehide',()=>dispose(nonce),{once:true});
frame.title=${scriptValue(title)};
frame.srcdoc=${scriptValue(html)};
})();</script></body></html>`;
}

/** Raw WK message handlers exist in child frames too; only this document's shell may relay. */
export function nativeMiniappMessage(encoded: string, nonce: string): string | null {
  if (!validNonce(nonce) || encoded.length > MAX_MESSAGE_LENGTH * 2 + 128) return null;
  try {
    const envelope: unknown = JSON.parse(encoded);
    if (!envelope || typeof envelope !== 'object' || Array.isArray(envelope)) return null;
    const value = envelope as Record<string, unknown>;
    if (
      Object.keys(value).sort().join(',') !== 'message,nonce' ||
      value.nonce !== nonce ||
      typeof value.message !== 'string' ||
      value.message.length > MAX_MESSAGE_LENGTH
    )
      return null;
    const message: unknown = JSON.parse(value.message);
    return message &&
      typeof message === 'object' &&
      !Array.isArray(message) &&
      'jsonrpc' in message &&
      message.jsonrpc === '2.0'
      ? value.message
      : null;
  } catch {
    return null;
  }
}

/** Android's modern WebMessageListener reports the opaque shell origin, not its URL. */
export function nativeMiniappMessageSource(url: string, platform: string): boolean {
  return url === 'about:blank' || url === '' || (platform === 'android' && url === 'null');
}

export function nativeMiniappCommand(
  nonce: string,
  action: 'send' | 'title' | 'dispose',
  message?: unknown,
): string {
  if (!validNonce(nonce)) throw new Error('Invalid app document');
  const encoded = JSON.stringify(message);
  const payload = encoded === undefined ? 'null' : `JSON.parse(${scriptValue(encoded)})`;
  const args = action !== 'dispose' ? `${scriptValue(nonce)},${payload}` : scriptValue(nonce);
  return `window.__zoenHostTransport?.${action}(${args});true;`;
}

export function nativeMiniappNavigation(request: { url: string; isTopFrame: boolean }): boolean {
  return (
    request.url === '' ||
    request.url === 'about:blank' ||
    (!request.isTopFrame &&
      (request.url === 'about:srcdoc' || request.url.startsWith('about:srcdoc#')))
  );
}
