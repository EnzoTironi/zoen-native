import assert from 'node:assert/strict';
import test from 'node:test';
import {IDBFactory} from 'fake-indexeddb';
import {authorityJson,openAuthority,parseAuthority,sealAuthority} from './authority.ts';
import {BrowserVault} from './vault.ts';
import type {StoredSession} from './session.ts';

function storage(t:test.TestContext){const factory=new IDBFactory();const previous=Object.getOwnPropertyDescriptor(globalThis,'indexedDB');Object.defineProperty(globalThis,'indexedDB',{configurable:true,value:factory});t.after(()=>{if(previous)Object.defineProperty(globalThis,'indexedDB',previous);else Reflect.deleteProperty(globalThis,'indexedDB');});return factory;}
function pending(device:string):Extract<StoredSession,{kind:'provider_pending'}>{return {kind:'provider_pending',snapshot:'',account:{identity:'11'.repeat(32),device,cert:'root-certified-device',name:'Pessoa',handle:'zoen_1111111111111111',relay:'https://relay.example',primary:device,primary_hpke:'22'.repeat(32),hpke_secret:'33'.repeat(32),link_secret:'',agreement_secret:'44'.repeat(32),profile_key:'55'.repeat(32),profile_key_version:1,profile_seen_version:1,profile_fields:''}};}

test('authority ciphertext is device and identity bound, and temporary plaintext is cleared',async()=>{
  const key=await crypto.subtle.generateKey({name:'AES-GCM',length:256},false,['encrypt','decrypt']);
  const seed=new Uint8Array(32).fill(7);const device='ab'.repeat(32);const identity='cd'.repeat(32);
  const sealed=await sealAuthority(seed,key,identity,device);assert.deepEqual(seed,new Uint8Array(32));
  assert.equal(key.extractable,false);const opened=await openAuthority(parseAuthority(JSON.parse(authorityJson(sealed))),key,device);
  assert.deepEqual(opened,new Uint8Array(32).fill(7));opened.fill(0);
  await assert.rejects(openAuthority({...sealed,identity:'ef'.repeat(32)},key,device));
  await assert.rejects(openAuthority(sealed,key,'ef'.repeat(32)),/outro dispositivo/);
  await assert.rejects(crypto.subtle.exportKey('raw',key));
});

test('pending account and protected authority commit atomically and cannot overwrite an existing root',async(t)=>{
  storage(t);const vault=await BrowserVault.open();const device=await vault.device();const session=pending(device);
  const sealed=await sealAuthority(new Uint8Array(32).fill(9),vault.authorityKey(),session.account.identity,device);
  await vault.preparePrimary(session,sealed);vault.close();
  const reopened=await BrowserVault.open();t.after(()=>reopened.close());
  assert.deepEqual(await reopened.load(),session);const authority=await reopened.authority();assert.ok(authority);
  assert.equal(authority.identity,session.account.identity);assert.deepEqual(await openAuthority(authority,reopened.authorityKey(),device),new Uint8Array(32).fill(9));
  await assert.rejects(reopened.preparePrimary({...session,account:{...session.account,name:'overwrite'}},sealed));
  assert.deepEqual(await reopened.load(),session);assert.equal((await reopened.authority())?.identity,session.account.identity);
});

test('cancelled primary preparation writes neither pending account nor authority',async(t)=>{
  storage(t);const vault=await BrowserVault.open();t.after(()=>vault.close());const device=await vault.device();const session=pending(device);
  const sealed=await sealAuthority(new Uint8Array(32).fill(8),vault.authorityKey(),session.account.identity,device);const abort=new AbortController();
  const writing=vault.preparePrimary(session,sealed,abort.signal);abort.abort();await assert.rejects(writing,{name:'AbortError'});
  assert.deepEqual(await vault.load(),{kind:'empty'});assert.equal(await vault.authority(),null);
});

test('an orphaned primary authority cannot silently generate replacement browser keys',async(t)=>{
  const factory=storage(t);const opening=factory.open('zoen-browser-v1',1);opening.onupgradeneeded=()=>opening.result.createObjectStore('vault');
  const db=await new Promise<IDBDatabase>((resolve,reject)=>{opening.onsuccess=()=>resolve(opening.result);opening.onerror=()=>reject(opening.error);});
  const transaction=db.transaction('vault','readwrite');const done=new Promise<void>((resolve,reject)=>{transaction.oncomplete=()=>resolve();transaction.onabort=()=>reject(transaction.error);});
  transaction.objectStore('vault').put({version:1,identity:'11'.repeat(32),device:'22'.repeat(32),iv:new Uint8Array(12),ciphertext:new Uint8Array(48)},'authority');await done;db.close();
  const generated=t.mock.method(crypto.subtle,'generateKey');await assert.rejects(BrowserVault.open(),/missing its protected keys/);assert.equal(generated.mock.calls.length,0);
});

test('erasure fences a provider account and authority still waiting for encryption',async(t)=>{
  storage(t);const vault=await BrowserVault.open();const device=await vault.device();const session=pending(device);
  const sealed=await sealAuthority(new Uint8Array(32).fill(6),vault.authorityKey(),session.account.identity,device);
  let resume!:()=>void;let started!:()=>void;
  const suspended=new Promise<void>(resolve=>{resume=resolve;});const entered=new Promise<void>(resolve=>{started=resolve;});
  const encrypt=crypto.subtle.encrypt.bind(crypto.subtle);
  t.mock.method(crypto.subtle,'encrypt',async(...args:Parameters<SubtleCrypto['encrypt']>)=>{started();await suspended;return encrypt(...args);});
  const preparing=vault.preparePrimary(session,sealed);const refused=assert.rejects(preparing,/vault is closed/);
  await entered;await vault.erase();resume();await refused;
  const reopened=await BrowserVault.open();t.after(()=>reopened.close());
  assert.notEqual(await reopened.device(),device);assert.equal(await reopened.authority(),null);assert.deepEqual(await reopened.load(),{kind:'empty'});
});
