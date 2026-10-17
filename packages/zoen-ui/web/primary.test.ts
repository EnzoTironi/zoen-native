import assert from 'node:assert/strict';
import test from 'node:test';
import {enrollPrimary} from './primary.ts';
import {parseAuthority} from './authority.ts';
import type {StoredSession,LinkedAccount} from './session.ts';
import type {CoreOperation} from './worker-client.ts';

const account:LinkedAccount={identity:'11'.repeat(32),device:'22'.repeat(32),cert:'certificate',name:'Pessoa',handle:'zoen_1111111111111111',relay:'https://relay.example',primary:'22'.repeat(32),primary_hpke:'33'.repeat(32),hpke_secret:'44'.repeat(32),link_secret:'',agreement_secret:'55'.repeat(32),profile_key:'66'.repeat(32),profile_key_version:1,profile_seen_version:1,profile_fields:''};
const authority=parseAuthority({version:1,identity:account.identity,device:account.device,iv:new Uint8Array(12),ciphertext:new Uint8Array(48)});
function fixture(){
  const order:string[]=[];let stored:StoredSession={kind:'empty'};let held:typeof authority|null=null;
  const vault={load:async()=>stored,authority:async()=>held,device:async()=>account.device,sign:async()=>new Uint8Array(64),
    preparePrimary:async(session:Extract<StoredSession,{kind:'provider_pending'}>,envelope:typeof authority)=>{order.push('durable-pending');stored=session;held=envelope;},
    save:async(session:StoredSession)=>{order.push('durable-linked');stored=session;}};
  const core={text:async(operation:CoreOperation)=>{order.push(operation.kind);switch(operation.kind){
    case 'primary_prepare':return JSON.stringify({account,authority:{...authority,iv:Array.from(authority.iv),ciphertext:Array.from(authority.ciphertext)}});
    case 'primary_restore':return JSON.stringify(account);
    case 'primary_finish_bind':return JSON.stringify({proof_token:operation.proof,profile:{id:account.identity},device:account.device,certificate:account.cert,signature:'signed'});
    default:throw new Error('unexpected core text');}},bytes:async(operation:CoreOperation)=>{order.push(operation.kind);return new Uint8Array([1]);}};
  return {vault,core,order,stored:()=>stored,run:(fetcher:typeof fetch,signal=new AbortController().signal)=>enrollPrimary({vault,core,relay:account.relay,name:'Pessoa',proof:'ab'.repeat(32),signal,pending:()=>order.push('pending-visible'),fetch:fetcher})};
}
const confirmed=()=>Response.json({identity_id:account.identity,handle:account.handle});

test('a primary bind never reaches the relay before both protected keys and pending account are durable',async()=>{
  const f=fixture();const linked=await f.run(async(_input,init)=>{f.order.push('bind-http');assert.equal(f.stored().kind,'provider_pending');assert.equal(init?.redirect,'error');return confirmed();});
  assert.equal(linked.kind,'linked');assert.ok(f.order.indexOf('durable-pending')<f.order.indexOf('bind-http'));assert.ok(f.order.indexOf('bind-http')<f.order.indexOf('durable-linked'));
  const refused=fixture();refused.vault.preparePrimary=async()=>{throw new Error('disk failed');};let sent=false;
  await assert.rejects(refused.run(async()=>{sent=true;return confirmed();}),/disk failed/);assert.equal(sent,false);assert.equal(refused.stored().kind,'empty');
});

test('an uncertain bind retries the saved identity and authority after controller replacement',async()=>{
  const f=fixture();await assert.rejects(f.run(async()=>{throw new Error('lost connection after server accepted');}),/preservadas/);
  assert.equal(f.stored().kind,'provider_pending');assert.equal(f.order.filter(kind=>kind==='primary_prepare').length,1);
  const linked=await f.run(async()=>confirmed());assert.equal(linked.account.identity,account.identity);
  assert.equal(f.order.filter(kind=>kind==='primary_prepare').length,1);assert.equal(f.order.filter(kind=>kind==='primary_restore').length,1);
});

test('a mismatched acknowledgement or cancellation cannot expose a registered account',async()=>{
  const mismatch=fixture();await assert.rejects(mismatch.run(async()=>Response.json({identity_id:'33'.repeat(32),handle:account.handle})),/outra conta/);assert.equal(mismatch.stored().kind,'provider_pending');
  const cancelled=fixture();const abort=new AbortController();await assert.rejects(cancelled.run(async()=>{abort.abort();return confirmed();},abort.signal),{name:'AbortError'});assert.equal(cancelled.stored().kind,'provider_pending');
  const failedSave=fixture();failedSave.vault.save=async()=>{throw new Error('durability failure');};await assert.rejects(failedSave.run(async()=>confirmed()),/durability failure/);assert.equal(failedSave.stored().kind,'provider_pending');
});
