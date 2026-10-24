// Read-only lifecycle qualification; text input and native iPhone acceptance are separate.
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { openManagedBrowser, sha } from "../../shared-tests/managed-browser.mjs";
const [modulePath, wasmPath, htmlPath, seedPath, output, playwrightPath] = process.argv.slice(2);
const host = await openManagedBrowser({
  editor: "pdfcraft",
  modulePath,
  wasmPath,
  htmlPath,
  seedPath,
  output,
  playwrightPath,
});
let result = { status: "FAIL" };
try {
  const { page, seed } = host;
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  assert.equal(await page.evaluate(() => window.probe.version), 1);
  await page.evaluate(() => window.probe.share("pdf"));
  const initial = await page.evaluate(() => window.probe.exports.at(-1));
  assert.equal(initial.format, "pdf");
  assert.equal(initial.data, seed.toString("base64"));
  assert.equal(initial.sha256, sha(seed));
  await writeFile(resolve(output, "baseline.pdf"), Buffer.from(initial.data, "base64"));
  await page.screenshot({ path: resolve(output, "initial.png") });
  for (const [name, size] of [
    ["landscape", { width: 874, height: 402 }],
    ["reduced-height", { width: 874, height: 220 }],
    ["portrait", { width: 402, height: 874 }],
  ]) {
    await page.setViewportSize(size);
    const child = page.frames().find((frame) => frame.parentFrame());
    assert.ok(child);
    await child.evaluate(
      () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
    );
    await page.screenshot({ path: resolve(output, `${name}.png`) });
    assert.equal(await page.evaluate(() => window.probe.writes), 0, "layout cannot mutate the PDF");
  }
  await page.evaluate(() => window.probe.close());
  await host.reopen(initial.data, 1);
  assert.equal(await page.evaluate(() => window.probe.writes), 0);
  await page.evaluate(() => window.probe.share("pdf"));
  const recovered = await page.evaluate(() => window.probe.exports.at(-1));
  assert.equal(recovered.data, initial.data);
  assert.equal(recovered.sha256, initial.sha256);
  await writeFile(resolve(output, "reopened.pdf"), Buffer.from(recovered.data, "base64"));
  await page.screenshot({ path: resolve(output, "reopened.png") });
  await page.evaluate(() => window.probe.retire());
  await assert.rejects(page.evaluate(() => window.probe.share("pdf")));
  assert.deepEqual(host.errors, []);
  assert.deepEqual(host.consoleErrors, []);
  result = {
    status: "PASS",
    ...host.inputs,
    proof:
      "Read-only real PDF bootstrap/engine/export, orientation, cold fresh-renderer reopen and retired-owner refusal. Synthetic Core port; no typing, Rust persistence, agent authorization or iPhone claim.",
    exactExportSha256: initial.sha256,
    writes: 0,
    coldVersion: 1,
  };
} catch (error) {
  result = { ...result, error: String(error) };
  throw error;
} finally {
  await host.close();
  await writeFile(
    resolve(output, "result.json"),
    JSON.stringify(
      {
        ...result,
        requests: host.requests,
        errors: host.errors,
        consoleErrors: host.consoleErrors,
        cleanupErrors: host.cleanupErrors,
      },
      null,
      2,
    ) + "\n",
  );
}
