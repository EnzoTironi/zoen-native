// Actual worker/frame regression; requires an already qualified native binary.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { isAbsolute, resolve } from 'node:path';

const [binary, output] = process.argv.slice(2);
if (![binary, output].every((value) => value && isAbsolute(value))) {
  throw Error('Usage: worker.mjs ABS_ACTUAL_NATIVE_WORKER ABS_OUTPUT');
}
await mkdir(output, { recursive: true });
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const execute = (input, args = []) => execFileSync(binary, args, {
  input, timeout: 10000, maxBuffer: 2 * 1024 * 1024,
  env: { PATH: process.env.PATH, LANG: 'en_US.UTF-8' },
});
const seed = execute(undefined, ['--seed']);
assert.equal(seed.subarray(0, 2).toString(), 'PK');
const scope = { artifactId: 'synthetic-word-worker', actorId: 'synthetic-owner', actorKind: 'agent' };
const open = { scope, head: { version: '1', sha256: sha(seed) } };
const calls = [];
function run(metadata, bytes = seed) {
  const encoded = Buffer.from(JSON.stringify(metadata));
  const prefix = Buffer.alloc(4); prefix.writeUInt32LE(encoded.length);
  const raw = execute(Buffer.concat([prefix, encoded, bytes]));
  assert.ok(raw.length >= 4);
  const size = raw.readUInt32LE();
  assert.ok(size > 0 && size <= 65536 && raw.length >= 4 + size);
  const reply = JSON.parse(raw.subarray(4, 4 + size));
  const body = raw.subarray(4 + size);
  calls.push({ reply, bytes: body.length, sha256: sha(body) });
  return { reply, body };
}
const initial = run({ schema: 1, open, command: null });
assert.equal(initial.reply.ok, true);
assert.equal(initial.reply.metadata.text, '');
assert.equal(initial.body.length, 0);
const text = 'Texto real\n日本語 👩🏽‍💻\tsegundo\n';
const action = { command: 'text.replace', params: { text } };
const command = { mutation: { scope, base: open.head, mutationId: 'actual-text-edit' }, action };
const changed = run({ schema: 1, open, command });
assert.equal(changed.reply.ok, true);
assert.ok(changed.body.length > 0);
assert.equal(changed.reply.metadata.sha256, sha(changed.body));
assert.equal(changed.reply.metadata.byteLength, changed.body.length);
assert.deepEqual(changed.reply.metadata.operation.action, action);
assert.equal(changed.reply.metadata.acceptedHead, null);
const coldHead = { version: '2', sha256: sha(changed.body) };
const cold = run({ schema: 1, open: { scope, head: coldHead }, command: null }, changed.body);
assert.equal(cold.reply.ok, true);
assert.equal(cold.reply.metadata.text, text);
assert.deepEqual(cold.reply.metadata.head, coldHead);
const retry = run({ schema: 1, open, command });
assert.equal(retry.reply.ok, true);
assert.deepEqual(retry.body, changed.body);
for (const request of [
  { schema: 1, open, command: { ...command, action: { command: 'file.open', params: { path: '/tmp/forbidden' } } } },
  { schema: 1, open: { scope, head: { version: '1', sha256: '0'.repeat(64) } }, command: null },
  { schema: 1, open, command: { ...command, mutation: { ...command.mutation, scope: { ...scope, actorId: 'foreign' } } } },
  { schema: 1, open, command: { ...command, mutation: { ...command.mutation, base: { ...open.head, version: '0' } } } },
]) {
  const refused = run(request);
  assert.equal(refused.reply.ok, false);
  assert.equal(refused.body.length, 0);
}
await writeFile(resolve(output, 'seed.docx'), seed);
await writeFile(resolve(output, 'proposal.docx'), changed.body);
await writeFile(resolve(output, 'result.json'), JSON.stringify({
  status: 'PASS', scope: 'Actual native WordCraft Session/worker/frame/plain DOCX cold-open and exact retry with synthetic supplied Scope. No certified-agent authorization, canonical persistence, WASI, browser or iPhone claim.',
  binarySha256: sha(await readFile(binary)), seedSha256: sha(seed), proposalSha256: sha(changed.body), calls,
}, null, 2) + '\n');
