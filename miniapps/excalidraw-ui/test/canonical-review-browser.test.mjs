import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";
import { findBrowserExecutable, ownedBrowserResult } from "./browser-driver.mjs";
import { withBrowserProfile } from "./browser-lifecycle.mjs";
import { reviewHostScript } from "./review-browser-fixture.mjs";
import { sandboxHtml } from "../../../apps/zoen/src/miniapps/sandbox.ts";
import { drawingScene, sha } from "../../../apps/zoen/src/core/drawing-review.fixtures.ts";
const root = fileURLToPath(new URL("..", import.meta.url));

test("actual admitted MCP frame exports upstream blue/red/empty PNG pixels without mutating the open scene", async (t) => {
  const chrome = await findBrowserExecutable();
  if (!chrome) {
    t.skip("Chrome unavailable. This is separate from native WebKit acceptance.");
    return;
  }
  const folder = await mkdtemp(join(tmpdir(), "zoen-review-pixels-"));
  await withBrowserProfile(folder, async () => {
    const html = await readFile(
      process.env.ZOEN_DRAWING_REVIEW_HTML ??
        join(root, "../../crates/miniapps/resources/excalidraw.html"),
      "utf8",
    );
    const scenes = {
      before: drawingScene("#0000ff"),
      after: drawingScene("#ff0000"),
      empty: JSON.stringify({
        type: "excalidraw",
        version: 2,
        elements: [],
        appState: { viewBackgroundColor: "#ffffff" },
        files: {},
      }),
    };
    const host = reviewHostScript(sandboxHtml(html, []), scenes);
    const compiled = await build({
      absWorkingDir: root,
      stdin: { contents: host, resolveDir: root, loader: "js" },
      bundle: true,
      write: false,
      format: "esm",
      platform: "browser",
      define: { "process.env.NODE_ENV": '"production"' },
    });
    const path = join(folder, "review.html");
    await writeFile(
      path,
      `<!doctype html><body><script type="module">${compiled.outputFiles[0].text.replaceAll("</script", "<\\/script")}</script></body>`,
    );
    const browser = await ownedBrowserResult(chrome, folder, path),
      result = JSON.parse(browser.text);
    assert.deepEqual(browser.errors, []);
    assert.equal(result.error, undefined, result.error);
    assert.deepEqual(result.errors, []);
    assert.equal(result.renderer, "excalidraw@0.18.0");
    assert.equal(result.nonce, "pixels");
    assert.ok(result.metrics.before.blue > 1000);
    assert.equal(result.metrics.before.red, 0);
    assert.ok(result.metrics.after.red > 1000);
    assert.equal(result.metrics.after.blue, 0);
    assert.equal(result.metrics.empty.red + result.metrics.empty.blue, 0);
    assert.ok(result.metrics.empty.width > 0 && result.metrics.empty.height > 0);
    for (const side of ["before", "after"]) {
      const image = result.images[side],
        bytes = Buffer.from(image.dataBase64, "base64");
      assert.ok(bytes.length <= 131072);
      assert.equal(sha(bytes), image.imageSha256);
      assert.equal(image.imageSha256, result.metrics[side].sha256);
      assert.equal(image.sceneSha256, sha(scenes[side]));
      assert.ok(image.width <= 896 && image.height <= 896);
    }
    assert.notEqual(result.images.before.imageSha256, result.images.after.imageSha256);
    assert.deepEqual(result.calls, ["app_scene_read", "app_scene_read_chunk"]);
    assert.equal(
      result.statusesUnchanged,
      true,
      "readonly export must not change dirty/head/observed state",
    );
    const evidence = process.env.ZOEN_DRAWING_REVIEW_EVIDENCE;
    if (evidence) {
      assert.ok(evidence.startsWith("/tmp/zoen-drawing-review-v645-"));
      for (const side of ["before", "after"])
        await writeFile(
          join(evidence, side + ".png"),
          Buffer.from(result.images[side].dataBase64, "base64"),
        );
      await writeFile(join(evidence, "browser.png"), Buffer.from(browser.png, "base64"));
      await writeFile(
        join(evidence, "pixels.json"),
        JSON.stringify({ ...result, images: undefined, lifecycle: browser.lifecycle }, null, 2) +
          "\n",
      );
    }
  });
});
