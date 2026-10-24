import assert from "node:assert/strict";

// Fault model only: refuse canvas focus, while leaving the real input untouched.
// This also covers a canvas grant delayed beyond the eligible user callback.
// Never focus another element, change events, or infer an iPhone keyboard pass.
export async function installCanvasGestureFocusRefusal(page, canvasId) {
  assert.match(canvasId, /^[a-z][a-z0-9_]{0,63}$/);
  await page.addInitScript((id) => {
    const state = { denied: 0, calls: [] };
    Object.defineProperty(window, "__zoenTestCanvasFocus", { value: state });
    let contact;
    for (const type of ["pointerdown", "touchstart", "pointerup", "touchend"])
      window.addEventListener(
        type,
        (event) => {
          contact = event;
        },
        { capture: true, passive: true },
      );
    const original = HTMLElement.prototype.focus;
    HTMLElement.prototype.focus = function (options) {
      const inDispatch = contact?.isTrusted && contact.eventPhase !== Event.NONE;
      const phase = !inDispatch
        ? "idle"
        : contact.type === "pointerup" || contact.type === "touchend"
          ? "release"
          : "press";
      const canvas = this.id === id;
      const input =
        this.tagName === "INPUT" && this.type === "text" && this.previousElementSibling?.id === id;
      if (canvas || input) state.calls.push({ role: canvas ? "canvas" : "input", phase });
      if (canvas) {
        state.denied++;
        return;
      }
      return original.call(this, options);
    };
  }, canvasId);
}

export async function assertCanvasGestureTextAdmission(page) {
  const child = page.frames().find((frame) => frame.parentFrame());
  assert.ok(child, "real editor child frame must exist");
  const state = await child.evaluate(() => window.__zoenTestCanvasFocus);
  assert.ok(state?.denied > 0, "the actual canvas must experience focus refusal");
  assert.ok(
    state.calls.some(({ role, phase }) => role === "input" && phase === "release"),
    "current text must receive focus within the trusted release, even without synchronous canvas focus",
  );
  return state;
}
