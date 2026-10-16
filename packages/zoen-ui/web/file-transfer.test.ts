import { test } from 'node:test';
import assert from 'node:assert/strict';
import { download, prepare, upload, ensurePage, parseManifest, parsePlan, MAX_FILE_BYTES } from './file-transfer.ts';
import type { FileCore } from './file-transfer.ts';
const a = 'a'.repeat(64), b = 'b'.repeat(64);
const headers = { 'x-zoen-device': 'device', 'x-zoen-ts': '1', 'x-zoen-sig': 'sig' };
function fixture() {
  const chunks = [a,b].map(blob => ({blob, bytes:1, sealed_bytes:41, key:a, sha256:b}));
  const doc = {name:'test.txt', mime:'text/plain', bytes:2, sha256:a, chunks};
  const plan = {schema:1, space:'space', item:'item', doc, uploads:chunks.map((c,i) => ({blob:c.blob, body_base64:Buffer.alloc(41,i+1).toString('base64'),headers}))};
  const core: FileCore = {
    fileManifest:async () => JSON.stringify({item:'item', ...doc}),
    fileOpen:async (_item, packed) => {assert.equal(packed.length,82); assert.equal(packed[0],1); assert.equal(packed[41],2); return new Uint8Array([9,8]);},
    filePrepare:async () => JSON.stringify(plan), fileUploadHeaders:async () => JSON.stringify({...headers,'x-zoen-ts':'fresh'}),
    fileCommit:async () => { throw Error('must not commit'); }
  };
  return {core,plan};
}
test('download fetches ciphertext in manifest order, omits credentials, then invokes authenticated core open', async () => {
  const {core} = fixture(); const seen:string[]=[];
  const bytes = await download(core,'item',{relay:'https://relay.example/relay',fetch:async (url,init) => {seen.push(String(url)); assert.equal(init?.credentials,'omit'); assert.equal(init?.redirect,'error'); assert.equal(init?.headers,undefined); return new Response(new Uint8Array(41).fill(seen.length));}});
  assert.deepEqual(seen,[`https://relay.example/relay/v1/blobs/${a}`,`https://relay.example/relay/v1/blobs/${b}`]); assert.deepEqual([...bytes.bytes],[9,8]);
});
test('rejects manifest bounds before network, streams with strict size, and never returns unauthenticated bytes', async () => {
  const {core} = fixture(); let requests=0;
  const fetch:typeof globalThis.fetch=async () => {requests++; return new Response(new Uint8Array(42));};
  const original=core.fileManifest; core.fileManifest=async()=>JSON.stringify({item:'item',name:'x',mime:'x/y',bytes:MAX_FILE_BYTES+1,chunks:[]});
  await assert.rejects(download(core,'item',{relay:'https://relay.example',fetch}),/download failed/); assert.equal(requests,0);
  core.fileManifest=original;
  await assert.rejects(download(core,'item',{relay:'https://relay.example',fetch}),/download failed/);
  core.fileOpen=async()=>{throw Error('secret internal details');};
  await assert.rejects(download(core,'item',{relay:'https://relay.example',fetch:async()=>new Response(new Uint8Array(41).fill(1))}),e=>e instanceof Error && !e.message.includes('secret'));
});
test('durable prepared plan preserves edit ID; upload refreshes headers and completes all PUTs without committing', async()=>{
  const {core} = fixture(); const prepared=await prepare(core,'space','test.txt','text/plain',new Uint8Array(2),1,'stable-edit');
  assert.equal(parsePlan(prepared.planJSON,prepared.editId).editId,'stable-edit'); let puts=0, signed=0;
  core.fileUploadHeaders=async(blob,now)=>{assert.equal(blob,puts===0?a:b);assert.equal(now,99);signed++;return JSON.stringify({...headers,'x-zoen-ts':'99'});};
  await upload(core,prepared,{relay:'https://relay.example',fetch:async (_url,init)=>{puts++; assert.equal(signed,puts);assert.equal(init?.method,'PUT');assert.equal((init?.headers as Record<string,string>)['x-zoen-ts'],'99');assert.equal((init?.body as Uint8Array).length,41);return new Response(null,{status:201});}},()=>99);
  assert.equal(puts,2);
});
test('each chunk reads the clock after the previous upload finishes',async()=>{
  const {core}=fixture(); const prepared=await prepare(core,'space','test.txt','text/plain',new Uint8Array(2),1); let now=100; const signed:number[]=[]; const events:string[]=[];
  core.fileUploadHeaders=async(_blob,timestamp)=>{signed.push(timestamp);events.push('sign');return JSON.stringify(headers);};
  await upload(core,prepared,{relay:'https://relay.example',fetch:async()=>{events.push('put');now+=120_000;return new Response(null,{status:201});}},()=>{events.push('clock');return now;});
  assert.deepEqual(signed,[100,120_100]);
  assert.deepEqual(events,['clock','sign','put','clock','sign','put']);
});
test('refusal and upload failure stop completion; untrusted plan URLs/headers cannot redirect uploads',async()=>{
  const {core,plan}=fixture();const prepared=await prepare(core,'space','test.txt','text/plain',new Uint8Array(2),1);let puts=0;
  await assert.rejects(upload(core,prepared,{relay:'https://relay.example',fetch:async()=>{puts++;return new Response(null,{status:403});}}),/not committed/);assert.equal(puts,1);
  puts=0; plan.uploads[1]!.blob='../escape';
  await assert.rejects(upload(core,{...prepared,planJSON:JSON.stringify(plan)},{relay:'https://relay.example',fetch:async()=>{puts++;return new Response();}}));assert.equal(puts,0);
  await assert.rejects(download(core,'item',{relay:'https://user:password@relay.example',fetch:async()=>{puts++;return new Response();}})); assert.equal(puts,0);
});
test('page chunks are supplied in order; ready empty pages need no network',async()=>{
  const supplied:string[]=[];let requests=0;
  const core={pageDownloadManifest:async()=>JSON.stringify({ready:false,chunks:[a,b].map(blob=>({blob,bytes:1,sealed_bytes:41}))}),pageBlobSupply:async(item:string,blob:string,sealed:Uint8Array)=>{assert.equal(item,'page');assert.equal(sealed.length,41);supplied.push(blob);}};
  const network={relay:'https://relay.example',fetch:async(_url:unknown,init?:RequestInit)=>{requests++;assert.equal(init?.credentials,'omit');assert.equal(init?.redirect,'error');return new Response(new Uint8Array(41));}};
  await ensurePage(core,'page',network);assert.deepEqual(supplied,[a,b]);
  core.pageDownloadManifest=async()=>JSON.stringify({ready:true,chunks:[]});await ensurePage(core,'page',network);assert.equal(requests,2);
});
test('page refusal, truncated chunks, and core rejection never claim readiness',async()=>{
  let supplied=0,requests=0;
  const core={pageDownloadManifest:async()=>JSON.stringify({ready:false,chunks:[a,b].map(blob=>({blob,bytes:1,sealed_bytes:41}))}),pageBlobSupply:async()=>{supplied++;}};
  await assert.rejects(ensurePage(core,'page',{relay:'https://relay.example',fetch:async()=>{requests++;return requests===1?new Response(new Uint8Array(41)):new Response(null,{status:403});}}),/readiness is unverified/);assert.equal(supplied,1);
  supplied=0;await assert.rejects(ensurePage(core,'page',{relay:'https://relay.example',fetch:async()=>new Response(new Uint8Array(40))}));assert.equal(supplied,0);
  core.pageBlobSupply=async()=>{throw Error('private details');};
  await assert.rejects(ensurePage(core,'page',{relay:'https://relay.example',fetch:async()=>new Response(new Uint8Array(41))}),error=>error instanceof Error&&!error.message.includes('private'));
});
test('strict plan rejects invalid base64 and inconsistent chunk sums before decode/network',()=>{
  const {plan}=fixture();plan.uploads[0]!.body_base64='!'.repeat(56);assert.throws(()=>parsePlan(JSON.stringify(plan),'edit'));
  assert.throws(()=>parseManifest(JSON.stringify({item:'x',name:'x',mime:'x/y',bytes:0,chunks:[{blob:a,bytes:1}]})));
});
