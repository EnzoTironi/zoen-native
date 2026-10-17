import test from 'node:test';
import assert from 'node:assert/strict';
import { buildSync } from 'esbuild';
import { JSDOM } from 'jsdom';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import type { PageDto, PageSaveRequest } from './live-page';
const output = resolve(import.meta.dirname, '../node_modules/.cache/live-page-test.mjs');
buildSync({entryPoints:[resolve(import.meta.dirname,'live-page.tsx')],outfile:output,bundle:true,platform:'node',format:'esm',packages:'external',jsx:'automatic',loader:{'.css':'empty'}});
const dom = new JSDOM('<div id="root"></div>',{url:'http://localhost/'});
Object.assign(globalThis,{window:dom.window,document:dom.window.document,HTMLElement:dom.window.HTMLElement,IS_REACT_ACT_ENVIRONMENT:true});
Object.defineProperty(globalThis,'navigator',{configurable:true,value:dom.window.navigator});
const {createElement:h,act} = await import('react'); const {createRoot} = await import('react-dom/client');
const {LivePage} = await import(pathToFileURL(output).href);
const page = (): PageDto => ({item_id:'p',space_id:'s',title:'Notes',path:'notes.md',version:1,ready:true,unsaved:false,pending_sync:false,save_error:null,can_edit:true,content_hash:'one',edit_context:'ctx-one',blocks:[{id:'b',kind:'paragraph',level:0,indent:0,number:0,checked:false,lang:'',url:'',alt:'',text:'Original',spans:[{start:0,end:8,key:'b',value:''}]}]});
async function click(label:string){const b=[...document.querySelectorAll('button')].find(b=>b.textContent===label);assert.ok(b);await act(async()=>b.click());}
async function input(text:string){const el=document.querySelector('[role=textbox]')!;await act(async()=>{el.textContent=text;el.dispatchEvent(new dom.window.Event('input',{bubbles:true}));});}
test('actual editor retains failed edits, retries same CRDT context and shows remote conflict',async()=>{
 const root=createRoot(document.getElementById('root')!);const original=page();const requests:PageSaveRequest[]=[];let fail=true;let conflicts=0;
 const save=async(r:PageSaveRequest)=>{requests.push(r);if(fail)throw Error('Offline');return {...original,blocks:r.blocks,content_hash:'saved',edit_context:'ctx-saved'};};
 const remote={...original,edit_context:'ctx-remote',content_hash:'remote',version:2,blocks:[{...original.blocks[0],text:'Remote'}]};
 const render=(p:PageDto)=>root.render(h(LivePage,{page:p,save,refresh:async()=>remote,onConflict:()=>{conflicts++;}}));
 try{await act(async()=>render(original));assert.equal(document.querySelector('strong')?.textContent,'Original');await input('My edits');await click('Save');assert.match(document.querySelector('[role=alert]')!.textContent!,/Offline/);assert.equal(document.querySelector('[role=textbox]')!.textContent,'My edits');await act(async()=>render(remote));assert.ok(conflicts);assert.equal(document.querySelector('[role=textbox]')!.textContent,'My edits');fail=false;await click('Retry save');assert.equal(requests.length,2);assert.equal(requests[0].edit_context,'ctx-one');assert.equal(requests[1].edit_id,requests[0].edit_id);assert.deepEqual(requests[1].base_blocks,original.blocks);assert.equal(requests[1].changed[0].text,'My edits');assert.equal(document.querySelector('[role=textbox]')!.textContent,'My edits');}
 finally{await act(async()=>root.unmount());}
});
test('pending save retains content; explicit refresh discards only by user action',async()=>{
 const root=createRoot(document.getElementById('root')!);const original=page();let finish!:(p:PageDto)=>void;const remote={...original,edit_context:'remote',blocks:[{...original.blocks[0],text:'New remote'}]};const save=()=>new Promise<PageDto>(resolve=>{finish=resolve;});
 const render=(p:PageDto)=>root.render(h(LivePage,{page:p,save,refresh:async()=>remote}));
 try{await act(async()=>render(original));await input('Pending text');await click('Save');assert.equal(document.querySelector('[role=textbox]')!.getAttribute('contenteditable'),'false');assert.equal(document.querySelector('[role=textbox]')!.textContent,'Pending text');await act(async()=>render(remote));assert.equal(document.querySelector('[role=textbox]')!.textContent,'Pending text');await act(async()=>finish({...original,save_error:'Storage full'}));assert.match(document.body.textContent!,/Storage full/);await click('Discard edits and refresh');assert.equal(document.querySelector('[role=textbox]')!.textContent,'New remote');}
 finally{await act(async()=>root.unmount());}
});
test('toolbar formats real selected text and converts blocks into checklists',async()=>{
 const root=createRoot(document.getElementById('root')!);const original=page();original.blocks[0].spans=[];let saved:PageSaveRequest|undefined;
 try{await act(async()=>root.render(h(LivePage,{page:original,save:async(r:PageSaveRequest)=>{saved=r;return {...original,blocks:r.blocks,edit_context:'formatted'};},refresh:async()=>original})));const el=document.querySelector('[role=textbox]')!;const range=document.createRange();range.selectNodeContents(el);window.getSelection()!.removeAllRanges();window.getSelection()!.addRange(range);await act(async()=>el.dispatchEvent(new dom.window.MouseEvent('mouseup',{bubbles:true})));await click('Bold');assert.equal(document.querySelector('strong')!.textContent,'Original');await click('Heading');assert.ok(document.querySelector('.live-page-body h1'));await click('Checklist');const checkbox=document.querySelector('input[type=checkbox]') as HTMLInputElement;assert.ok(checkbox);await act(async()=>checkbox.click());await click('Save');assert.equal(saved?.blocks[0].kind,'task');assert.equal(saved?.blocks[0].checked,true);assert.deepEqual(saved?.blocks[0].spans,[{start:0,end:8,key:'b',value:''}]);}
 finally{await act(async()=>root.unmount());}
});
test('plain text paste cannot inject HTML and unsafe native links are not rendered',async()=>{
 const root=createRoot(document.getElementById('root')!);const original=page();original.blocks[0].spans=[{start:0,end:8,key:'a',value:'javascript:alert(1)'}];let saved:PageSaveRequest|undefined;
 try{await act(async()=>root.render(h(LivePage,{page:original,save:async(r:PageSaveRequest)=>{saved=r;return {...original,blocks:r.blocks};},refresh:async()=>original})));assert.equal(document.querySelector('a'),null);const el=document.querySelector('[role=textbox]')!;const range=document.createRange();range.selectNodeContents(el);const selection=window.getSelection()!;selection.removeAllRanges();selection.addRange(range);const paste=new dom.window.Event('paste',{bubbles:true,cancelable:true});Object.defineProperty(paste,'clipboardData',{value:{getData:(type:string)=>type==='text/plain'?'<img src=x onerror=alert(1)>':''}});await act(async()=>el.dispatchEvent(paste));assert.equal(document.querySelector('img'),null);await click('Save');assert.equal(saved?.blocks[0].text,'<img src=x onerror=alert(1)>');}
 finally{await act(async()=>root.unmount());}
});
test('saved failed draft restores original context and retries after remount', async () => {
 const original = page();
 const draft: PageSaveRequest = { item_id: 'p', edit_id: 'retry-id', edit_context: 'ctx-old', base_blocks: original.blocks, blocks: [{ ...original.blocks[0], text: 'Recovered' }], order: ['b'], changed: [{ ...original.blocks[0], text: 'Recovered' }] };
 const root = createRoot(document.getElementById('root')!); let sent: PageSaveRequest | undefined;
 try {
  await act(async () => root.render(h(LivePage, { page: { ...original, draft, unsaved: true, save_error: 'Rejected' }, refresh: async () => original, save: async (request: PageSaveRequest) => { sent = request; return { ...original, blocks: request.blocks }; } })));
  assert.equal(document.querySelector('[role=textbox]')!.textContent, 'Recovered');
  assert.match(document.body.textContent!, /Rejected/);
  await click('Retry save'); assert.equal(sent?.edit_id, 'retry-id'); assert.equal(sent?.edit_context, 'ctx-old');
 } finally { await act(async () => root.unmount()); }
});
test('dirty state is reported for navigation and remains dirty after a failed save',async()=>{
 const root=createRoot(document.getElementById('root')!);const original=page();const states:boolean[]=[];let fail=true;
 try{await act(async()=>root.render(h(LivePage,{page:original,onDirtyChange:(dirty:boolean)=>states.push(dirty),save:async(r:PageSaveRequest)=>{if(fail)throw Error('Offline');return {...original,blocks:r.blocks,edit_context:'saved'};},refresh:async()=>original})));assert.equal(states.at(-1),false);await input('Unsaved');assert.equal(states.at(-1),true);await click('Save');assert.equal(states.at(-1),true);fail=false;await click('Retry save');assert.equal(states.at(-1),false);}
 finally{await act(async()=>root.unmount());}
});
