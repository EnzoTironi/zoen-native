import assert from 'node:assert/strict';
import { test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { binaryEditorFormat } from './binary-formats.ts';
import { parseRuntimePackage, type RuntimePackage } from './runtime-package.ts';
import { runtimeSupportScript } from './vendor/runtime-support.ts';
type Client = { load(): Promise<unknown>; dispose(): void };
function generatedConsumer() {
  const context = { TextEncoder, TextDecoder, Uint8Array, URL, AbortController, Blob, console };
  runInNewContext(runtimeSupportScript, context);
  return (
    context as typeof context & {
      __zoenRuntimeSupport: {
        installRuntimeClient(input: RuntimePackage, target: Window): Client;
      };
    }
  ).__zoenRuntimeSupport;
}
function runtime(editor: 'deckcraft' | 'gridcraft' | 'wordcraft') {
  const format = binaryEditorFormat(editor);
  const root = `https://code.example.test/${editor}/${format.engineRevision}/`;
  return parseRuntimePackage({
    schema: 1,
    engine: editor,
    revision: format.engineRevision,
    abi: 'zoen-host-v1',
    module: { url: root + 'engine.js', bytes: 1, sha256: 'a'.repeat(64), mime: 'text/javascript' },
    wasm: { url: root + 'engine.wasm', bytes: 8, sha256: 'b'.repeat(64), mime: 'application/wasm' },
  });
}
function target() {
  const events = new Map<string, EventListener>();
  const messages: unknown[] = [];
  const window = {
    parent: { postMessage: (message: unknown) => messages.push(message) },
    addEventListener: (name: string, listener: EventListener) => events.set(name, listener),
    removeEventListener: (name: string) => events.delete(name),
  } as unknown as Window;
  return { window, events, messages };
}
// Execute the generated consumer injected by sandboxHtml, not another import of
// the authored parser. This proves ABI admission/lifecycle, never WASM execution.
for (const editor of ['deckcraft', 'gridcraft', 'wordcraft'] as const) {
  test(`generated sandbox loader admits only its ${editor} package and retires pending load`, async () => {
    const consumer = generatedConsumer();
    const fixture = target();
    const client = consumer.installRuntimeClient(runtime(editor), fixture.window);
    assert.equal(fixture.messages.length, 0, 'installation does not fetch/start the editor');
    const pending = client.load();
    assert.equal(client.load(), pending, 'one pending load, never a second request');
    assert.equal(fixture.messages.length, 1);
    assert.equal(JSON.stringify(fixture.messages[0]), '{"zoenRuntime":1,"kind":"load"}');
    const refused = assert.rejects(pending, /retired/);
    client.dispose();
    await refused;
    assert.equal(fixture.events.size, 0);
    assert.throws(() => client.load(), /retired/);
    assert.equal(fixture.messages.length, 1, 'retirement cannot load a replacement editor');
  });
}
test('generated consumer refuses unknown engine, extra authority and oversized code before installing callbacks', () => {
  const consumer = generatedConsumer();
  const valid = runtime('wordcraft');
  for (const invalid of [
    { ...valid, engine: 'other' },
    { ...valid, extra: true },
    { ...valid, wasm: { ...valid.wasm, bytes: 48 * 1024 * 1024 + 1 } },
    { ...valid, module: { ...valid.module, url: valid.module.url + '?document=private' } },
  ]) {
    const fixture = target();
    assert.throws(() => consumer.installRuntimeClient(invalid as RuntimePackage, fixture.window));
    assert.equal(fixture.events.size, 0);
    assert.equal(fixture.messages.length, 0);
  }
});
