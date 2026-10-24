import assert from "node:assert/strict";

/** Request the real public pause in the final trusted touch's bubble phase.
 * No synthetic event, forced focus, document mutation or animation-frame wait. */
export async function pauseOnCompletedTouch(page, x, y) {
  const child = page.frames().find((frame) => frame.parentFrame());
  assert.ok(child);
  await child.evaluate(async () => {
    const engine = await window.__zoenRuntime.load();
    window.touchPause = { events: [] };
    document.addEventListener(
      "touchend",
      (event) => {
        window.touchPause.events.push({
          trusted: event.isTrusted,
          remaining: event.touches.length,
        });
        if (event.isTrusted && event.touches.length === 0) {
          window.touchPause.promise = engine.zoenHostRequest(
            JSON.stringify({ op: "freeze", input: { frozen: true } }),
            new Uint8Array(),
          );
        }
      },
      { once: true },
    );
  });
  await page.touchscreen.tap(x, y);
  const observed = await child.evaluate(async () => {
    if (!window.touchPause.promise) throw Error("completed trusted touch did not request pause");
    const result = await window.touchPause.promise;
    return {
      events: window.touchPause.events,
      status: JSON.parse(result.metadata),
      activeElement: document.activeElement?.tagName,
    };
  });
  assert.deepEqual(observed.events, [{ trusted: true, remaining: 0 }]);
  assert.equal(observed.status.frozen, true);
  assert.equal(observed.status.dirty, true, "pause must include the final real stroke");
  await child.evaluate(async () => {
    const engine = await window.__zoenRuntime.load();
    await engine.zoenHostRequest(
      JSON.stringify({ op: "freeze", input: { frozen: false } }),
      new Uint8Array(),
    );
  });
  return observed;
}
