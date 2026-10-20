import { DECKCRAFT_ENGINE_REVISION } from './canonical-binary.ts';
import type { RuntimePackage } from './runtime-package';

// Temporary, explicitly enabled QA telemetry. Never an App/agent command channel.
const MARKER = 'zoen.deckcraft.focus.qa.v1';
const TOKEN = /^[a-f0-9-]{36}$/i;
const EVENTS = ['ready', 'focus', 'pointerup', 'touchend', 'settled'] as const;
const TAGS = ['NONE', 'HTML', 'BODY', 'IFRAME', 'CANVAS', 'INPUT', 'TEXTAREA', 'OTHER'] as const;
const BOOLEANS = [
  'hasFocus',
  'canvasFocused',
  'inputFocused',
  'isTop',
  'inert',
  'disabled',
] as const;
const KEYS = ['marker', 'session', 'slot', 'event', 'activeTag', 'inputCount', ...BOOLEANS];
export type DeckcraftFocusQaSnapshot = Readonly<{
  slot: 'top' | 'childCanvas';
  event: (typeof EVENTS)[number];
  activeTag: (typeof TAGS)[number];
  inputCount: number;
  hasFocus: boolean;
  canvasFocused: boolean;
  inputFocused: boolean;
  isTop: boolean;
  inert: boolean;
  disabled: boolean;
}>;
export type DeckcraftFocusQaSlots = Readonly<{
  top: DeckcraftFocusQaSnapshot | null;
  childCanvas: DeckcraftFocusQaSnapshot | null;
}>;
export const emptyDeckcraftFocusQa = (): DeckcraftFocusQaSlots => ({
  top: null,
  childCanvas: null,
});

export function deckcraftFocusQaEnabled(
  platform: string,
  flag: string | undefined,
  runtime: RuntimePackage | undefined,
): boolean {
  return (
    flag === '1' &&
    platform === 'ios' &&
    runtime?.schema === 1 &&
    runtime.engine === 'deckcraft' &&
    runtime.abi === 'zoen-host-v1' &&
    runtime.revision === DECKCRAFT_ENGINE_REVISION
  );
}

/** Strict, small scalar diagnostics only; no field can carry document/URL/text data. */
export function decodeDeckcraftFocusQa(
  encoded: unknown,
  session: string,
): DeckcraftFocusQaSnapshot | null {
  if (!TOKEN.test(session) || typeof encoded !== 'string' || encoded.length > 768) return null;
  try {
    const value: unknown = JSON.parse(encoded);
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
    const data = value as Record<string, unknown>;
    if (
      Object.keys(data).sort().join(',') !== [...KEYS].sort().join(',') ||
      data.marker !== MARKER ||
      data.session !== session ||
      (data.slot !== 'top' && data.slot !== 'childCanvas') ||
      !EVENTS.some((event) => event === data.event) ||
      !TAGS.some((tag) => tag === data.activeTag) ||
      !BOOLEANS.every((key) => typeof data[key] === 'boolean') ||
      !Number.isSafeInteger(data.inputCount) ||
      (data.inputCount as number) < 0 ||
      (data.inputCount as number) > 8 ||
      data.isTop !== (data.slot === 'top')
    )
      return null;
    return Object.freeze({
      slot: data.slot,
      event: data.event as DeckcraftFocusQaSnapshot['event'],
      activeTag: data.activeTag as DeckcraftFocusQaSnapshot['activeTag'],
      inputCount: data.inputCount as number,
      hasFocus: data.hasFocus as boolean,
      canvasFocused: data.canvasFocused as boolean,
      inputFocused: data.inputFocused as boolean,
      isTop: data.isTop as boolean,
      inert: data.inert as boolean,
      disabled: data.disabled as boolean,
    });
  } catch {
    return null;
  }
}

/** AX output is reconstructed from validated scalars, never the received JSON. */
export function deckcraftFocusQaLabel(slots: DeckcraftFocusQaSlots): string {
  const format = (value: DeckcraftFocusQaSnapshot | null) =>
    value
      ? `${value.event} tag=${value.activeTag} focus=${+value.hasFocus} canvas=${+value.canvasFocused} input=${+value.inputFocused} top=${+value.isTop} inert=${+value.inert} disabled=${+value.disabled} inputs=${value.inputCount}`
      : 'pending';
  return `top: ${format(slots.top)} | childCanvas: ${format(slots.childCanvas)}`;
}

/**
 * WK document-start script, all frames. It observes fixed DOM identities only.
 * This separate token is NOT the canonical RPC nonce. Child telemetry is untrusted.
 */
export function deckcraftFocusQaScript(session: string): string {
  if (!TOKEN.test(session)) throw new Error('Invalid QA document token');
  return `(function(){
const session=${JSON.stringify(session)};
let active=true,timer=0,raf=0,emissions=0;
const isTop=window===window.top;
const report=event=>{
  if(!active||emissions>=64)return;
  try{
    const canvas=document.getElementById('deckcraft_canvas');
    if(!isTop&&!canvas)return;
    const candidate=canvas?.nextElementSibling;
    const input=candidate?.tagName==='INPUT'?candidate:null;
    const tag=document.activeElement?.tagName||'NONE';
    const activeTag=['NONE','HTML','BODY','IFRAME','CANVAS','INPUT','TEXTAREA'].includes(tag)?tag:'OTHER';
    const snapshot={marker:${JSON.stringify(MARKER)},session,slot:isTop?'top':'childCanvas',event,activeTag,
      inputCount:Math.min(8,document.querySelectorAll('input').length),
      hasFocus:document.hasFocus(),canvasFocused:!!canvas?.matches(':focus'),inputFocused:!!input?.matches(':focus'),
      isTop,inert:!!input?.closest('[inert]'),disabled:!!input?.disabled};
    const bridge=window.webkit?.messageHandlers?.ReactNativeWebView;
    if(!bridge)return;
    emissions++;bridge.postMessage(JSON.stringify(snapshot));
  }catch{}
};
const later=()=>{
  clearTimeout(timer);cancelAnimationFrame(raf);
  timer=setTimeout(()=>{if(active)raf=requestAnimationFrame(()=>report('settled'));},500);
};
const ready=()=>{report('ready');later();};
const focus=()=>{report('focus');later();};
const pointer=()=>{report('pointerup');later();};
const touch=()=>{report('touchend');later();};
document.addEventListener('focus',focus,true);
document.addEventListener('blur',focus,true);
document.addEventListener('pointerup',pointer,{capture:true,passive:true});
document.addEventListener('touchend',touch,{capture:true,passive:true});
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',ready,{once:true});else ready();
window.addEventListener('pagehide',()=>{
  active=false;clearTimeout(timer);cancelAnimationFrame(raf);
  document.removeEventListener('focus',focus,true);document.removeEventListener('blur',focus,true);
  document.removeEventListener('pointerup',pointer,true);document.removeEventListener('touchend',touch,true);
  document.removeEventListener('DOMContentLoaded',ready);
},{once:true});
})();true;`;
}
