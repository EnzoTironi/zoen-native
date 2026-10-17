import {object,string} from './boundary.ts';
import {authorityJson,parseAuthority} from './authority.ts';
import {parseAccount,relayUrl,type StoredSession} from './session.ts';
import type {BrowserVault} from './vault.ts';
import type {CoreWorker} from './worker-client.ts';

type Host={
  vault:Pick<BrowserVault,'load'|'authority'|'device'|'sign'|'preparePrimary'|'save'>;
  core:Pick<CoreWorker,'text'|'bytes'>;
  relay:string;proof:string;name:string;signal:AbortSignal;
  pending(session:Extract<StoredSession,{kind:'provider_pending'}>):void;
  fetch?:typeof fetch;
};

async function confirmationBody(response:Response):Promise<unknown>{
  if(!response.body)throw new Error('Resposta de entrada vazia.');
  const reader=response.body.getReader();const chunks:Uint8Array[]=[];let length=0;
  try{while(true){const {done,value}=await reader.read();if(done)break;length+=value.length;if(length>8192)throw new Error('Resposta de entrada inválida.');chunks.push(value);}}
  catch(error){await reader.cancel().catch(()=>{});throw error;}finally{reader.releaseLock();}
  const bytes=new Uint8Array(length);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
}

/** A provider proof never owns identity keys. Provision independently, persist both
 * the root authority and pending certified device atomically, then bind with the
 * existing relay protocol. An interrupted bind always retries the same identity.
 */
export async function enrollPrimary(host:Host):Promise<Extract<StoredSession,{kind:'linked'}>>{
  host.signal.throwIfAborted();const relay=relayUrl(host.relay);
  if(!/^[0-9a-f]{64}$/i.test(host.proof))throw new Error('Confirme sua entrada com Apple ou Google novamente.');
  const stored=await host.vault.load();host.signal.throwIfAborted();
  if(stored.kind==='linked')throw new Error('Este navegador já está conectado.');
  const device=await host.vault.device();const authority=await host.vault.authority();host.signal.throwIfAborted();
  let pending:Extract<StoredSession,{kind:'provider_pending'}>;
  if(stored.kind==='provider_pending'){
    if(relayUrl(stored.account.relay)!==relay||!authority)throw new Error('Recupere as chaves desta conta antes de continuar.');
    const account=parseAccount(await host.core.text({kind:'primary_restore',device,account:JSON.stringify(stored.account),authority:authorityJson(authority)}));
    host.signal.throwIfAborted();pending={...stored,account};
  }else{
    if(authority)throw new Error('Este navegador já guarda uma conta. Recupere o acesso antes de criar outra.');
    const prepared=object(JSON.parse(await host.core.text({kind:'primary_prepare',device,relay,name:host.name})));
    const account=parseAccount(JSON.stringify(prepared.account));const sealed=parseAuthority(prepared.authority);
    if(account.device!==device||account.relay!==relay||sealed.identity!==account.identity||sealed.device!==device)throw new Error('A conta mudou durante a entrada.');
    pending={kind:'provider_pending',account,snapshot:''};
    await host.vault.preparePrimary(pending,sealed,host.signal);host.signal.throwIfAborted();
  }
  host.pending(pending);host.signal.throwIfAborted();
  const payload=await host.core.bytes({kind:'primary_bind_message',proof:host.proof});host.signal.throwIfAborted();
  const signature=await host.vault.sign(payload);host.signal.throwIfAborted();
  const body=await host.core.text({kind:'primary_finish_bind',proof:host.proof,signature});host.signal.throwIfAborted();
  const request=object(JSON.parse(body));const profile=object(request.profile);
  if(profile.id!==pending.account.identity||request.device!==device||request.certificate!==pending.account.cert)throw new Error('A conta mudou durante a entrada.');
  let response:Response;
  try{response=await (host.fetch??fetch)(`${relay}/v1/auth/bind`,{method:'POST',redirect:'error',credentials:'omit',cache:'no-store',
    signal:AbortSignal.any([host.signal,AbortSignal.timeout(15_000)]),headers:{'content-type':'application/json'},body});}
  catch{host.signal.throwIfAborted();throw new Error('Não foi possível concluir a entrada. Suas chaves foram preservadas; tente novamente.');}
  host.signal.throwIfAborted();
  if(!response.ok)throw new Error(response.status===409?'Esta conta já existe. Recupere o acesso pelo seu outro aparelho ou backup.':'Confirme sua entrada com Apple ou Google novamente. Suas chaves foram preservadas.');
  const confirmed=object(await confirmationBody(response));host.signal.throwIfAborted();
  if(string(confirmed.identity_id)!==pending.account.identity||string(confirmed.handle)!==pending.account.handle)throw new Error('O servidor confirmou outra conta. Suas chaves foram preservadas.');
  const linked:Extract<StoredSession,{kind:'linked'}>={kind:'linked',account:pending.account,snapshot:pending.snapshot};
  await host.vault.save(linked,host.signal);host.signal.throwIfAborted();return linked;
}
