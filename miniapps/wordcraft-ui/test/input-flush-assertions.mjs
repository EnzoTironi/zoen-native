import assert from "node:assert/strict";

/** Regression against the genuine browser engine and its actual hidden input.
 * Call after an observed trusted canvas touch. This deliberately issues the
 * public host pause in the last trusted input's bubble phase: TextAgent has
 * queued the character, and no next animation frame can precede this request.
 * It does not inject an event, set a value, force focus or replace the renderer.
 */
export async function pauseOnLastTypedCharacter(page, prefix, lastCharacter) {
  assert.equal([...lastCharacter].length, 1);
  const child = page.frames().find((frame) => frame.parentFrame());
  assert.ok(child, "actual opaque App frame must exist");
  await page.keyboard.type(prefix);
  await child.evaluate(async (last) => {
    const engine = await window.__zoenRuntime.load();
    window.inputPause = { events: [] };
    document.addEventListener("input", (event) => {
      window.inputPause.events.push({
        trusted: event.isTrusted,
        data: event.data,
        inputType: event.inputType,
        composing: event.isComposing,
      });
      if (event.isTrusted && event.data === last) {
        window.inputPause.promise = engine.zoenHostRequest(
          JSON.stringify({ op: "freeze", input: { frozen: true } }),
          new Uint8Array(),
        );
      }
    }, { once: true });
  }, lastCharacter);
  await page.keyboard.type(lastCharacter);
  const observation = await child.evaluate(async () => {
    if (!window.inputPause.promise) throw Error("last trusted input did not request pause");
    const reply = await window.inputPause.promise;
    return { events: window.inputPause.events, status: JSON.parse(reply.metadata) };
  });
  assert.deepEqual(observation.events, [{
    trusted: true, data: lastCharacter, inputType: "insertText", composing: false,
  }]);
  assert.equal(observation.status.frozen, true);
  return observation;
}
