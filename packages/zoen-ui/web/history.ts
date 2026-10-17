import { integer, object, string } from './boundary.ts';
import type { CoreOperation } from './worker-client.ts';

export type HistoryResult = {messages:number;chunks:number;resumed:number};
type Operation = Extract<CoreOperation,{kind:'history_start'|'history_finish'|'history_cleaned'|'history_manifest'|'history_chunk'}>;
type Host = {
  core(operation:Operation):Promise<unknown>;
  rpc(kind:string,value:string):Promise<unknown>;
  base:string;
  signal:AbortSignal;
  valid():boolean;
  fetch?:typeof fetch;
};

/** Stream before allocating: a hostile Content-Length is never a memory budget. */
export async function boundedHistoryBody(response:Response,max:number):Promise<Uint8Array> {
  if(!response.ok)throw new Error('O histórico aguarda o celular terminar de enviar.');
  const declared=response.headers.get('content-length');
  if(declared!==null&&(!/^\d+$/.test(declared)||BigInt(declared)>BigInt(max)))throw new Error('Pacote de histórico grande demais.');
  if(!response.body)throw new Error('Pacote de histórico vazio.');
  const reader=response.body.getReader();const chunks:Uint8Array[]=[];let length=0;
  try {while(true){const {done,value}=await reader.read();if(done)break;length+=value.byteLength;if(length>max)throw new Error('Pacote de histórico grande demais.');chunks.push(value);}}
  catch(error){await reader.cancel().catch(()=>{});throw error;}
  finally{reader.releaseLock();}
  const result=new Uint8Array(length);let offset=0;for(const chunk of chunks){result.set(chunk,offset);offset+=chunk.length;}return result;
}

/** One bounded work slice. Each core callback must persist its Update before return.
 * Network requests run outside the message serial queue, so SyncDone can arrive.
 */
export async function historySlice(host:Host):Promise<HistoryResult|null> {
  const current=()=>{if(!host.valid()||host.signal.aborted)throw new Error('A sessão mudou durante a restauração do histórico.');};
  const fetcher=host.fetch??fetch;
  current();let work=object(await host.core({kind:'history_start',now:Date.now()}));
  for(let count=0;count<4;count++){
    current();
    switch(string(work.kind)){
      case 'fetch_manifest':{
        const box=string(work.box_id);if(!/^[0-9a-f]{64}$/.test(box))throw new Error('Manifesto de histórico inválido.');
        const response=object(await host.rpc('fetch_link',box));current();
        if(response.kind!=='link')throw new Error('O servidor não retornou o histórico.');
        if(response.sealed===null)return null;
        const values=response.sealed;
        if(!Array.isArray(values)||values.length<48||values.length>(64<<10)||values.some(value=>!Number.isInteger(value)||value<0||value>255))throw new Error('Manifesto de histórico inválido.');
        work=object(await host.core({kind:'history_manifest',sealed:Uint8Array.from(values),now:Date.now()}));break;
      }
      case 'download':{
        const transfer=string(work.transfer);const index=integer(work.index);
        if(!/^[0-9a-f]{64}$/.test(transfer))throw new Error('Transferência de histórico inválida.');
        const response=await fetcher(`${host.base}/v1/transfer/${transfer}/${index}`,{signal:host.signal});
        const sealed=await boundedHistoryBody(response,(1<<20)+16);current();
        work=object(await host.core({kind:'history_chunk',index,sealed,now:Date.now()}));break;
      }
      case 'waiting_sync':return null;
      case 'complete':{
        const result={messages:integer(work.messages),chunks:integer(work.chunks),resumed:integer(work.resumed)};
        if(work.cleanup!==null){
          const cleanup=object(work.cleanup);const transfer=string(cleanup.transfer);
          if(!/^[0-9a-f]{64}$/.test(transfer))throw new Error('Limpeza de histórico inválida.');
          current();
          const response=await fetcher(`${host.base}/v1/transfer/${transfer}`,{method:'DELETE',signal:host.signal,headers:{'x-zoen-device':string(cleanup.device),'x-zoen-ts':string(cleanup.timestamp),'x-zoen-sig':string(cleanup.signature)}});
          if(!response.ok&&response.status!==404)throw new Error('O histórico está salvo; a limpeza será retomada ao conectar.');
          current();await host.core({kind:'history_cleaned',now:Date.now()});
        }
        return result;
      }
      default:throw new Error('Etapa de histórico desconhecida.');
    }
  }
  return null;
}
