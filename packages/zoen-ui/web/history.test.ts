import assert from 'node:assert/strict';
import test from 'node:test';
import {boundedHistoryBody,historySlice} from './history.ts';

test('history body enforces actual streamed bytes even when the server omits its length',async()=>{
  let cancelled=false;
  const body=new ReadableStream<Uint8Array>({start(controller){controller.enqueue(new Uint8Array(8));controller.enqueue(new Uint8Array(9));},cancel(){cancelled=true;}});
  await assert.rejects(boundedHistoryBody(new Response(body),16),/grande demais/);
  assert.equal(cancelled,true);
  await assert.rejects(boundedHistoryBody(new Response(new Uint8Array(1),{headers:{'content-length':'9999999999999999999999999'}}),16),/grande demais/);
});

test('history cleanup follows the imported durable update and persists its acknowledgement',async()=>{
  const order:string[]=[];
  const cleanup={transfer:'ab'.repeat(32),device:'cd'.repeat(32),timestamp:'20',signature:'ef'.repeat(64)};
  const result=await historySlice({base:'https://relay.example',signal:new AbortController().signal,valid:()=>true,
    rpc:async()=>{throw new Error('unexpected mailbox');},
    core:async(operation)=>{order.push(operation.kind);return operation.kind==='history_start'?{kind:'complete',messages:7,chunks:2,resumed:1,cleanup}:{kind:'complete',cleanup:null};},
    fetch:async(input,init)=>{order.push('delete');assert.equal(String(input),`https://relay.example/v1/transfer/${cleanup.transfer}`);assert.equal(init?.method,'DELETE');assert.equal(new Headers(init?.headers).get('x-zoen-sig'),cleanup.signature);return new Response(null,{status:204});},
  });
  assert.deepEqual(result,{messages:7,chunks:2,resumed:1});assert.deepEqual(order,['history_start','delete','history_cleaned']);
  let deleted=false;
  await assert.rejects(historySlice({base:'https://relay.example',signal:new AbortController().signal,valid:()=>true,rpc:async()=>null,
    core:async()=>{throw new Error('storage failed before imported snapshot');},fetch:async()=>{deleted=true;return new Response();}}),/storage failed/);
  assert.equal(deleted,false);
});

test('an account transition during history download cannot import into the closed core',async()=>{
  let valid=true;let coreCalls=0;
  await assert.rejects(historySlice({base:'https://relay.example',signal:new AbortController().signal,valid:()=>valid,rpc:async()=>null,
    core:async()=>{coreCalls++;return {kind:'download',transfer:'ab'.repeat(32),index:0,sha256:'cd'.repeat(32)};},
    fetch:async()=>{valid=false;return new Response(new Uint8Array(16));}}),/sessão mudou/);
  assert.equal(coreCalls,1);
});
