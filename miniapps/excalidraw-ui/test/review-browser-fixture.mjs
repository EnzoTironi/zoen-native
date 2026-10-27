/** Real official host/frame and upstream graphics; only persistence is an explicit test port. */
export function reviewHostScript(resource, scenes) {
  return `import {MiniappSession} from '../../apps/zoen/src/miniapps/protocol.ts';
const frame=document.createElement('iframe');frame.sandbox='allow-scripts';frame.style='width:420px;height:760px';
const scenes=${JSON.stringify(scenes)};
const bytes=new TextEncoder().encode(scenes.before), calls=[], statuses=[], errors=[];
const hex=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),n=>n.toString(16).padStart(2,'0')).join('');
const digest=await hex(bytes);let started=false;
const output=value=>{const pre=document.createElement('pre');pre.id='result';pre.textContent=JSON.stringify(value);document.body.append(pre);};
const inspect=async image=>{
 const blob=await(await fetch('data:image/png;base64,'+image.dataBase64)).blob();
 const bitmap=await createImageBitmap(blob), canvas=document.createElement('canvas');
 canvas.width=bitmap.width;canvas.height=bitmap.height;const ctx=canvas.getContext('2d');ctx.drawImage(bitmap,0,0);bitmap.close();
 const pixels=ctx.getImageData(0,0,canvas.width,canvas.height).data;let blue=0,red=0;
 for(let i=0;i<pixels.length;i+=4){if(pixels[i+2]>200&&pixels[i]<30&&pixels[i+1]<30)blue++;if(pixels[i]>200&&pixels[i+1]<30&&pixels[i+2]<30)red++;}
 return {width:canvas.width,height:canvas.height,blue,red,sha256:await hex(new Uint8Array(await blob.arrayBuffer()))};
};
let session;
const run=async()=>{
 await new Promise(r=>setTimeout(r,200));const count=statuses.length;
 const args={nonce:'pixels',artifactId:'item',requestId:'request',contentHash:'cd'.repeat(32),resourceUri:'ui://zoen/package/file/1/'+ 'ef'.repeat(32),baseVersion:1,
 beforeSceneJson:scenes.before,beforeSha256:digest,afterSceneJson:scenes.after,afterSha256:await hex(new TextEncoder().encode(scenes.after))};
 const result=await session.callAppTool('render_scene_review',args);
 if(result.isError)throw Error(JSON.stringify(result));const value=result.structuredContent;
 const empty=await session.callAppTool('render_scene_review',{...args,nonce:'empty',beforeSceneJson:scenes.empty,beforeSha256:await hex(new TextEncoder().encode(scenes.empty))});
 if(empty.isError)throw Error(JSON.stringify(empty));
 const metrics={before:await inspect(value.before),after:await inspect(value.after),empty:await inspect(empty.structuredContent.before)};
 for(const side of ['before','after']){const img=document.createElement('img');img.src='data:image/png;base64,'+value[side].dataBase64;img.style='width:280px';document.body.append(img);}
 output({metrics,images:{before:value.before,after:value.after},calls,statusesUnchanged:statuses.length===count,errors,renderer:value.renderer,nonce:value.nonce,resourceUri:value.resourceUri});
};
session=new MiniappSession({send:m=>frame.contentWindow.postMessage(m,'*'),
 initialize:()=>({hostInfo:{name:'Zoen drawing review QA',version:'645'},hostCapabilities:{serverTools:{},experimental:{zoen:{canonicalScene:{version:1,itemId:'item'}}}},hostContext:{displayMode:'fullscreen',locale:'en-US'}}),
 arguments:()=>({}),state:()=>({content:[]}),context:()=>({displayMode:'fullscreen',locale:'en-US'}),
 tools:new Set(['app_scene_read','app_scene_read_chunk','app_scene_begin','app_scene_append','app_scene_commit','app_scene_abort']),
 resourceUri:'ui://zoen/excalidraw.html',resource:async()=>({contents:[]}),
 call:async(name)=>{calls.push(name);if(name==='app_scene_read')return{content:[],structuredContent:{transferId:'read',version:1,currentVersion:1,bytes:bytes.length,sha256:digest,chunkBytes:196608}};if(name==='app_scene_read_chunk')return{content:[],structuredContent:{index:0,data:btoa(scenes.before)}};throw Error('Readonly review tried to save '+name);},
 updateModelContext:async params=>{statuses.push(params);const v=params.structuredContent?.zoen?.sceneSave;if(v?.ready&&v.reviewPreview===1&&!started){started=true;void run().catch(e=>output({error:String(e),calls,statuses,errors}));}return{};},
 openLink:async()=>({}),share:async()=>({}),native:async()=>({}),haptic(){},resize(){},close(){}});
addEventListener('message',e=>{if(e.source===frame.contentWindow)void session.receive(JSON.stringify(e.data)).catch(e=>errors.push(String(e)));});
frame.srcdoc=${JSON.stringify(resource)};document.body.append(frame);
setTimeout(()=>{if(!started)output({error:'No review capability',calls,statuses,errors});},8000);`;
}
