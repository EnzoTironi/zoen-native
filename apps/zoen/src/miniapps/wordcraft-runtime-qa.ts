import type { RuntimePackage } from './runtime-package';
import { WORDCRAFT_ENGINE_REVISION } from './binary-formats.ts';
const MARKER = 'zoen.wordcraft.runtime.qa.v1';
const TOKEN = /^[a-f0-9-]{36}$/i;
export type WordcraftRuntimeQa = Readonly<{
  sample: number;
  eventSerial: number;
  ownerReady: boolean;
  engine: number;
  frozen: number;
  version: string;
  sha256: string;
  dom: readonly number[];
  activation: readonly (readonly number[])[];
}>;
export function wordcraftRuntimeQaEnabled(
  platform: string,
  flag: string | undefined,
  runtime?: RuntimePackage,
) {
  return (
    platform === 'ios' &&
    flag === '1' &&
    runtime?.schema === 1 &&
    runtime.engine === 'wordcraft' &&
    runtime.abi === 'zoen-host-v1' &&
    runtime.revision === WORDCRAFT_ENGINE_REVISION
  );
}
export function wordcraftRuntimeQaOwnerReady(message: string): boolean | null {
  try {
    const value = JSON.parse(message);
    const ready = value?.params?.structuredContent?.zoen?.binarySave?.ready;
    return value?.jsonrpc === '2.0' &&
      value?.method === 'ui/update-model-context' &&
      typeof ready === 'boolean'
      ? ready
      : null;
  } catch {
    return null;
  }
}
/** Fixed diagnostic trigger, sent only after the existing canonical frame admits a message. */
export function wordcraftRuntimeQaCommand(session: string, ownerReady: boolean) {
  if (!TOKEN.test(session)) throw Error('Invalid QA document token');
  return `document.getElementById('miniapp')?.contentWindow?.postMessage(${JSON.stringify({ marker: MARKER, session, kind: 'snapshot', ownerReady })},'*');true;`;
}
function validDom(dom: unknown): dom is number[] {
  if (
    !Array.isArray(dom) ||
    dom.length !== 25 ||
    !dom.every((n) => typeof n === 'number' && Number.isFinite(n) && Math.abs(n) <= 65535)
  )
    return false;
  return (
    [0, 1, 2, 3].includes(dom[0]) &&
    [0, 1].includes(dom[1]) &&
    [-1, 0, 1].includes(dom[2]) &&
    [0, 1].includes(dom[3]) &&
    dom[4] > 0 &&
    dom[4] <= 16 &&
    [0, 1, 2].includes(dom[5]) &&
    [8, 9, 12, 13, 14, 15, 16, 17, 20, 21, 22].every((index) => dom[index] >= 0) &&
    [14, 15, 16, 17].every((index) => Number.isInteger(dom[index])) &&
    [-1, 0, 1].includes(dom[18]) &&
    [-1, 0, 1].includes(dom[19])
  );
}
export function decodeWordcraftRuntimeQa(
  encoded: unknown,
  session: string,
): WordcraftRuntimeQa | null {
  if (!TOKEN.test(session) || typeof encoded !== 'string' || encoded.length > 8192) return null;
  try {
    const v = JSON.parse(encoded);
    if (
      !v ||
      Object.keys(v).sort().join(',') !==
        'activation,dom,engine,eventSerial,frozen,marker,ownerReady,sample,session,sha256,version' ||
      v.marker !== MARKER ||
      v.session !== session ||
      !Number.isInteger(v.sample) ||
      v.sample < 1 ||
      v.sample > 64 ||
      !Number.isSafeInteger(v.eventSerial) ||
      v.eventSerial < 0 ||
      v.eventSerial > 65535 ||
      typeof v.ownerReady !== 'boolean' ||
      ![0, 1, 2].includes(v.engine) ||
      ![-1, 0, 1].includes(v.frozen) ||
      typeof v.version !== 'string' ||
      !/^(|[1-9][0-9]{0,19})$/.test(v.version) ||
      typeof v.sha256 !== 'string' ||
      !/^(|[a-f0-9]{64})$/.test(v.sha256) ||
      (v.engine === 1
        ? !v.version || !v.sha256 || v.frozen === -1
        : v.version !== '' || v.sha256 !== '' || v.frozen !== -1) ||
      !validDom(v.dom) ||
      !Array.isArray(v.activation) ||
      v.activation.length > 16 ||
      !v.activation.every(
        (r: unknown) =>
          Array.isArray(r) &&
          r.length === 5 &&
          r.every(Number.isSafeInteger) &&
          r[0] >= 1 &&
          r[0] <= 65535 &&
          r[1] >= 0 &&
          r[1] <= 11 &&
          [0, 1].includes(r[2]) &&
          [-1, 0, 1].includes(r[3]) &&
          [-1, 0, 1].includes(r[4]),
      )
    )
      return null;
    return Object.freeze({
      sample: v.sample,
      eventSerial: v.eventSerial,
      ownerReady: v.ownerReady,
      engine: v.engine,
      frozen: v.frozen,
      version: v.version,
      sha256: v.sha256,
      dom: Object.freeze([...v.dom]),
      activation: Object.freeze(v.activation.map((r: number[]) => Object.freeze([...r]))),
    });
  } catch {
    return null;
  }
}
export function wordcraftRuntimeQaLabel(v: WordcraftRuntimeQa | null) {
  return v
    ? `word runtime: ${v.sample}/${v.eventSerial}/${+v.ownerReady}/${v.engine}/${v.frozen}; head=${v.version}/${v.sha256}; dom=${v.dom.join('/')}; activation=${v.activation.map((r) => r.join('/')).join(';')}`
    : 'word runtime: pending';
}
/** Existing all-frame WK QA channel; no text, scope IDs, credentials, focus or mutation calls.
 * dom: active role, hasFocus, visibility, composing, DPR, input pointerEvents,
 * input x/y/w/h, canvas x/y/w/h/backingW/H, innerW/H, userActivation active/ever,
 * visualViewport W/H/scale/offsetTop/offsetLeft. Unknown booleans/enums = -1.
 * activation rows: serial, event kind, trusted, activation active/ever. Captured
 * synchronously with DOM after the read-only status reply; eventSerial binds
 * the copied rows to that observation. Neither UA nor later-task observations claim native admission.
 */
export function wordcraftRuntimeQaScript(session: string) {
  if (!TOKEN.test(session)) throw Error('Invalid QA document token');
  return `(function(){
const session=${JSON.stringify(session)},marker=${JSON.stringify(MARKER)},revision=${JSON.stringify(WORDCRAFT_ENGINE_REVISION)};
let alive=true,ready=false,busy=false,again=false,timer=0,sample=0,serial=0,composing=false,mount=null;
const activation=[],kinds=['pointerdown','pointerup','pointercancel','touchstart','touchend','touchcancel','focus','blur','beforeinput','input','compositionstart','compositionend'];
const bit=v=>typeof v==='boolean'?+v:-1,n=v=>Number.isFinite(v)?Math.max(-65535,Math.min(65535,Math.round(v*1000)/1000)):0;
const elements=()=>{const canvas=document.getElementById('wordcraft_canvas'),input=canvas?.nextElementSibling;return canvas?.tagName==='CANVAS'&&input?.tagName==='INPUT'&&input.type==='text'&&input.previousElementSibling===canvas&&input.parentElement===canvas.parentElement?{canvas,input}:null};
const role=(v,e)=>!v?0:v===e.input?1:v===e.canvas?2:3;
const rect=v=>{const r=v.getBoundingClientRect();return[r.x,r.y,r.width,r.height].map(n)};
const take=async()=>{
 timer=0;if(!alive||busy||sample>=64)return;const e=elements();if(!e){if(!mount&&typeof MutationObserver==='function'){mount=new MutationObserver(()=>{if(elements()){mount.disconnect();mount=null;schedule();}});mount.observe(document,{childList:true,subtree:true});}return;}
 const ownerReady=ready;let engine=0,frozen=-1,version='',sha256='';busy=true;
 try{if(ownerReady&&window.__zoenRuntime){const bindings=await window.__zoenRuntime.load();if(!alive||ready!==ownerReady)return;const r=await bindings.zoenHostRequest(JSON.stringify({op:'status'}),new Uint8Array());const s=JSON.parse(r.metadata);if(s.engineRevision!==revision||typeof s.frozen!=='boolean'||typeof s.head?.version!=='string'||!/^[1-9][0-9]{0,19}$/.test(s.head.version)||typeof s.head?.sha256!=='string'||!/^[a-f0-9]{64}$/.test(s.head.sha256))throw Error();engine=1;frozen=+s.frozen;version=s.head.version;sha256=s.head.sha256;}}catch{engine=2;}finally{busy=false;if(again){again=false;schedule();}}
 if(!alive||ready!==ownerReady||elements()?.input!==e.input||!e.input.isConnected)return;
 const ua=navigator.userActivation,vv=window.visualViewport,pe=getComputedStyle(e.input).pointerEvents;
 const dom=[role(document.activeElement,e),+document.hasFocus(),document.visibilityState==='visible'?1:document.visibilityState==='hidden'?0:-1,+composing,n(devicePixelRatio),pe==='none'?0:pe==='auto'?1:2,...rect(e.input),...rect(e.canvas),n(e.canvas.width),n(e.canvas.height),n(innerWidth),n(innerHeight),bit(ua?.isActive),bit(ua?.hasBeenActive),n(vv?.width),n(vv?.height),n(vv?.scale),n(vv?.offsetTop),n(vv?.offsetLeft)];
 const capturedActivation=activation.map(row=>row.slice()),eventSerial=Math.min(65535,serial);
 try{window.webkit?.messageHandlers?.ReactNativeWebView?.postMessage(JSON.stringify({marker,session,sample:++sample,eventSerial,ownerReady,engine,frozen,version,sha256,dom,activation:capturedActivation}));}catch{}
};
const schedule=()=>{if(busy){again=true;return;}if(alive&&!timer&&sample<64)timer=setTimeout(take,0)};
const observe=e=>{if(!alive||!elements())return;if(e.type==='compositionstart')composing=true;if(e.type==='compositionend')composing=false;const ua=navigator.userActivation;activation.push([Math.min(65535,++serial),kinds.indexOf(e.type),+!!e.isTrusted,bit(ua?.isActive),bit(ua?.hasBeenActive)]);if(activation.length>16)activation.shift();schedule();};
const receive=e=>{const d=e.data;if(e.source!==window.parent||!d||Object.keys(d).sort().join(',')!=='kind,marker,ownerReady,session'||d.marker!==marker||d.session!==session||d.kind!=='snapshot'||typeof d.ownerReady!=='boolean')return;ready=d.ownerReady;schedule();};
for(const kind of kinds)document.addEventListener(kind,observe,{capture:true,passive:true});window.addEventListener('message',receive);
window.addEventListener('pagehide',()=>{alive=false;clearTimeout(timer);mount?.disconnect();mount=null;for(const kind of kinds)document.removeEventListener(kind,observe,true);window.removeEventListener('message',receive);},{once:true});
})();true;`;
}
