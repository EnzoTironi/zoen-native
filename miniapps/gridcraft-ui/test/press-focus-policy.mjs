import assert from "node:assert/strict";

/** A test-only browser policy: canvas focus on press is refused, while a real
 * release may focus normally. This never focuses an element or changes events.
 * It is a fault model, not a claim about a particular native browser policy. */
export async function installPressFocusRefusal(page, canvasId) {
  assert.match(canvasId, /^[a-z][a-z0-9_]{0,63}$/);
  await page.addInitScript((id) => {
    const state = { phase: "idle", denied: 0, releaseAttempts: 0 };
    Object.defineProperty(window, "__zoenTestPressFocus", { value: state });
    const original = HTMLElement.prototype.focus;
    HTMLElement.prototype.focus = function (options) {
      if (this.id === id) {
        if (state.phase === "press") {
          state.denied++;
          return;
        }
        if (state.phase === "release") state.releaseAttempts++;
      }
      return original.call(this, options);
    };
    for (const type of [
      "pointerdown",
      "touchstart",
      "pointerup",
      "touchend",
      "pointercancel",
      "touchcancel",
    ])
      document.addEventListener(
        type,
        () => {
          state.phase = type.endsWith("cancel")
            ? "idle"
            : type === "pointerdown" || type === "touchstart"
              ? "press"
              : "release";
        },
        { capture: true, passive: true },
      );
  }, canvasId);
}

export async function observePressFocusRefusal(page) {
  const child = page.frames().find((frame) => frame.parentFrame());
  assert.ok(child, "real editor child frame must exist");
  const state = await child.evaluate(() => window.__zoenTestPressFocus);
  assert.ok(state?.denied > 0, "the real canvas must have experienced press-focus refusal");
  return state;
}
