/** Trusted touch regression against the actual WASM, in its own synthetic document.
 * This proves browser hit-testing, not an iPhone keyboard or Core durability. */
import assert from "node:assert/strict";
import { writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

// Reuse the bounded ZIP/CRC/XML reader; resolve shared strings only for this
// text-edit fixture, without changing the existing numeric reader's contract.
function textCells(path) {
  const reader = fileURLToPath(new URL("./", import.meta.url));
  return JSON.parse(
    execFileSync(
      "python3",
      [
        "-c",
        `
import sys,json
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from xlsx_reader import inspect_file,package,xml,MAIN,require
path=sys.argv[2];result=inspect_file(path);parts=package(Path(path).read_bytes())
root=xml(parts['xl/sharedStrings.xml'])
strings=[''.join(n.itertext()) for n in root.findall('{'+MAIN+'}si')]
cells={}
for key,cell in result['cells'].items():
    value=cell['value']
    if cell['type']=='s':
        require(value.isascii() and value.isdecimal(),'shared string index')
        index=int(value);require(index<len(strings),'shared string bound');value=strings[index]
    cells[key]={'value':value,'formula':cell['formula']}
print(json.dumps(cells))
`,
        reader,
        path,
      ],
      { encoding: "utf8", timeout: 10000 },
    ),
  );
}

export async function verifyRetiredAgent(browser, url, output) {
  const context = await browser.newContext({
    viewport: { width: 402, height: 874 },
    hasTouch: true,
    isMobile: true,
  });
  const errors = [];
  context.on("page", (page) => page.on("pageerror", (error) => errors.push(String(error))));
  try {
    await context.addInitScript(() => {
      const rows = [];
      window.retiredAgentRows = rows;
      for (const type of [
        "pointerdown",
        "pointerup",
        "touchstart",
        "touchend",
        "compositionstart",
        "compositionend",
        "input",
      ])
        document.addEventListener(
          type,
          (event) =>
            rows.push({
              type,
              target: event.target.tagName,
              trusted: event.isTrusted,
              composing: event.isComposing,
            }),
          true,
        );
    });
    const page = await context.newPage();
    await page.goto(url, { waitUntil: "load" });
    const frame = page.frames().find((candidate) => candidate !== page.mainFrame());
    assert.ok(frame);
    await frame.waitForFunction(() => window.probe?.ready || window.probe?.failed, undefined, {
      timeout: 18000,
    });
    assert.equal(await frame.evaluate(() => window.probe.failed), undefined);
    const frames = () =>
      frame.evaluate(
        () => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))),
      );
    const state = () =>
      frame.evaluate(() => {
        const input = document.querySelector("input"),
          box = input.getBoundingClientRect();
        return {
          x: box.x + box.width / 2,
          y: box.y + box.height / 2,
          pointerEvents: getComputedStyle(input).pointerEvents,
          hit: document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2)?.tagName,
          active: document.activeElement?.tagName,
          value: input.value,
          start: input.selectionStart,
          end: input.selectionEnd,
        };
      });
    const tap = async (x, y) => {
      await page.touchscreen.tap(x, y);
      await frames();
    };
    await frames();
    await tap(53, 185);
    await tap(53, 185);
    assert.equal((await state()).active, "INPUT");
    assert.equal((await state()).hit, "INPUT");
    await page.keyboard.insertText("QA345-P485");
    await frames();
    await page.keyboard.press("Enter");
    await frames();
    const retired = await state();
    assert.equal(retired.hit, "CANVAS");
    await page.setViewportSize({ width: 874, height: 402 });
    await frame.evaluate(() => window.probe.layout(874, 402));
    await frames();
    const rotated = await state();
    assert.equal(
      rotated.hit,
      "CANVAS",
      "old caret footprint cannot obstruct the neighboring cell after resize",
    );
    await frame.evaluate(() => {
      window.retiredAgentRows.length = 0;
    });
    await tap(rotated.x, rotated.y);
    await tap(rotated.x, rotated.y);
    const active = await state();
    assert.equal(active.active, "INPUT");
    assert.equal(active.hit, "INPUT", "the current editor regains native caret interaction");
    const touches = await frame.evaluate(() =>
      window.retiredAgentRows.filter((row) => row.type === "touchstart" || row.type === "touchend"),
    );
    assert.equal(touches.length, 4);
    assert.ok(touches.every((row) => row.trusted && row.target === "CANVAS"));
    // Actual IME protocol events exercise the same native input/composition handlers.
    const cdp = await context.newCDPSession(page);
    await cdp.send("Input.imeSetComposition", { text: "あ", selectionStart: 1, selectionEnd: 1 });
    await frames();
    assert.equal((await state()).hit, "INPUT");
    await cdp.send("Input.imeSetComposition", { text: "", selectionStart: 0, selectionEnd: 0 });
    await cdp.send("Input.insertText", { text: "あ" });
    await frames();
    const composed = await state();
    assert.equal(composed.value, "あ");
    assert.equal(composed.start, 1);
    assert.equal(composed.end, 1);
    const composition = await frame.evaluate(() =>
      window.retiredAgentRows.filter((row) => row.type.startsWith("composition")),
    );
    assert.ok(composition.some((row) => row.type === "compositionstart" && row.trusted));
    await writeFile(
      resolve(output, "composition-observation.json"),
      JSON.stringify(
        { composed, composition, rows: await frame.evaluate(() => window.retiredAgentRows) },
        null,
        2,
      ) + "\n",
    );
    // Chrome's CDP empty-composition command reports an untrusted end here;
    // retain that observation instead of claiming a native IME-end event.
    assert.ok(composition.some((row) => row.type === "compositionend" && row.target === "INPUT"));
    const composingInputs = await frame.evaluate(() =>
      window.retiredAgentRows.filter((row) => row.type === "input" && row.composing),
    );
    assert.ok(
      composingInputs.length > 0 &&
        composingInputs.every((row) => row.trusted && row.target === "INPUT"),
    );
    await page.screenshot({ path: resolve(output, "retired-agent-b1.png") });
    await page.keyboard.press("Enter");
    await frames();
    assert.equal((await state()).hit, "CANVAS");
    const committed = await frame.evaluate(() =>
      window.probe.snapshot("retired-agent-before-undo"),
    );
    const committedPath = resolve(output, "retired-agent-committed.xlsx");
    await writeFile(committedPath, Buffer.from(committed.data, "base64"));
    const committedCells = textCells(committedPath);
    assert.equal(committedCells.A1?.value, "QA345-P485");
    assert.equal(committedCells.B1?.value, "あ", "the new editor belongs to B1, not the old A1/A2");
    assert.ok(!committedCells.A2?.value);
    await frame.evaluate(() => window.probe.accept());
    await frame.evaluate(() => window.probe.freeze(false));
    await page.keyboard.press("Meta+z");
    await frames();
    const undone = await frame.evaluate(() => window.probe.snapshot("retired-agent-undo"));
    const undoPath = resolve(output, "retired-agent-undo.xlsx");
    await writeFile(undoPath, Buffer.from(undone.data, "base64"));
    const undoCells = textCells(undoPath);
    assert.equal(undoCells.A1?.value, "QA345-P485");
    assert.ok(!undoCells.B1?.value, "normal Undo removes only the last B1 edit");
    await frame.evaluate(() => window.probe.accept());
    await frame.evaluate(() => window.probe.freeze(false));
    await page.keyboard.press("Meta+Shift+z");
    await frames();
    await tap(10, 185); // Row selector is a control, not a text editor.
    assert.equal((await state()).active, "CANVAS");
    assert.equal((await state()).hit, "CANVAS");
    await cdp.send("Input.dispatchTouchEvent", {
      type: "touchStart",
      touchPoints: [{ x: 53, y: 185, id: 71 }],
    });
    await cdp.send("Input.dispatchTouchEvent", { type: "touchCancel", touchPoints: [] });
    await frames();
    assert.equal((await state()).active, "CANVAS", "cancelled contact cannot activate a keyboard");
    assert.equal((await state()).hit, "CANVAS");
    await frame.evaluate(() => window.probe.freeze(true));
    await tap(53, 185);
    await tap(53, 185);
    assert.equal((await state()).active, "CANVAS", "a frozen document cannot admit an editor");
    assert.equal((await state()).hit, "CANVAS");
    const snapshot = await frame.evaluate(() => window.probe.snapshot("retired-agent"));
    const path = resolve(output, "retired-agent.xlsx");
    await writeFile(path, Buffer.from(snapshot.data, "base64"));
    const cells = textCells(path);
    assert.equal(cells.A1?.value, "QA345-P485");
    assert.equal(cells.B1?.value, "あ", "the new editor belongs to B1, not the old A1/A2");
    assert.ok(!cells.A2?.value);
    assert.deepEqual(errors, []);
    return {
      retired,
      rotated,
      active,
      composed,
      touches,
      composition,
      cells,
      committedCells,
      undoCells,
      errors,
      scope: "actual trusted browser touches/IME; no device or Core acceptance",
    };
  } finally {
    await context.close();
  }
}
