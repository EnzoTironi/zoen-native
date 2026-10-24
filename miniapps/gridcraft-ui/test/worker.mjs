import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { isAbsolute, resolve } from 'node:path';
const [binary, source, output] = process.argv.slice(2);
if (![binary,source,output].every(p=>p&&isAbsolute(p))) throw Error('Usage: worker.mjs ABS_ACTUAL_NATIVE_WORKER ABS_ACTUAL_GRID7_XLSX ABS_OUTPUT');
await mkdir(output,{recursive:true});
const sha=b=>createHash('sha256').update(b).digest('hex');
const bytes=await readFile(source);
const scope={artifactId:'synthetic-grid-worker',actorId:'synthetic-agent',actorKind:'agent'};
const open={scope,head:{version:'1',sha256:sha(bytes)}};
const calls=[];
function run(metadata,document=bytes) {
  const encoded=Buffer.from(JSON.stringify(metadata));const prefix=Buffer.alloc(4);prefix.writeUInt32LE(encoded.length);
  const raw=execFileSync(binary,[],{input:Buffer.concat([prefix,encoded,document]),timeout:10000,maxBuffer:2*1024*1024,
    env:{PATH:process.env.PATH,LANG:'en_US.UTF-8'}});
  assert.ok(raw.length>=4);const size=raw.readUInt32LE();assert.ok(size>0&&size<=65536&&raw.length>=4+size);
  const reply=JSON.parse(raw.subarray(4,4+size));const body=raw.subarray(4+size);calls.push({metadata,reply,bytes:body.length,sha256:sha(body)});
  return {reply,body};
}
const first=run({schema:1,open,command:null});
assert.equal(first.reply.ok,true);assert.equal(first.reply.metadata.cell.value,7);
assert.deepEqual(first.reply.metadata.query,{sheet:'Sheet1',cell:'A1'});assert.equal(first.body.length,0);
const explicit=run({schema:1,open,command:null,read:{sheet:'Sheet1',cell:'B1'}});
assert.equal(explicit.reply.ok,true);assert.equal(explicit.reply.metadata.cell.cell,'B1');assert.equal(explicit.reply.metadata.cell.type,'empty');
const action={command:'cell.set',params:{sheet:'Sheet1',cell:'B1',input:'=A1*2'}};
const edit=run({schema:1,open,command:{mutation:{scope,base:open.head,mutationId:'worker-formula'},action}});
assert.equal(edit.reply.ok,true);assert.ok(edit.body.length>0);assert.equal(edit.reply.metadata.sha256,sha(edit.body));assert.deepEqual(edit.reply.metadata.operation.action,action);
const reopened=run({schema:1,open:{scope,head:{version:'2',sha256:sha(edit.body)}},command:null,read:{sheet:'Sheet1',cell:'B1'}},edit.body);
assert.equal(reopened.reply.ok,true);assert.equal(reopened.reply.metadata.cell.value,14);assert.equal(reopened.reply.metadata.cell.formula,'=A1*2');
for (const read of [{sheet:'Sheet1',cell:'XFD1'},{sheet:'missing',cell:'A1'},{sheet:'Sheet1',cell:'A1',path:'/tmp/forged'}]) {
  const refused=run({schema:1,open,command:null,read});assert.equal(refused.reply.ok,false);assert.equal(refused.body.length,0);
}
const mixed=run({schema:1,open,command:{mutation:{scope,base:open.head,mutationId:'mixed'},action},read:{sheet:'Sheet1',cell:'A1'}});
assert.equal(mixed.reply.ok,false);assert.equal(mixed.body.length,0);
assert.equal(sha(await readFile(source)),sha(bytes));
await writeFile(resolve(output,'proposal.xlsx'),edit.body);
await writeFile(resolve(output,'result.json'),JSON.stringify({status:'PASS',scope:'Real native GridCraft worker/frame/inspection and calculation with synthetic supplied Scope; not certified-agent admission or WASI execution.',binarySha256:sha(await readFile(binary)),sourceSha256:sha(bytes),calls},null,2)+'\n');
