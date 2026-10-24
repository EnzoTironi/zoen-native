/** Actual engine/browser component fixture. Receipts here are deliberate ABI
 * fixtures; this does not claim Core persistence, a grant, or agent admission. */
export function childHtml(seedBase64, seedHash) {
  return `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden}canvas{width:100%;height:100%;position:absolute;inset:0}
</style></head><body><canvas id="gridcraft_canvas"></canvas><script>
(async()=>{
 const engine=await window.__zoenRuntime.load();
 const scope={artifactId:'browser-component-workbook',actorId:'fixture-human',actorKind:'human'};
 const from64=s=>Uint8Array.from(atob(s),c=>c.charCodeAt(0));
 const to64=b=>{let s='';for(let at=0;at<b.length;at+=32768)s+=String.fromCharCode(...b.subarray(at,at+32768));return btoa(s);};
 const call=async(op,input,bytes=new Uint8Array())=>{
  const r=await engine.zoenHostRequest(JSON.stringify(input===undefined?{op}:{op,input}),bytes);
  return {metadata:JSON.parse(r.metadata),bytes:r.bytes};
 };
 let head={version:'1',sha256:${JSON.stringify(seedHash)}},pending;
 await call('layout',{width:innerWidth,height:innerHeight,insets:{top:0,right:0,bottom:0,left:0},closeButton:null,theme:'light'});
 await call('open',{scope,head},from64(${JSON.stringify(seedBase64)}));
 const changes=[];engine.zoenHostOnChange(s=>changes.push(JSON.parse(s)));
 window.probe={ready:true,changes,
  status:async()=> (await call('status')).metadata,
  freeze:async(frozen)=> (await call('freeze',{frozen})).metadata,
  layout:async(width,height)=> (await call('layout',{width,height,insets:{top:0,right:0,bottom:0,left:0},closeButton:null,theme:'light'})).metadata,
  snapshot:async(mutationId)=>{pending=await call('snapshot',{scope,base:head,mutationId});return {metadata:pending.metadata,data:to64(pending.bytes)};},
  command:async(mutationId,sheet,cell,input)=>{pending=await call('command',{mutation:{scope,base:head,mutationId},action:{command:'cell.set',params:{sheet,cell,input}}});return {metadata:pending.metadata,data:to64(pending.bytes)};},
  wrongAccept:async()=>call('accept',{scope,mutationId:pending.metadata.mutationId,head:{version:String(BigInt(head.version)+1n),sha256:'0'.repeat(64)}}),
  accept:async()=>{const next={version:String(BigInt(head.version)+1n),sha256:pending.metadata.sha256};const r=await call('accept',{scope,mutationId:pending.metadata.mutationId,head:next});head=next;return r.metadata;},
  export:async()=>{const p=await call('export',{scope,head,format:'xlsx'});return {metadata:p.metadata,data:to64(p.bytes)};},
  reopen:async(data,next)=>{await call('open',{scope,head:next},from64(data));head=next;return (await call('status')).metadata;},
  retire:()=>engine.zoenHostRetire(),
  request:call
 };
})().catch(e=>{window.probe={failed:String(e)};});
</script></body></html>`;
}
