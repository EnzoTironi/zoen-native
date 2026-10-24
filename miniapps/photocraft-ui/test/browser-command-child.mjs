/** Actual renderer ABI fixture. Supplied scope/ACK are synthetic, never Core
 * persistence, a grant, certified-agent admission or an approval UI journey. */
export function childHtml(seedBase64, seedHash, version = "1") {
  return `<!doctype html><html><head><meta charset="utf-8"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden}canvas{position:absolute;inset:0;width:100%;height:100%}
</style></head><body><canvas id="photocraft_canvas"></canvas><script>
(async()=>{
 const engine=await window.__zoenRuntime.load();
 const scope={artifactId:'browser-component-photo',actorId:'fixture-human',actorKind:'human'};
 const from64=s=>Uint8Array.from(atob(s),c=>c.charCodeAt(0));
 const to64=b=>{let s='';for(let at=0;at<b.length;at+=32768)s+=String.fromCharCode(...b.subarray(at,at+32768));return btoa(s);};
 const counts={};
 const call=async(op,input,bytes=new Uint8Array())=>{
  counts[op]=(counts[op]??0)+1;
  const r=await engine.zoenHostRequest(JSON.stringify(input===undefined?{op}:{op,input}),bytes);
  return {metadata:JSON.parse(r.metadata),bytes:r.bytes};
 };
 let head={version:${JSON.stringify(version)},sha256:${JSON.stringify(seedHash)}},pending;
 await call('layout',{width:innerWidth,height:innerHeight,insets:{top:0,right:0,bottom:0,left:0},closeButton:null,theme:'light'});
 await call('open',{scope,head},from64(${JSON.stringify(seedBase64)}));
 const changes=[];engine.zoenHostOnChange(s=>changes.push(JSON.parse(s)));
 window.probe={ready:true,changes,counts,
  status:async()=> (await call('status')).metadata,
  command:async(id,action)=>{pending=await call('command',{mutation:{scope,base:head,mutationId:id},action});return {metadata:pending.metadata,data:to64(pending.bytes)};},
  snapshot:async(id)=>{pending=await call('snapshot',{scope,base:head,mutationId:id});return {metadata:pending.metadata,data:to64(pending.bytes)};},
  wrongAccept:async()=>call('accept',{scope,mutationId:pending.metadata.mutationId,head:{version:String(BigInt(head.version)+1n),sha256:'0'.repeat(64)}}),
  accept:async()=>{const next={version:String(BigInt(head.version)+1n),sha256:pending.metadata.sha256};const r=await call('accept',{scope,mutationId:pending.metadata.mutationId,head:next});head=next;return r.metadata;},
  export:async(format='pcraft')=>{const r=await call('export',{scope,head,format});return {metadata:r.metadata,data:to64(r.bytes)};},
  foreign:()=>call('command',{mutation:{scope:{...scope,actorId:'foreign'},base:head,mutationId:'foreign'},action:'rotate_180'}),
  stale:()=>call('command',{mutation:{scope,base:{version:'1',sha256:${JSON.stringify(seedHash)}},mutationId:'stale'},action:'rotate_180'}),
  retire:()=>engine.zoenHostRetire()
 };
})().catch(e=>{window.probe={failed:String(e)};});
</script></body></html>`;
}
