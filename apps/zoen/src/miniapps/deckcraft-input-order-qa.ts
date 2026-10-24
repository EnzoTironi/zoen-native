import type { RuntimePackage } from './runtime-package';

const MARKER = 'zoen.deckcraft.input-order.qa.v2';
const TOKEN = /^[a-f0-9-]{36}$/i;
const MAX_ROWS = 256;
// Phases: 0 document capture, 1 document bubble, 2 a later task. Bubble may
// precede another document listener; none of these claims engine admission.
// Rows contain enums/counts only, never event.data, input text, keys, URLs or errors.
// [serial, eventSerial, phase, eventKind, inputType, composing, utf16Length,
//  selectionStart, selectionEnd, activeRole, keyClass, modifiers, trusted,
//  targetRole, relatedRole, pointerType, isPrimary, touches, changedTouches, defaultPrevented]
// Roles: 0 absent, 1 exact TextAgent, 2 exact canvas, 3 other. Pointer types:
// 0 not pointer, 1 mouse, 2 touch, 3 pen, 4 other. Unknown/not-applicable is -1
// for isPrimary/counts. Later-task defaultPrevented is -1, never a final-event claim.
export type DeckcraftInputOrderRow = readonly number[];
export type DeckcraftInputOrderQa = Readonly<{
  rows: readonly DeckcraftInputOrderRow[];
  truncated: boolean;
}>;
export function deckcraftInputOrderQaEnabled(
  platform: string,
  flag: string | undefined,
  runtime: RuntimePackage | undefined,
  expectedRevision: string,
) {
  return (
    /^[a-f0-9]{40}$/i.test(expectedRevision) &&
    flag === '1' &&
    platform === 'ios' &&
    runtime?.schema === 1 &&
    runtime.engine === 'deckcraft' &&
    runtime.abi === 'zoen-host-v1' &&
    runtime.revision === expectedRevision
  );
}

export function decodeDeckcraftInputOrderQa(
  encoded: unknown,
  session: string,
): DeckcraftInputOrderQa | null {
  if (!TOKEN.test(session) || typeof encoded !== 'string' || encoded.length > 32768) return null;
  try {
    const value: unknown = JSON.parse(encoded);
    if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
    const data = value as Record<string, unknown>;
    if (
      Object.keys(data).sort().join(',') !== 'marker,rows,session,truncated' ||
      data.marker !== MARKER ||
      data.session !== session ||
      typeof data.truncated !== 'boolean' ||
      !Array.isArray(data.rows) ||
      data.rows.length > MAX_ROWS
    )
      return null;
    let serial = 0;
    const rows: DeckcraftInputOrderRow[] = [];
    for (const row of data.rows) {
      if (!Array.isArray(row) || row.length !== 20 || !row.every(Number.isSafeInteger)) return null;
      const [
        n,
        event,
        phase,
        kind,
        inputType,
        composing,
        length,
        start,
        end,
        role,
        key,
        mods,
        trusted,
        target,
        related,
        pointer,
        primary,
        touches,
        changedTouches,
        prevented,
      ] = row as number[];
      const isPointer = kind! >= 9 && kind! <= 11;
      const isTouch = kind! >= 12 && kind! <= 14;
      if (
        n !== ++serial ||
        event! < 1 ||
        event! > MAX_ROWS ||
        phase! < 0 ||
        phase! > 2 ||
        kind! < 0 ||
        kind! > 14 ||
        inputType! < 0 ||
        inputType! > 14 ||
        (composing !== 0 && composing !== 1) ||
        length! < 0 ||
        length! > 65535 ||
        start! < -1 ||
        start! > 65535 ||
        end! < -1 ||
        end! > 65535 ||
        role! < 0 ||
        role! > 3 ||
        key! < 0 ||
        key! > 3 ||
        mods! < 0 ||
        mods! > 15 ||
        (trusted !== 0 && trusted !== 1) ||
        target! < 0 ||
        target! > 3 ||
        related! < 0 ||
        related! > 3 ||
        (kind !== 5 && kind !== 6 && related !== 0) ||
        pointer! < 0 ||
        pointer! > 4 ||
        (isPointer ? pointer === 0 : pointer !== 0) ||
        primary! < -1 ||
        primary! > 1 ||
        (!isPointer && primary !== -1) ||
        touches! < -1 ||
        touches! > 65535 ||
        changedTouches! < -1 ||
        changedTouches! > 65535 ||
        (!isTouch && (touches !== -1 || changedTouches !== -1)) ||
        (phase === 2 ? prevented !== -1 : prevented !== 0 && prevented !== 1)
      )
        return null;
      rows.push(Object.freeze([...row]));
    }
    return Object.freeze({ rows: Object.freeze(rows), truncated: data.truncated });
  } catch {
    return null;
  }
}

/** Reconstruct the AX label from validated enums/counts, never received strings. */
export function deckcraftInputOrderQaLabel(value: DeckcraftInputOrderQa | null) {
  if (!value) return 'input trace: pending';
  return (
    `input trace: ${value.rows.length} rows; truncated=${+value.truncated}; ` +
    value.rows.map((row) => row.join('/')).join(';')
  );
}

/**
 * Diagnostic-only WK document-start observer for all frames. Uses the existing
 * telemetry handler and a separate QA token, never the canonical RPC nonce.
 * Parent must gate this exact configured runtime and active frame document.
 */
export function deckcraftInputOrderQaScript(
  session: string,
  canvasId: 'deckcraft_canvas' | 'wordcraft_canvas' = 'deckcraft_canvas',
) {
  if (!TOKEN.test(session) || !['deckcraft_canvas', 'wordcraft_canvas'].includes(canvasId))
    throw new Error('Invalid QA document token');
  return `(function(){
const session=${JSON.stringify(session)};
const marker=${JSON.stringify(MARKER)};
let alive=true,recording=true,truncated=false,eventSerial=0,timer=0;
const rows=[],ids=new WeakMap(),pendingSamples=new Set();
const kinds=['beforeinput','input','compositionstart','compositionupdate','compositionend','focus','blur','keydown','keyup','pointerdown','pointerup','pointercancel','touchstart','touchend','touchcancel'];
const types=['','insertText','insertReplacementText','insertCompositionText','deleteCompositionText','deleteContentBackward','deleteContentForward','insertFromComposition','insertParagraph','insertLineBreak','insertFromPaste','historyUndo','historyRedo','insertFromDrop','deleteByCut'];
const number=value=>Number.isFinite(value)?Math.min(65535,Math.max(0,Math.trunc(value))):0;
const selection=value=>Number.isInteger(value)?Math.min(65535,Math.max(0,value)):-1;
const count=value=>Number.isInteger(value)?Math.min(65535,Math.max(0,value)):-1;
const emit=()=>{
  if(!alive||!rows.length)return;
  try{window.webkit?.messageHandlers?.ReactNativeWebView?.postMessage(JSON.stringify({marker,session,rows,truncated}));}catch{}
};
const changed=()=>{clearTimeout(timer);timer=setTimeout(emit,250);};
const elements=()=>{
  const canvas=document.getElementById(${JSON.stringify(canvasId)}),input=canvas?.nextElementSibling;
  return canvas?.tagName==='CANVAS'&&input?.tagName==='INPUT'&&input.type==='text'&&
    input.previousElementSibling===canvas&&input.parentElement===canvas.parentElement?{input,canvas}:null;
};
const role=(node,input,canvas)=>!node?0:node===input?1:node===canvas?2:3;
const remove=()=>{for(const kind of kinds){document.removeEventListener(kind,capture,true);document.removeEventListener(kind,bubble,false);}};
const sample=(input,canvas,meta,phase,prevented)=>{
  if(!alive||!recording)return;
  if(rows.length>=${MAX_ROWS}){truncated=true;recording=false;remove();changed();return;}
  const active=role(document.activeElement,input,canvas);
  const stats=meta.target===1&&meta.kind<=8;
  rows.push([rows.length+1,meta.event,phase,meta.kind,meta.type,meta.composing,
    stats?number(input.value.length):0,stats?selection(input.selectionStart):-1,stats?selection(input.selectionEnd):-1,active,
    meta.key,meta.mods,meta.trusted,meta.target,meta.related,meta.pointer,meta.primary,meta.touches,meta.changed,prevented]);
  changed();
};
const capture=event=>{
  if(!alive||!recording)return;
  const found=elements();if(!found)return;
  const kind=kinds.indexOf(event.type);if(kind<0)return;
  const target=role(event.target,found.input,found.canvas);
  if(kind<=8&&kind!==5&&kind!==6&&target!==1)return;
  const pointer=kind>=9&&kind<=11,touch=kind>=12&&kind<=14;
  // Preserve distinct keydown/up kinds; classify0/229 without reading event.key.
  const key=event.type==='keydown'||event.type==='keyup'?
    (event.keyCode===0?1:event.keyCode===229?2:3):0;
  const meta={event:++eventSerial,kind,type:kind<=8?Math.max(0,types.indexOf(event.inputType||'')):0,
    composing:kind<=8?+!!event.isComposing:0,key,mods:(event.shiftKey?1:0)|(event.ctrlKey?2:0)|(event.altKey?4:0)|(event.metaKey?8:0),trusted:+!!event.isTrusted,
    target,related:kind===5||kind===6?role(event.relatedTarget,found.input,found.canvas):0,
    pointer:pointer?(['mouse','touch','pen'].indexOf(event.pointerType)+1||4):0,
    primary:pointer&&typeof event.isPrimary==='boolean'?+event.isPrimary:-1,
    touches:touch?count(event.touches?.length):-1,changed:touch?count(event.changedTouches?.length):-1};
  ids.set(event,{meta,input:found.input,canvas:found.canvas});sample(found.input,found.canvas,meta,0,+!!event.defaultPrevented);
  if(!recording)return;
  // A native listener's microtask checkpoint can precede the target handler.
  // Use a later task, not a microtask, and retain no mutable DOM event.
  // Other events may precede this task; eventSerial preserves that distinction.
  const input=found.input,canvas=found.canvas;
  const pending=setTimeout(()=>{
    pendingSamples.delete(pending);
    const current=elements();
    if(alive&&input.isConnected&&canvas.isConnected&&current?.input===input&&current.canvas===canvas)sample(input,canvas,meta,2,-1);
  },0);
  pendingSamples.add(pending);
};
const bubble=event=>{
  if(!alive||!recording)return;
  const captured=ids.get(event),found=elements();
  if(captured&&found&&captured.input===found.input&&captured.canvas===found.canvas)sample(found.input,found.canvas,captured.meta,1,+!!event.defaultPrevented);
};
for(const kind of kinds){document.addEventListener(kind,capture,{capture:true,passive:true});document.addEventListener(kind,bubble,{capture:false,passive:true});}
window.addEventListener('pagehide',()=>{alive=false;recording=false;clearTimeout(timer);for(const pending of pendingSamples)clearTimeout(pending);pendingSamples.clear();remove();},{once:true});
})();true;`;
}
