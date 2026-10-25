// Actual upstream renderer/worker and public MCP transport. Canonical CAS here
// is synthetic; authorization, durable persistence and iPhone need other proofs.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import { isAbsolute, resolve } from "node:path";
import { openManagedBrowser, sha } from "../../shared-tests/managed-browser.mjs";

const [modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath, worker] = process.argv.slice(2);
assert.ok(worker && isAbsolute(worker), "Supply the actual qualified upstream worker executable");
const host = await openManagedBrowser({
  editor: "pdfcraft", modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath,
});
let result = { status: "FAIL" };
try {
  const page = host.page;
  const child = page.frames().find((frame) => frame.parentFrame());
  assert.ok(child);
  const frames = () => child.evaluate(async () => {
    for (let n = 0; n < 6; n++) await new Promise((done) => requestAnimationFrame(done));
  });
  const exported = async (name) => {
    await page.evaluate(() => window.probe.share("pdf"));
    const value = await page.evaluate(() => ({
      ...window.probe.exports.at(-1), version: window.probe.version, writes: window.probe.writes,
    }));
    assert.equal(value.format, "pdf");
    const bytes = Buffer.from(value.data, "base64");
    assert.equal(sha(bytes), value.sha256);
    await writeFile(resolve(output, `${name}.pdf`), bytes);
    return value;
  };
  const baseline = await exported("baseline");
  assert.equal(baseline.data, host.seed.toString("base64"));
  await page.touchscreen.tap(28, 24);
  await frames();
  await page.touchscreen.tap(124, 106);
  await child.waitForFunction(() => document.activeElement?.tagName === "INPUT", undefined,
    { timeout: 3000 });
  await page.keyboard.type(" browser live review");
  await page.evaluate(() => window.probe.flush());
  const human = await exported("human");
  assert.equal(human.version, 2);
  assert.equal(human.writes, 1);

  // Generate an external proposal from the EXACT human-accepted bytes/head,
  // through the real worker stdio ABI, rather than building a replacement PDF.
  const scope = { artifactId: "bootstrap-fixture", actorId: "fixture-agent", actorKind: "agent" };
  const base = { version: String(human.version), sha256: human.sha256 };
  const action = { op: "rotate_pages", pages: [1], degrees: 90 };
  const request = Buffer.from(JSON.stringify({ schema: 1, open: { scope, head: base },
    command: { mutation: { scope, base, mutationId: "external-pdf-rotation" }, action } }));
  const prefix = Buffer.alloc(4);
  prefix.writeUInt32LE(request.length);
  const reply = execFileSync(worker, [], {
    input: Buffer.concat([prefix, request, Buffer.from(human.data, "base64")]),
    timeout: 30000, maxBuffer: 9 * 1024 * 1024,
  });
  const length = reply.readUInt32LE();
  assert.ok(length > 0 && length <= 65536 && reply.length > length + 4);
  const envelope = JSON.parse(reply.subarray(4, length + 4).toString());
  const proposal = reply.subarray(length + 4);
  assert.equal(envelope.ok, true);
  assert.equal(envelope.metadata.sha256, sha(proposal));
  assert.equal(envelope.metadata.byteLength, proposal.length);
  assert.deepEqual(envelope.metadata.scope, scope);
  assert.deepEqual(envelope.metadata.base, base);
  assert.deepEqual(envelope.metadata.operation, { kind: "command", action });
  await writeFile(resolve(output, "proposal.pdf"), proposal);
  await writeFile(resolve(output, "worker-metadata.json"), JSON.stringify(envelope, null, 2) + "\n");
  await page.evaluate((data) => window.probe.receiveCanonical(data, 3), proposal.toString("base64"));
  assert.deepEqual(await page.evaluate(() => window.probe.observedHead()), {
    itemId: "bootstrap-fixture", version: 3, sha256: sha(proposal),
  });
  const external = await exported("external");
  assert.equal(external.data, proposal.toString("base64"));
  assert.equal(external.writes, 1, "reviewing a canonical head must not save it again");
  await page.screenshot({ path: resolve(output, "external.png") });

  // The real upstream toolbar must retain BOTH the received version and the
  // earlier human edit in its native Undo stack, each Undo becoming a new CAS.
  await page.touchscreen.tap(166, 24);
  await frames();
  await page.evaluate(() => window.probe.flush());
  const undoExternal = await exported("undo-external");
  assert.equal(undoExternal.version, 4);
  assert.equal(undoExternal.writes, 2);
  await page.touchscreen.tap(166, 24);
  await frames();
  await page.evaluate(() => window.probe.flush());
  const undoHuman = await exported("undo-human");
  assert.equal(undoHuman.version, 5);
  assert.equal(undoHuman.writes, 3);
  await page.screenshot({ path: resolve(output, "undo-human.png") });
  await page.evaluate(() => window.probe.close());
  await host.reopen(undoHuman.data, 5);
  const reopened = await exported("reopened");
  assert.equal(reopened.data, undoHuman.data);
  assert.equal(reopened.version, 5);
  assert.equal(reopened.writes, 0);
  await page.evaluate(() => window.probe.retire());
  await assert.rejects(page.evaluate(() => window.probe.share("pdf")));
  assert.deepEqual(host.errors, []);
  assert.deepEqual(host.consoleErrors, []);
  result = {
    status: "PASS", ...host.inputs, workerSha256: sha(await readFile(worker)),
    proof: "Real trusted touch edit, external upstream worker proposal, same rendered editor review and two native Undo saves; exact cold chosen-version export. Synthetic canonical CAS only; no Core authorization or iPhone claim.",
    exported: Object.fromEntries(Object.entries({ baseline, human, external, undoExternal, undoHuman, reopened })
      .map(([name, value]) => [name, { version: value.version, writes: value.writes,
        sha256: value.sha256, bytes: Buffer.from(value.data, "base64").length }])),
  };
} catch (error) {
  result.error = String(error);
  await host.page.screenshot({ path: resolve(output, "failure.png") }).catch(() => {});
  throw error;
} finally {
  await host.close();
  await writeFile(resolve(output, "result.json"), JSON.stringify({ ...result,
    requests: host.requests, errors: host.errors, consoleErrors: host.consoleErrors,
    cleanupErrors: host.cleanupErrors }, null, 2) + "\n");
}
